use std::time::Duration;

use serde_json::Value;

const MESSAGE_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VertexError {
    #[error("vertex call timed out")]
    Timeout,
    #[error("vertex connection failed")]
    Connect,
    #[error("vertex rate limited ({status}): {message}")]
    RateLimited { status: String, message: String, retry_after: Option<Duration> },
    #[error("vertex rejected the key or permission ({code} {status}): {message}")]
    Auth { code: u16, status: String, message: String },
    #[error("vertex rejected the request ({code} {status}): {message}")]
    BadRequest { code: u16, status: String, message: String },
    #[error("vertex server error ({code} {status}): {message}")]
    Server { code: u16, status: String, message: String },
    #[error("vertex blocked the output: {0}")]
    Blocked(String),
    #[error("vertex output stopped at the token limit")]
    Truncated,
    #[error("vertex reply could not be read: {0}")]
    Malformed(String),
}

impl VertexError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, VertexError::Timeout | VertexError::Connect | VertexError::RateLimited { .. } | VertexError::Server { .. })
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            VertexError::RateLimited { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    pub fn is_fatal(&self) -> bool {
        matches!(self, VertexError::Auth { .. })
    }
}

fn short(s: &str) -> String {
    s.chars().take(MESSAGE_CHARS).collect()
}

pub fn from_send(e: &reqwest::Error) -> VertexError {
    if e.is_timeout() { VertexError::Timeout } else { VertexError::Connect }
}

pub fn retry_after_header(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

pub fn from_status(code: u16, body: &str, retry_after: Option<Duration>) -> VertexError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let err = parsed.as_ref().and_then(|v| v.get("error"));
    let status = err.and_then(|e| e.get("status")).and_then(Value::as_str).unwrap_or("").to_string();
    let message = short(err.and_then(|e| e.get("message")).and_then(Value::as_str).unwrap_or(body));
    match code {
        429 => VertexError::RateLimited { status, message, retry_after },
        401 | 403 => VertexError::Auth { code, status, message },
        500..=599 => VertexError::Server { code, status, message },
        _ if status == "RESOURCE_EXHAUSTED" => VertexError::RateLimited { status, message, retry_after },
        _ => VertexError::BadRequest { code, status, message },
    }
}

pub fn log(error: &VertexError, model: &str, op: &str, attempt: u32) {
    if error.is_fatal() {
        tracing::error!(%error, model, op, attempt, "vertex call failed; check VERTEX api key and permissions");
    } else if error.is_retryable() {
        tracing::warn!(%error, model, op, attempt, "vertex call failed; retrying");
    } else {
        tracing::warn!(%error, model, op, attempt, "vertex call failed");
    }
}

#[cfg(test)]
mod tests {
    use super::{VertexError, from_status};
    use std::time::Duration;

    #[test]
    fn statuses_map_to_retry_classes() {
        let body = r#"{"error":{"code":429,"message":"Quota exceeded","status":"RESOURCE_EXHAUSTED"}}"#;
        let e = from_status(429, body, Some(Duration::from_secs(7)));
        assert!(e.is_retryable());
        assert_eq!(e.retry_after(), Some(Duration::from_secs(7)));
        let auth = from_status(403, r#"{"error":{"code":403,"message":"denied","status":"PERMISSION_DENIED"}}"#, None);
        assert!(auth.is_fatal() && !auth.is_retryable());
        assert!(matches!(from_status(400, "not json", None), VertexError::BadRequest { code: 400, .. }));
        assert!(from_status(503, "", None).is_retryable());
    }
}
