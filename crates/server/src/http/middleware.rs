use std::any::Any;
use std::time::Instant;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE, SERVER};
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::Instrument;

use crate::error::{ApiError, ErrorReason};
use crate::state::AppState;

pub const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");
pub const INTERNAL_KEY: HeaderName = HeaderName::from_static("x-internal-key");
const POWERED_BY: HeaderName = HeaderName::from_static("x-powered-by");
const MAX_REQUEST_ID: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(pub String);

fn incoming_id(req: &Request) -> Option<String> {
    let raw = req.headers().get(&REQUEST_ID)?.to_str().ok()?.trim();
    let ok =
        !raw.is_empty() && raw.len() <= MAX_REQUEST_ID && raw.bytes().all(|b| b.is_ascii_graphic());
    ok.then(|| raw.to_string())
}

pub async fn request_id(mut req: Request, next: Next) -> Response {
    let id = incoming_id(&req).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    req.extensions_mut().insert(RequestId(id.clone()));
    let span = tracing::info_span!(
        "request",
        request_id = %id,
        method = %req.method(),
        path = %req.uri().path()
    );
    let started = Instant::now();
    let res = next.run(req).instrument(span.clone()).await;
    span.in_scope(|| {
        tracing::info!(
            status = res.status().as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "request done"
        );
    });
    let mut res = match res.extensions().get::<ErrorReason>().cloned() {
        Some(ErrorReason(reason)) => with_request_id(res, &reason, &id),
        None => res,
    };
    if let Ok(v) = HeaderValue::from_str(&id) {
        res.headers_mut().insert(REQUEST_ID, v);
    }
    res
}

fn with_request_id(res: Response, reason: &str, id: &str) -> Response {
    let (mut parts, _) = res.into_parts();
    let body = serde_json::json!({ "error": reason, "request_id": id }).to_string();
    parts.headers.remove(CONTENT_LENGTH);
    parts
        .headers
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Response::from_parts(parts, Body::from(body))
}

pub async fn internal_key(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let given = req
        .headers()
        .get(&INTERNAL_KEY)
        .map(HeaderValue::as_bytes)
        .unwrap_or_default();
    if !state.cfg.internal_key.matches(given) {
        tracing::warn!("internal key rejected");
        return ApiError::forbidden().into_response();
    }
    next.run(req).await
}

pub async fn strip_server_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    res.headers_mut().remove(SERVER);
    res.headers_mut().remove(POWERED_BY);
    res
}

pub fn panic_response(_payload: Box<dyn Any + Send + 'static>) -> Response {
    tracing::error!("handler panicked");
    ApiError::internal().into_response()
}
