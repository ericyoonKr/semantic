use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::thread;

use docmodel::{Block, Document};
use parsers::{GuardLimits, PDF_PENDING, ParseError, PdfReader};
use tokio::sync::{mpsc, oneshot};

use crate::error::ApiError;

type Reply = Result<Document, ParseError>;

struct Job {
    bytes: Vec<u8>,
    limits: GuardLimits,
    reply: oneshot::Sender<Result<Reply, ApiError>>,
}

pub struct PdfWorker(Option<mpsc::Sender<Job>>);

fn unavailable() -> ParseError {
    ParseError::Unsupported(PDF_PENDING.to_string())
}

impl PdfWorker {
    pub fn start(library_dir: Option<&Path>, queue: usize) -> Self {
        let Some(dir) = library_dir else {
            tracing::warn!("PDFIUM_DIR is not set; PDF uploads will be refused");
            return Self(None);
        };
        let (tx, rx) = mpsc::channel(queue.max(1));
        let dir: PathBuf = dir.to_path_buf();
        match thread::Builder::new()
            .name("pdfium".to_string())
            .spawn(move || serve(&dir, rx))
        {
            Ok(_) => Self(Some(tx)),
            Err(e) => {
                tracing::error!(error = %e, "pdfium thread could not start; PDF uploads will be refused");
                Self(None)
            }
        }
    }

    pub async fn read(&self, bytes: Vec<u8>, limits: GuardLimits) -> Result<Document, ApiError> {
        let Some(tx) = &self.0 else {
            return Err(unavailable().into());
        };
        let (reply, rx) = oneshot::channel();
        let job = Job {
            bytes,
            limits,
            reply,
        };
        if tx.send(job).await.is_err() {
            return Err(unavailable().into());
        }
        match rx.await {
            Ok(Ok(out)) => out.map_err(ApiError::from),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(unavailable().into()),
        }
    }
}

fn serve(dir: &Path, mut rx: mpsc::Receiver<Job>) {
    let reader = match PdfReader::load(dir) {
        Ok(reader) => reader,
        Err(_) => {
            tracing::warn!("pdfium library could not be loaded; PDF uploads will be refused");
            return;
        }
    };
    tracing::info!("pdf reading ready");
    while let Some(job) = rx.blocking_recv() {
        let out = catch_unwind(AssertUnwindSafe(|| read_checked(&reader, &job.bytes, &job.limits)));
        drop(job.bytes);
        let out = out.map_err(|_| {
            tracing::error!("pdf task panicked");
            ApiError::internal()
        });
        let _ = job.reply.send(out);
    }
}

fn read_checked(reader: &PdfReader, bytes: &[u8], limits: &GuardLimits) -> Reply {
    if bytes.len() as u64 > limits.max_file_bytes {
        return Err(ParseError::TooLarge);
    }
    let doc = reader.read(bytes, parsers::file_id(bytes))?;
    check_document(&doc, limits)?;
    Ok(doc)
}

fn check_document(doc: &Document, limits: &GuardLimits) -> Result<(), ParseError> {
    let mut chars: u64 = 0;
    let mut visible = false;
    let mut max_page: u32 = 0;
    for block in &doc.blocks {
        let (texts, pages): (Vec<&str>, Vec<Option<u32>>) = match block {
            Block::Heading { text, anchor, .. } | Block::Para { text, anchor } => {
                (vec![text.as_str()], vec![anchor.page])
            }
            Block::Table(t) => (
                t.cells.iter().map(|c| c.text.as_str()).collect(),
                t.cells
                    .iter()
                    .map(|c| c.anchor.page)
                    .chain([t.anchor.page])
                    .collect(),
            ),
        };
        for text in texts {
            chars += text.chars().count() as u64;
            visible |= text.chars().any(|c| !c.is_whitespace());
        }
        max_page = pages.into_iter().flatten().fold(max_page, u32::max);
    }
    if chars > limits.max_chars || max_page > limits.max_pages {
        return Err(ParseError::TooLarge);
    }
    if !visible {
        return Err(ParseError::EmptyText);
    }
    Ok(())
}
