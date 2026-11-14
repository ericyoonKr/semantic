use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("password does not meet the policy")]
    WeakPassword,
    #[error("login failed")]
    LoginFailed,
    #[error("account locked")]
    Locked,
    #[error("not signed in")]
    Unauthenticated,
    #[error("csrf token mismatch")]
    Csrf,
    #[error("not found")]
    NotFound,
    #[error("too many requests")]
    TooMany,
    #[error("thread is busy")]
    Busy,
    #[error("nothing to retry")]
    NothingToRetry,
    #[error("temporarily unavailable")]
    Unavailable,
    #[error("bad request")]
    BadRequest,
    #[error("missing or invalid settings: {0}")]
    Config(String),
    #[error("upstream unavailable")]
    Upstream,
    #[error("internal error")]
    Internal,
}

impl GatewayError {
    fn status_and_text(&self) -> (StatusCode, &'static str) {
        match self {
            GatewayError::WeakPassword => (StatusCode::BAD_REQUEST, "비밀번호는 12자 이상이어야 합니다"),
            GatewayError::LoginFailed | GatewayError::Locked => (StatusCode::UNAUTHORIZED, "아이디 또는 비밀번호가 맞지 않습니다"),
            GatewayError::Unauthenticated => (StatusCode::UNAUTHORIZED, "로그인이 필요합니다"),
            GatewayError::Csrf => (StatusCode::FORBIDDEN, "요청을 처리할 수 없습니다"),
            GatewayError::NotFound => (StatusCode::NOT_FOUND, "없는 대화입니다"),
            GatewayError::TooMany => (StatusCode::TOO_MANY_REQUESTS, "잠시 뒤 다시 시도해 주세요"),
            GatewayError::Busy => (StatusCode::CONFLICT, "앞 질문의 답을 기다리다 시간이 지났습니다. 다시 보내 주세요"),
            GatewayError::NothingToRetry => (StatusCode::NOT_FOUND, "다시 답할 질문이 없습니다"),
            GatewayError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "잠시 뒤 다시 시도해 주세요"),
            GatewayError::BadRequest => (StatusCode::BAD_REQUEST, "요청을 처리할 수 없습니다"),
            GatewayError::Upstream => (StatusCode::BAD_GATEWAY, "지금은 답할 수 없습니다. 잠시 뒤 다시 시도해 주세요"),
            GatewayError::Config(_) | GatewayError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "처리하지 못했습니다"),
        }
    }
}

impl GatewayError {
    pub fn message(&self) -> &'static str {
        self.status_and_text().1
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let (status, text) = self.status_and_text();
        (status, Json(json!({ "message": text }))).into_response()
    }
}
