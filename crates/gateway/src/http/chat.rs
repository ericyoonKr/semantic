use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::audit::Event;
use crate::error::GatewayError;
use crate::filter::SseFilter;
use crate::http::guard::{Signed, thread_id};
use crate::http::sides::SideSink;
use crate::quota::AskPermit;
use crate::state::AppState;

fn upstream(state: &AppState, method: reqwest::Method, path: &str, user: Uuid, request_id: &str) -> reqwest::RequestBuilder {
    state
        .http
        .request(method, format!("{}{path}", state.cfg.agent_url))
        .header("x-internal-key", &state.cfg.internal_key)
        .header("x-user-id", user.to_string())
        .header("x-request-id", request_id)
        .timeout(state.cfg.call_deadline)
}

fn request_id() -> String {
    Uuid::new_v4().simple().to_string()
}

async fn owned(state: &AppState, signed: &Signed, raw: &str) -> Result<Uuid, GatewayError> {
    let id = thread_id(raw)?;
    if !state.store.owns(signed.user_id, id).await? {
        return Err(GatewayError::NotFound);
    }
    Ok(id)
}

async fn json_of(res: reqwest::Response) -> Result<Value, GatewayError> {
    if !res.status().is_success() {
        return Err(GatewayError::Upstream);
    }
    res.json::<Value>().await.map_err(|_| GatewayError::Upstream)
}

pub async fn list(State(state): State<AppState>, signed: Signed) -> Result<Json<Value>, GatewayError> {
    let rows = state.store.threads(signed.user_id).await?;
    Ok(Json(json!(rows.into_iter().map(|(id, title)| json!({ "session_id": id.to_string(), "title": title })).collect::<Vec<_>>())))
}

pub const TITLE_MAX_CHARS: usize = 80;

pub fn clean_title(raw: &str) -> Option<String> {
    let title: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty() && title.chars().count() <= TITLE_MAX_CHARS && !title.chars().any(char::is_control)).then_some(title)
}

