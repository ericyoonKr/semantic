use axum::Json;
use axum::extract::multipart::{Field, MultipartError};
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use parsers::{ParseError, RESAVE_GUIDANCE};
use serde::Serialize;
use tokio::sync::oneshot;

use crate::error::ApiError;
use crate::session::{SessionEvent, SessionId, send};
use crate::state::AppState;
use crate::tasks;
use crate::upload::filename;
use crate::upload::{FileMeta, IncomingFile, UploadReply};

const FILE_FIELD: &str = "file";
const MAGICS: [&[u8]; 4] = [
    b"%PDF-",
    b"PK\x03\x04",
    b"PK\x05\x06",
    &[0xD0, 0xCF, 0x11, 0xE0],
];
const MAGIC_LEN: usize = 5;

#[derive(Debug, Serialize)]
pub struct FileList {
    pub files: Vec<FileMeta>,
}

fn multipart_error(e: MultipartError) -> ApiError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        ApiError::from(ParseError::TooLarge)
    } else {
        ApiError::bad_request("multipart 본문을 읽지 못했습니다")
    }
}

fn magic_ok(head: &[u8]) -> bool {
    MAGICS
        .iter()
        .any(|m| head.starts_with(m) || m.starts_with(head))
}

async fn read_field(mut field: Field<'_>, max_bytes: u64) -> Result<Vec<u8>, ApiError> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut checked = false;
    while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
        if (bytes.len() + chunk.len()) as u64 > max_bytes {
            return Err(ParseError::TooLarge.into());
        }
        bytes.extend_from_slice(&chunk);
        if !checked && bytes.len() >= MAGIC_LEN {
            if !magic_ok(&bytes[..MAGIC_LEN]) {
                return Err(ParseError::Unsupported(RESAVE_GUIDANCE.to_string()).into());
            }
            checked = true;
        }
    }
    if !magic_ok(&bytes) || bytes.is_empty() {
        return Err(ParseError::Unsupported(RESAVE_GUIDANCE.to_string()).into());
    }
    Ok(bytes)
}

async fn read_file(multipart: &mut Multipart, state: &AppState) -> Result<IncomingFile, ApiError> {
    let limits = &state.cfg.limits;
    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() != Some(FILE_FIELD) && field.file_name().is_none() {
            continue;
        }
        let raw_name = field.file_name().unwrap_or_default().to_string();
        let filename = filename::clean(&raw_name, limits.filename_chars)
            .ok_or_else(|| ApiError::bad_request("파일 이름이 없습니다"))?;
        let bytes = read_field(field, limits.guard.max_file_bytes).await?;
        return Ok(IncomingFile { filename, bytes });
    }
    Err(ApiError::bad_request("file 칸이 없습니다"))
}

pub async fn upload(
    State(state): State<AppState>,
    Path(sid): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<UploadReply>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let file = read_file(&mut multipart, &state).await?;
    let (reply, rx) = oneshot::channel();
    let event = SessionEvent::Upload {
        file,
        reply,
        span: tracing::Span::current(),
    };
    send(&state, &sid, event)?;
    match tokio::time::timeout(state.cfg.limits.upload_deadline, rx).await {
        Ok(Ok(out)) => out.map(Json),
        Ok(Err(_)) => Err(ApiError::closed()),
        Err(_) => Err(ApiError::timeout()),
    }
}

pub async fn list(
    State(state): State<AppState>,
    Path(sid): Path<String>,
) -> Result<Json<FileList>, ApiError> {
    let sid = SessionId::parse(&sid)?;
    let db = state.db.clone();
    let session = sid.as_str().to_string();
    let stored = tasks::blocking(move || db.list_files(&session)).await??;
    let mut files: Vec<FileMeta> = stored
        .iter()
        .filter_map(|f| serde_json::from_slice(&f.meta).ok())
        .collect();
    files.sort_by(|a, b| {
        a.uploaded_at
            .cmp(&b.uploaded_at)
            .then_with(|| a.file_id.cmp(&b.file_id))
    });
    Ok(Json(FileList { files }))
}
