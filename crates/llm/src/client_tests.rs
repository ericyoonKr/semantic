use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::Deserialize;
use serde_json::json;

use super::{CallOptions, LlmClient};
use crate::error::LlmError;

#[derive(Clone, Copy)]
enum Plan {
    AlwaysText,
    RateLimitedTwiceThenOk,
    AlwaysUnavailable,
    Slow,
}

#[derive(Clone)]
struct Fake {
    plan: Plan,
    calls: Arc<AtomicUsize>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Answer {
    value: String,
}

fn content(text: &str) -> Response {
    axum::Json(json!({"choices": [{"message": {"role": "assistant", "content": text}}]})).into_response()
}

async fn handler(State(fake): State<Fake>) -> Response {
    let n = fake.calls.fetch_add(1, Ordering::SeqCst);
    match fake.plan {
        Plan::AlwaysText => content("이건 JSON이 아니다"),
        Plan::RateLimitedTwiceThenOk if n < 2 => StatusCode::TOO_MANY_REQUESTS.into_response(),
        Plan::RateLimitedTwiceThenOk => content("{\"value\": \"True\"}"),
        Plan::AlwaysUnavailable => StatusCode::SERVICE_UNAVAILABLE.into_response(),
        Plan::Slow => {
            tokio::time::sleep(Duration::from_millis(500)).await;
            content("{\"value\": \"True\"}")
        }
    }
}

async fn serve(plan: Plan) -> Result<(String, Arc<AtomicUsize>), std::io::Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/v1/chat/completions", post(handler))
        .with_state(Fake { plan, calls: calls.clone() });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok((format!("http://{addr}/v1"), calls))
}

fn opts(deadline_ms: u64) -> CallOptions {
    CallOptions {
        deadline: Duration::from_millis(deadline_ms),
        retries: 3,
        backoff_base: Duration::from_millis(1),
        ..CallOptions::default()
    }
}

#[tokio::test]
async fn parse_failure_reasks_once_then_fails() -> Result<(), Box<dyn std::error::Error>> {
    let (url, calls) = serve(Plan::AlwaysText).await?;
    let client = LlmClient::new(url, "k", "m")?;
    let result = client.complete_json::<Answer>("s", "u", &opts(5_000)).await;
    assert_eq!(result, Err(LlmError::ParseFailed));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn rate_limit_is_retried_until_success() -> Result<(), Box<dyn std::error::Error>> {
    let (url, calls) = serve(Plan::RateLimitedTwiceThenOk).await?;
    let client = LlmClient::new(url, "k", "m")?;
    let result = client.complete_json::<Answer>("s", "u", &opts(5_000)).await;
    assert_eq!(result, Ok(Answer { value: "True".into() }));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    Ok(())
}

#[tokio::test]
async fn server_error_gives_up_after_retries() -> Result<(), Box<dyn std::error::Error>> {
    let (url, calls) = serve(Plan::AlwaysUnavailable).await?;
    let client = LlmClient::new(url, "k", "m")?;
    let result = client.complete_json::<Answer>("s", "u", &opts(5_000)).await;
    assert_eq!(result, Err(LlmError::Http(503)));
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    Ok(())
}

#[tokio::test]
async fn deadline_is_not_retried() -> Result<(), Box<dyn std::error::Error>> {
    let (url, calls) = serve(Plan::Slow).await?;
    let client = LlmClient::new(url, "k", "m")?;
    let result = client.complete_json::<Answer>("s", "u", &opts(100)).await;
    assert_eq!(result, Err(LlmError::Timeout));
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}