pub async fn rename(State(state): State<AppState>, signed: Signed, Path(raw): Path<String>, Json(body): Json<Value>) -> Result<StatusCode, GatewayError> {
    let id = owned(&state, &signed, &raw).await?;
    let title = body.get("title").and_then(Value::as_str).and_then(clean_title).ok_or(GatewayError::BadRequest)?;
    if !state.store.rename_thread(signed.user_id, id, &title).await? {
        return Err(GatewayError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create(State(state): State<AppState>, signed: Signed) -> Result<Json<Value>, GatewayError> {
    let res = upstream(&state, reqwest::Method::POST, "/session", signed.user_id, &request_id()).send().await.map_err(|_| GatewayError::Upstream)?;
    let body = json_of(res).await?;
    let id = body.get("session_id").and_then(Value::as_str).and_then(|s| Uuid::parse_str(s).ok()).ok_or(GatewayError::Upstream)?;
    state.store.add_thread(signed.user_id, id).await?;
    Ok(Json(json!({ "session_id": id.to_string() })))
}

const STREAM_BROKEN: &str = "event: Error\ndata: {}\n\n";

struct AskStream {
    filter: SseFilter,
    sides: SideSink,
    _permit: AskPermit,
}

pub async fn ask(State(state): State<AppState>, signed: Signed, Path(raw): Path<String>, Json(body): Json<Value>) -> Result<Response, GatewayError> {
    let id = owned(&state, &signed, &raw).await?;
    let question = body.get("question").and_then(Value::as_str).ok_or(GatewayError::BadRequest)?;
    let rid = request_id();
    if state.store.tokens_today(signed.user_id).await? >= state.cfg.daily_token_limit {
        tracing::info!(request_id = %rid, "ask.daily_limit");
        state.audit.spawn(Event::new("guard_block").user(signed.user_id).thread(id).request(&rid).detail(json!({ "guard": "daily_tokens" })));
        return Err(GatewayError::DailyLimit);
    }
    let permit = state.quota.acquire(signed.user_id, id).await?;
    let res = upstream(&state, reqwest::Method::POST, &format!("/session/{id}/ask"), signed.user_id, &rid)
        .timeout(state.cfg.ask_deadline)
        .json(&json!({ "question": question }))
        .send()
        .await
        .map_err(|_| GatewayError::Upstream)?;
    if !res.status().is_success() {
        return Err(GatewayError::Upstream);
    }
    state.audit.spawn(Event::new("ask").user(signed.user_id).thread(id).request(&rid));
    state.store.touch_thread(id).await?;
    let sides = SideSink { store: state.store.clone(), audit: state.audit.clone(), user: signed.user_id, thread: id, request_id: rid.clone() };
    let start = AskStream { filter: SseFilter::new(rid, state.cfg.hidden_spans.clone()), sides, _permit: permit };
    let stream = res.bytes_stream().scan(start, |st, chunk| {
        let frames = match chunk {
            Ok(bytes) => st.filter.push(&String::from_utf8_lossy(&bytes)),
            Err(_) => st.filter.push(STREAM_BROKEN),
        };
        for side in st.filter.take_side() {
            st.sides.handle(side);
        }
        futures::future::ready(Some(Ok::<_, std::io::Error>(frames.concat())))
    });
    let mut out = Response::new(Body::from_stream(stream));
    out.headers_mut().insert(CONTENT_TYPE, axum::http::HeaderValue::from_static("text/event-stream"));
    Ok(out)
}

pub async fn live(State(state): State<AppState>, signed: Signed, Path(raw): Path<String>) -> Result<Response, GatewayError> {
    let id = owned(&state, &signed, &raw).await?;
    let rid = request_id();
    let res = upstream(&state, reqwest::Method::GET, &format!("/session/{id}/live"), signed.user_id, &rid)
        .timeout(state.cfg.ask_deadline)
        .send()
        .await
        .map_err(|_| GatewayError::Upstream)?;
    if res.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    if !res.status().is_success() {
        return Err(GatewayError::Upstream);
    }
    let filter = SseFilter::new(rid, state.cfg.hidden_spans.clone());
    let stream = res.bytes_stream().scan(filter, |filter, chunk| {
        let frames = match chunk {
            Ok(bytes) => filter.push(&String::from_utf8_lossy(&bytes)),
            Err(_) => filter.push(STREAM_BROKEN),
        };
        filter.take_side();
        futures::future::ready(Some(Ok::<_, std::io::Error>(frames.concat())))
    });
    let mut out = Response::new(Body::from_stream(stream));
    out.headers_mut().insert(CONTENT_TYPE, axum::http::HeaderValue::from_static("text/event-stream"));
    Ok(out)
}

pub async fn upload(State(state): State<AppState>, signed: Signed, Path(raw): Path<String>, headers: HeaderMap, body: Body) -> Result<Response, GatewayError> {
    let id = owned(&state, &signed, &raw).await?;
    if state.memory.low() {
        tracing::warn!(reason = "memory_low", "upload.refused");
        return Err(GatewayError::Unavailable);
    }
    let content_type = headers.get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_string();
    let res = upstream(&state, reqwest::Method::POST, &format!("/session/{id}/upload"), signed.user_id, &request_id())
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await
        .map_err(|_| GatewayError::Upstream)?;
    let status = res.status();
    if status.is_client_error() {
        return Err(GatewayError::BadRequest);
    }
    let value = json_of(res).await?;
    state.store.touch_thread(id).await?;
    Ok(Json(value).into_response())
}

pub async fn history(State(state): State<AppState>, signed: Signed, Path(raw): Path<String>) -> Result<Json<Value>, GatewayError> {
    let id = owned(&state, &signed, &raw).await?;
    let res = upstream(&state, reqwest::Method::GET, &format!("/session/{id}/history"), signed.user_id, &request_id())
        .send()
        .await
        .map_err(|_| GatewayError::Upstream)?;
    Ok(Json(json_of(res).await?))
}

pub async fn remove(State(state): State<AppState>, signed: Signed, Path(raw): Path<String>) -> Result<StatusCode, GatewayError> {
    let id = owned(&state, &signed, &raw).await?;
    let res = upstream(&state, reqwest::Method::DELETE, &format!("/session/{id}"), signed.user_id, &request_id())
        .send()
        .await
        .map_err(|_| GatewayError::Upstream)?;
    if !res.status().is_success() {
        return Err(GatewayError::Upstream);
    }
    state.store.remove_thread(signed.user_id, id).await?;
    state.audit.spawn(Event::new("delete").user(signed.user_id).thread(id));
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_prefs(State(state): State<AppState>, signed: Signed) -> Result<StatusCode, GatewayError> {
    let res = upstream(&state, reqwest::Method::DELETE, "/prefs", signed.user_id, &request_id())
        .send()
        .await
        .map_err(|_| GatewayError::Upstream)?;
    if !res.status().is_success() {
        return Err(GatewayError::Upstream);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod title_tests {
    use super::{TITLE_MAX_CHARS, clean_title};

    #[test]
    fn titles_are_trimmed_and_bounded() {
        assert_eq!(clean_title("  국가배상   질문 ").as_deref(), Some("국가배상 질문"));
        assert_eq!(clean_title("   "), None);
        assert_eq!(clean_title(&"가".repeat(TITLE_MAX_CHARS + 1)), None);
        assert!(clean_title(&"가".repeat(TITLE_MAX_CHARS)).is_some());
    }
}
