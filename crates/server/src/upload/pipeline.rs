use std::time::{SystemTime, UNIX_EPOCH};

use docmodel::{Block, Document};
use parsers::{Format, GuardLimits, ParseError};
use sessdb::UploadParts;

use crate::config::Limits;
use crate::error::ApiError;
use crate::session::SessionId;
use crate::state::AppState;
use crate::tasks;
use crate::upload::findings;
use crate::upload::record::{FileMeta, Finding, UploadReply, describe_warning};

pub struct IncomingFile {
    pub filename: String,
    pub bytes: Vec<u8>,
}

struct Prepared {
    reply: UploadReply,
    meta: Vec<u8>,
    model: Vec<u8>,
    spans: Vec<u8>,
    findings: Vec<u8>,
    original: Vec<u8>,
}

impl Prepared {
    fn parts(&self) -> UploadParts<'_> {
        UploadParts {
            meta: &self.meta,
            doc: &self.model,
            paged_text: self.reply.text.as_bytes(),
            span_map: &self.spans,
            findings: &self.findings,
            vectors: None,
            original: &self.original,
        }
    }
}

pub async fn run(
    state: &AppState,
    sid: &SessionId,
    file: IncomingFile,
) -> Result<UploadReply, ApiError> {
    let _permit = state
        .uploads
        .acquire()
        .await
        .map_err(|_| ApiError::closed())?;
    let limits = state.cfg.limits.clone();
    let size = file.bytes.len();
    let IncomingFile { filename, bytes } = file;
    let guard = limits.guard.clone();
    let (doc, format, original) = match tasks::cpu(&state.cpu, move || parse(bytes, &guard)).await?? {
        Parsed::Ready(doc, format, original) => (doc, format, original),
        Parsed::Pdf(bytes) => {
            let original = bytes.clone();
            (state.pdf.read_quick(bytes, limits.guard.clone()).await?, Format::Pdf, original)
        }
    };
    let pending = format == Format::Pdf && state.pdf.recognizes() && unextracted(&doc);
    let uploaded_at = now_secs();
    let later = pending.then(|| (filename.clone(), original.clone()));
    let prepared = tasks::cpu(&state.cpu, move || prepare(filename, doc, format, original, &limits, pending, uploaded_at)).await??;
    let prepared = store(state, sid.as_str(), prepared).await?;
    tracing::info!(
        file_id = %prepared.reply.file_id,
        format = %prepared.reply.format,
        bytes = size,
        pages = ?prepared.reply.pages,
        chars = prepared.reply.chars,
        pending,
        "upload stored"
    );
    let state_bg = state.clone();
    let session = sid.as_str().to_string();
    let file_id = prepared.reply.file_id.clone();
    state.tracker.clone().spawn(async move {
        if let Some((filename, original)) = later {
            recognize_later(state_bg.clone(), session.clone(), filename, original, uploaded_at).await;
        }
        crate::funcall::docindex::build_index(&state_bg, &session, &file_id).await;
    });
    Ok(prepared.reply)
}

async fn store(state: &AppState, session: &str, prepared: Prepared) -> Result<Prepared, ApiError> {
    let db = state.db.clone();
    let session = session.to_string();
    tasks::blocking(move || db.put_upload(&session, &prepared.reply.file_id, &prepared.parts()).map(|()| prepared)).await?.map_err(ApiError::from)
}

fn unextracted(doc: &Document) -> bool {
    doc.warnings.iter().any(|w| matches!(w, docmodel::ExtractWarning::PagesUnextracted { .. }))
}

fn still_there(state: &AppState, session: &str, file_id: &str) -> bool {
    sessdb::doc_key(session, file_id, sessdb::Part::Meta).ok().and_then(|key| state.db.get_state(&key).ok().flatten()).is_some()
}

async fn recognize_later(state: AppState, session: String, filename: String, original: Vec<u8>, uploaded_at: u64) {
    let started = std::time::Instant::now();
    let limits = state.cfg.limits.clone();
    let doc = match state.pdf.read(original.clone(), limits.guard.clone()).await {
        Ok(doc) => doc,
        Err(error) => {
            tracing::warn!(?error, "upload.recognize_failed");
            return;
        }
    };
    let prepared = match tasks::cpu(&state.cpu, move || prepare(filename, doc, Format::Pdf, original, &limits, false, uploaded_at)).await {
        Ok(Ok(p)) => p,
        _ => {
            tracing::warn!("upload.recognize_prepare_failed");
            return;
        }
    };
    if !still_there(&state, &session, &prepared.reply.file_id) {
        tracing::info!("upload.recognize_dropped session_gone");
        return;
    }
    match store(&state, &session, prepared).await {
        Ok(p) => tracing::info!(file_id = %p.reply.file_id, pages = ?p.reply.pages, chars = p.reply.chars, elapsed_ms = started.elapsed().as_millis() as u64, "upload recognized"),
        Err(error) => tracing::warn!(?error, "upload.recognize_store_failed"),
    }
}

enum Parsed {
    Ready(Document, Format, Vec<u8>),
    Pdf(Vec<u8>),
}

fn parse(bytes: Vec<u8>, guard: &GuardLimits) -> Result<Parsed, ApiError> {
    match parsers::parse(&bytes, guard) {
        Ok(doc) => {
            let format = parsers::detect(&bytes)?;
            Ok(Parsed::Ready(doc, format, bytes))
        }
        Err(ParseError::Unsupported(_)) if parsers::detect(&bytes) == Ok(Format::Pdf) => {
            Ok(Parsed::Pdf(bytes))
        }
        Err(e) => Err(e.into()),
    }
}

fn prepare(
    filename: String,
    doc: Document,
    format: Format,
    original: Vec<u8>,
    limits: &Limits,
    processing: bool,
    uploaded_at: u64,
) -> Result<Prepared, ApiError> {
    let (text, spans) = doc.render_paged();
    let findings: Vec<Finding> = findings::of(&doc);
    let reply = UploadReply {
        file_id: doc.source.0.clone(),
        filename,
        format: format_name(format).to_string(),
        pages: last_page(&doc),
        chars: text.chars().count() as u64,
        outline: doc.outline(limits.outline_level, limits.outline_lines),
        text,
        warnings: doc.warnings.iter().map(describe_warning).collect(),
        findings,
        processing,
    };
    let to_vec = |r: serde_json::Result<Vec<u8>>| r.map_err(|_| ApiError::internal());
    Ok(Prepared {
        meta: to_vec(serde_json::to_vec(&FileMeta::of(&reply, uploaded_at)))?,
        model: to_vec(serde_json::to_vec(&doc))?,
        spans: to_vec(serde_json::to_vec(&spans))?,
        findings: to_vec(serde_json::to_vec(&reply.findings))?,
        original,
        reply,
    })
}

fn format_name(f: Format) -> &'static str {
    match f {
        Format::Docx => "docx",
        Format::Hwpx => "hwpx",
        Format::Pdf => "pdf",
    }
}

fn last_page(doc: &Document) -> Option<u32> {
    doc.blocks
        .iter()
        .flat_map(|b| match b {
            Block::Heading { anchor, .. } | Block::Para { anchor, .. } => vec![anchor.page],
            Block::Table(t) => std::iter::once(t.anchor.page)
                .chain(t.cells.iter().map(|c| c.anchor.page))
                .collect(),
        })
        .flatten()
        .max()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
