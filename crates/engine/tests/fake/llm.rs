#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};

#[derive(Clone, Debug, Default)]
pub struct Script {
    pub replies: Vec<String>,
    pub delay: Duration,
    pub status: Option<u16>,
}

impl Script {
    pub fn says(replies: &[String]) -> Self {
        Self {
            replies: replies.to_vec(),
            ..Self::default()
        }
    }

    pub fn slow(reply: String, delay: Duration) -> Self {
        Self {
            replies: vec![reply],
            delay,
            status: None,
        }
    }

    pub fn status(code: u16) -> Self {
        Self {
            status: Some(code),
            ..Self::default()
        }
    }
}

pub fn reply(value: &str, quote: &str, source: &str) -> String {
    json!({"value": value, "quote": quote, "source": source}).to_string()
}

#[derive(Clone)]
struct Shared {
    scripts: Arc<HashMap<String, Script>>,
    calls: Arc<Mutex<HashMap<String, usize>>>,
}

pub struct FakeLlm {
    pub url: String,
    calls: Arc<Mutex<HashMap<String, usize>>>,
}

impl FakeLlm {
    pub fn calls(&self, proposition: &str) -> usize {
        self.calls
            .lock()
            .map(|m| m.get(proposition).copied().unwrap_or(0))
            .unwrap_or(0)
    }

    pub fn total(&self) -> usize {
        self.calls.lock().map(|m| m.values().sum()).unwrap_or(0)
    }
}

fn content(text: &str) -> Response {
    axum::Json(json!({"choices": [{"message": {"role": "assistant", "content": text}}]}))
        .into_response()
}

async fn handler(State(shared): State<Shared>, axum::Json(body): axum::Json<Value>) -> Response {
    let user = body["messages"][1]["content"].as_str().unwrap_or_default();
    let proposition = user
        .rsplit_once("[명제]\n")
        .map(|(_, p)| p.to_string())
        .unwrap_or_default();
    let n = match shared.calls.lock() {
        Ok(mut calls) => {
            let slot = calls.entry(proposition.clone()).or_insert(0);
            *slot += 1;
            *slot - 1
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(script) = shared.scripts.get(&proposition) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if !script.delay.is_zero() {
        tokio::time::sleep(script.delay).await;
    }
    if let Some(code) = script.status {
        return StatusCode::from_u16(code)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            .into_response();
    }
    let last = script.replies.len().saturating_sub(1);
    match script.replies.get(n.min(last)) {
        Some(text) => content(text),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn serve(scripts: HashMap<String, Script>) -> Result<FakeLlm, std::io::Error> {
    let calls = Arc::new(Mutex::new(HashMap::new()));
    let app = Router::new()
        .route("/v1/chat/completions", post(handler))
        .with_state(Shared {
            scripts: Arc::new(scripts),
            calls: calls.clone(),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(FakeLlm {
        url: format!("http://{addr}/v1"),
        calls,
    })
}
