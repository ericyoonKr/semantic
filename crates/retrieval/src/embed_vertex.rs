use std::time::Duration;

use futures::future::try_join_all;
use llm::LlmLimiter;
use llm::vertex::embed::{self, EmbedRequest, QUERY};
use llm::vertex::{RetryPolicy, VertexEndpoint, VertexError};

use crate::embedder::DIM;
use crate::error::RetrievalError;

const RETRIES: u32 = 3;
const BACKOFF_BASE: Duration = Duration::from_millis(300);
const BACKOFF_CAP: Duration = Duration::from_secs(5);

pub fn to_retrieval(e: VertexError) -> RetrievalError {
    match e {
        VertexError::Timeout | VertexError::Connect => RetrievalError::EmbedderDown,
        VertexError::RateLimited { .. } => RetrievalError::EmbedHttp(429),
        VertexError::Auth { code, .. } | VertexError::BadRequest { code, .. } | VertexError::Server { code, .. } => {
            RetrievalError::EmbedHttp(code)
        }
        VertexError::Blocked(_) | VertexError::Truncated | VertexError::Malformed(_) => RetrievalError::EmbedMalformed,
    }
}

pub async fn embed_queries(
    http: &reqwest::Client,
    endpoint: &VertexEndpoint,
    model: &str,
    limiter: &LlmLimiter,
    texts: &[&str],
) -> Result<Vec<Vec<f32>>, RetrievalError> {
    let policy = RetryPolicy { retries: RETRIES, base: BACKOFF_BASE, cap: BACKOFF_CAP };
    let calls = texts.iter().map(|text| async move {
        let _permit = limiter.acquire().await.map_err(|_| RetrievalError::EmbedderDown)?;
        let req = EmbedRequest { text, task: QUERY, title: None, dimensions: DIM };
        let reply = policy
            .run(model, "embed", || embed::embed_once(http, endpoint, model, &req))
            .await
            .map_err(to_retrieval)?;
        if reply.truncated {
            tracing::warn!(model, tokens = reply.tokens, "vertex embed input was truncated");
        }
        Ok::<Vec<f32>, RetrievalError>(reply.values)
    });
    try_join_all(calls).await
}
