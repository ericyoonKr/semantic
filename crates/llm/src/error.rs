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
}

impl LlmError {
    pub(crate) fn is_retryable(&self) -> bool {
        matches!(self, LlmError::RateLimited | LlmError::Unavailable)
            || matches!(self, LlmError::Http(code) if *code >= 500)
    }
}
