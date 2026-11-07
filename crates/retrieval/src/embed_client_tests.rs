use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};

use super::{EmbedClient, EmbedSettings};
use crate::embedder::{DIM, Embedder};
use crate::error::RetrievalError;

#[derive(Clone, Copy)]
enum Plan {
    Good,
    WrongDim,
    Status(u16),
    Slow,
}

type Seen = Arc<Mutex<Vec<(Option<String>, Value)>>>;

#[derive(Clone)]
struct Fake {
    plan: Plan,
    seen: Seen,
}

async fn handler(State(fake): State<Fake>, headers: HeaderMap, body: axum::Json<Value>) -> Response {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    if let Ok(mut seen) = fake.seen.lock() {
        seen.push((auth, body.0.clone()));
    }
    let n = body.0["input"].as_array().map(Vec::len).unwrap_or(0);
    let rows = |dim: usize| -> Value {
        let data: Vec<Value> = (0..n)
            .rev()
            .map(|i| json!({"index": i, "embedding": vec![i as f32; dim]}))
            .collect();
        json!({ "data": data })
    };
    match fake.plan {
        Plan::Good => axum::Json(rows(DIM)).into_response(),
        Plan::WrongDim => axum::Json(rows(768)).into_response(),
        Plan::Status(code) => StatusCode::from_u16(code)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            .into_response(),
        Plan::Slow => {
            tokio::time::sleep(Duration::from_millis(500)).await;
            axum::Json(rows(DIM)).into_response()
        }
    }
}

async fn serve(plan: Plan) -> Result<(SocketAddr, Fake), std::io::Error> {
    let fake = Fake {
        plan,
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    let app = Router::new()
        .route("/v1/embeddings", post(handler))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok((addr, fake))
}

fn client(addr: SocketAddr, deadline_ms: u64) -> Result<EmbedClient, RetrievalError> {
    EmbedClient::new(EmbedSettings {
        base_url: format!("http://{addr}/v1/"),
        model: "served-model".into(),
        api_key: "embed-key-xyz".into(),
        registry_name: "registry-model".into(),
        deadline: Duration::from_millis(deadline_ms),
        concurrency: 4,
        priority: Some(1),
    })
}

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn returns_vectors_in_input_order_and_sends_model_key_priority() -> TestResult {
    let (addr, fake) = serve(Plan::Good).await?;
    let c = client(addr, 2000)?;
    let out = c.embed_batch(&["가", "나", "다"]).await?;
    assert_eq!(out.len(), 3);
    assert_eq!(out[0][0], 0.0);
    assert_eq!(out[2][0], 2.0);
    assert_eq!(out[1].len(), DIM);
    let one = c.embed("하나").await?;
    assert_eq!(one.len(), DIM);
    assert_eq!(c.registry_name(), "registry-model");
    let seen = fake.seen.lock().map(|s| s.clone()).unwrap_or_default();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].0.as_deref(), Some("Bearer embed-key-xyz"));
    assert_eq!(seen[0].1["model"], "served-model");
    assert_eq!(seen[0].1["priority"], 1);
    assert_eq!(seen[0].1["input"], json!(["가", "나", "다"]));
    Ok(())
}

#[tokio::test]
async fn wrong_dimension_is_an_error() -> TestResult {
    let (addr, _) = serve(Plan::WrongDim).await?;
    let err = client(addr, 2000)?.embed_one("x").await;
    assert!(matches!(err, Err(RetrievalError::Dimension(768))), "{err:?}");
    Ok(())
}

#[tokio::test]
async fn server_errors_and_silence_are_embedding_failures() -> TestResult {
    let (addr, _) = serve(Plan::Status(503)).await?;
    let err = client(addr, 2000)?.embed_one("x").await;
    assert!(matches!(err, Err(RetrievalError::EmbedHttp(503))), "{err:?}");

    let (addr, _) = serve(Plan::Slow).await?;
    let err = client(addr, 100)?.embed_one("x").await;
    assert!(matches!(err, Err(RetrievalError::EmbedderDown)), "{err:?}");

    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let closed = listener.local_addr()?;
    drop(listener);
    let err = client(closed, 2000)?.embed_one("x").await;
    assert!(matches!(err, Err(RetrievalError::EmbedderDown)), "{err:?}");
    assert!(err.as_ref().map_err(RetrievalError::is_embedding).err().unwrap_or(false));
    Ok(())
}

#[test]
fn debug_output_hides_the_key() -> TestResult {
    let addr: SocketAddr = (Ipv4Addr::LOCALHOST, 9).into();
    let c = client(addr, 10)?;
    assert!(!format!("{c:?}").contains("embed-key-xyz"));
    let s = EmbedSettings {
        base_url: "http://h/v1".into(),
        model: "m".into(),
        api_key: "embed-key-xyz".into(),
        registry_name: "r".into(),
        deadline: Duration::from_secs(1),
        concurrency: 1,
        priority: None,
    };
    assert!(!format!("{s:?}").contains("embed-key-xyz"));
    Ok(())
}
