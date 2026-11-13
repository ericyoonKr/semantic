use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::thread;

use docmodel::Document;
use parsers::{GuardLimits, Ocr, PDF_PENDING, ParseError, PdfReader, check_document, check_size, log_load_failure};
use tokio::sync::{mpsc, oneshot};

use crate::error::ApiError;

type Reply = Result<Document, ParseError>;

struct Job {
    bytes: Vec<u8>,
    limits: GuardLimits,
    recognize: bool,
    reply: oneshot::Sender<Result<Reply, ApiError>>,
}

pub struct PdfWorker(Option<mpsc::Sender<Job>>, bool);

fn unavailable() -> ParseError {
    ParseError::Unsupported(PDF_PENDING.to_string())
}

impl PdfWorker {
    pub fn start(library_dir: Option<&Path>, ocr_dir: Option<&Path>, ocr_threads: usize, queue: usize) -> Self {
        let Some(dir) = library_dir else {
            tracing::warn!("PDFIUM_DIR is not set; PDF uploads will be refused");
            return Self(None, false);
        };
        let (tx, rx) = mpsc::channel(queue.max(1));
        let dir: PathBuf = dir.to_path_buf();
        let recognizes = ocr_dir.is_some();
        let ocr_dir: Option<PathBuf> = ocr_dir.map(Path::to_path_buf);
        match thread::Builder::new()
            .name("pdfium".to_string())
            .spawn(move || serve(&dir, ocr_dir.as_deref(), ocr_threads, rx))
        {
            Ok(_) => Self(Some(tx), recognizes),
            Err(e) => {
                tracing::error!(error = %e, "pdfium thread could not start; PDF uploads will be refused");
                Self(None, false)
            }
        }
    }

    pub fn recognizes(&self) -> bool {
        self.1
    }

    pub async fn read(&self, bytes: Vec<u8>, limits: GuardLimits) -> Result<Document, ApiError> {
        self.send(bytes, limits, true).await
    }

    pub async fn read_quick(&self, bytes: Vec<u8>, limits: GuardLimits) -> Result<Document, ApiError> {
        self.send(bytes, limits, false).await
    }

    async fn send(&self, bytes: Vec<u8>, limits: GuardLimits, recognize: bool) -> Result<Document, ApiError> {
        let Some(tx) = &self.0 else {
            return Err(unavailable().into());
        };
        let (reply, rx) = oneshot::channel();
        let job = Job {
            bytes,
            limits,
            recognize,
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

fn with_ocr(reader: PdfReader, ocr_dir: Option<&Path>, threads: usize) -> PdfReader {
    let Some(dir) = ocr_dir else {
        tracing::info!("OCR_DIR is not set; pages without text stay unextracted");
        return reader;
    };
    match Ocr::load(dir, threads) {
        Ok(ocr) => {
            tracing::info!("text recognition ready");
            reader.with_ocr(ocr)
        }
        Err(e) => {
            log_load_failure(&e, dir);
            reader
        }
    }
}

fn serve(dir: &Path, ocr_dir: Option<&Path>, ocr_threads: usize, mut rx: mpsc::Receiver<Job>) {
    let reader = match PdfReader::load(dir) {
        Ok(reader) => reader,
        Err(_) => {
            tracing::warn!("pdfium library could not be loaded; PDF uploads will be refused");
            return;
        }
    };
    let reader = with_ocr(reader, ocr_dir, ocr_threads);
    tracing::info!("pdf reading ready");
    while let Some(job) = rx.blocking_recv() {
        let out = catch_unwind(AssertUnwindSafe(|| read_checked(&reader, &job.bytes, &job.limits, job.recognize)));
        drop(job.bytes);
        let out = out.map_err(|_| {
            tracing::error!("pdf task panicked");
            ApiError::internal()
        });
        let _ = job.reply.send(out);
    }
}

fn read_checked(reader: &PdfReader, bytes: &[u8], limits: &GuardLimits, recognize: bool) -> Reply {
    check_size(bytes, limits)?;
    let id = parsers::file_id(bytes);
    let doc = if recognize { reader.read(bytes, id)? } else { reader.read_quick(bytes, id)? };
    match check_document(&doc, limits) {
        Err(ParseError::EmptyText) if !recognize => {}
        other => other?,
    }
    Ok(doc)
}
