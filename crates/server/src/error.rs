use std::borrow::Cow;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use kg::KgError;
use parsers::ParseError;
use sessdb::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required settings: {0}")]
    Missing(String),
    #[error("setting {0} is not valid")]
    Invalid(&'static str),
    #[error("BIND_ADDR must be a loopback address")]
    NotLoopback,
    #[error("{0} must be shorter than {1}")]
    DeadlineOrder(&'static str, &'static str),
}

#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    #[error("requirement tree bundle could not be read: {0}")]
    Io(#[from] std::io::Error),
    #[error("requirement tree manifest is not valid")]
    Manifest,
    #[error("requirement tree {0} did not pass the knowledge graph checks")]
    NotVerified(String),
    #[error("requirement tree {0} does not match its sha256")]
    Sha256(String),
    #[error("requirement tree {0} has a bad line {1}")]
    Line(String, usize),
    #[error("requirement tree {0} failed the engine checks: {1}")]
    Graph(String, engine::EngineError),
}

#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("{0}")]
    Config(#[from] ConfigError),
    #[error("data folder could not be prepared: {0}")]
    Io(#[from] std::io::Error),
    #[error("session store could not be opened: {0}")]
    Storage(#[from] StorageError),
    #[error("cpu pool could not be built")]
    CpuPool,
    #[error("log output could not be prepared")]
    Logging,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorReason(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub reason: Cow<'static, str>,
}

impl ApiError {
    pub fn new(status: StatusCode, reason: impl Into<Cow<'static, str>>) -> Self {
        Self {
            status,
            reason: reason.into(),
        }
    }

    pub fn bad_request(reason: impl Into<Cow<'static, str>>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, reason)
    }

    pub fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "거부")
    }

    pub fn busy() -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "이 세션은 처리 중입니다")
    }

    pub fn internal() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "내부 오류")
    }

    pub fn closed() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "세션이 닫혔습니다 — 다시 시도해 주세요",
        )
    }

    pub fn timeout() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "처리 시간 제한을 넘었습니다",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let reason = self.reason.into_owned();
        let mut res = (
            self.status,
            Json(serde_json::json!({ "error": reason.as_str() })),
        )
            .into_response();
        res.extensions_mut().insert(ErrorReason(reason));
        res
    }
}

impl From<StorageError> for ApiError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::Key(what) => Self::bad_request(format!("인자 오류: {what}")),
            other => {
                tracing::error!(error = %other, "session store failed");
                Self::internal()
            }
        }
    }
}

impl From<ParseError> for ApiError {
    fn from(e: ParseError) -> Self {
        let status = match e {
            ParseError::Unsupported(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ParseError::TooLarge | ParseError::ZipBomb => StatusCode::PAYLOAD_TOO_LARGE,
            ParseError::Corrupted
            | ParseError::Encrypted
            | ParseError::EmptyText
            | ParseError::Timeout => StatusCode::BAD_REQUEST,
        };
        Self::new(status, format!("추출 불가: {e}"))
    }
}

impl From<KgError> for ApiError {
    fn from(e: KgError) -> Self {
        match e {
            KgError::UnknownGraph => Self::bad_request("알 수 없는 그래프입니다"),
            KgError::UnknownNode => Self::bad_request("알 수 없는 노드입니다"),
            KgError::RelationNotAllowed => Self::bad_request("그래프 계약에 없는 관계입니다"),
            KgError::Argument(what) => Self::bad_request(format!("인자 오류: {what}")),
            other => {
                tracing::error!(error = %other, "knowledge graph query failed");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, "지식그래프 읽기 실패")
            }
        }
    }
}
