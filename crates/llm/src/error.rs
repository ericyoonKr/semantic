#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LlmError {
    #[error("model call timed out")]
    Timeout,
    #[error("model server rate limited")]
    RateLimited,
    #[error("model server returned status {0}")]
    Http(u16),
    #[error("model output could not be parsed")]
    ParseFailed,
    #[error("model call cancelled")]
    Cancelled,
    #[error("model server unavailable")]
    Unavailable,
    #[error("model server rejected the key or permission (status {0})")]
    Auth(u16),
    #[error("model output was blocked: {0}")]
    Blocked(String),
    #[error("model output stopped at the token limit")]
    Truncated,
}

impl From<crate::vertex::VertexError> for LlmError {
    fn from(e: crate::vertex::VertexError) -> Self {
        use crate::vertex::VertexError as V;
        match e {
            V::Timeout => LlmError::Timeout,
            V::Connect => LlmError::Unavailable,
            V::RateLimited { .. } => LlmError::RateLimited,
            V::Auth { code, .. } => LlmError::Auth(code),
            V::BadRequest { code, .. } | V::Server { code, .. } => LlmError::Http(code),
            V::Blocked(reason) => LlmError::Blocked(reason),
            V::Truncated => LlmError::Truncated,
            V::Malformed(_) => LlmError::ParseFailed,
        }
    }
}

impl LlmError {
    pub(crate) fn is_retryable(&self) -> bool {
        matches!(self, LlmError::RateLimited | LlmError::Unavailable)
            || matches!(self, LlmError::Http(code) if *code >= 500)
    }
}
