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
    let (doc, format) = match tasks::cpu(&state.cpu, move || parse(bytes, &guard)).await?? {
        Parsed::Ready(doc, format) => (doc, format),
        Parsed::Pdf(bytes) => (
            state.pdf.read(bytes, limits.guard.clone()).await?,
            Format::Pdf,
        ),
    };
    let prepared =
        tasks::cpu(&state.cpu, move || prepare(filename, doc, format, &limits)).await??;
    let db = state.db.clone();
    let session = sid.as_str().to_string();
    let prepared = tasks::blocking(move || {
        db.put_upload(&session, &prepared.reply.file_id, &prepared.parts())
            .map(|()| prepared)
    })
    .await??;
    tracing::info!(
        file_id = %prepared.reply.file_id,
        format = %prepared.reply.format,
        bytes = size,
        pages = ?prepared.reply.pages,
        chars = prepared.reply.chars,
        "upload stored"
    );
    Ok(prepared.reply)
}

enum Parsed {
    Ready(Document, Format),
    Pdf(Vec<u8>),
}

fn parse(bytes: Vec<u8>, guard: &GuardLimits) -> Result<Parsed, ApiError> {
    match parsers::parse(&bytes, guard) {
        Ok(doc) => Ok(Parsed::Ready(doc, parsers::detect(&bytes)?)),
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
    limits: &Limits,
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
    };
    let to_vec = |r: serde_json::Result<Vec<u8>>| r.map_err(|_| ApiError::internal());
    Ok(Prepared {
        meta: to_vec(serde_json::to_vec(&FileMeta::of(&reply, now_secs())))?,
        model: to_vec(serde_json::to_vec(&doc))?,
        spans: to_vec(serde_json::to_vec(&spans))?,
        findings: to_vec(serde_json::to_vec(&reply.findings))?,
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
