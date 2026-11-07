use std::fmt;
use std::time::{Duration, Instant};

use llm::LlmLimiter;
use serde::Deserialize;
use serde_json::json;

use crate::embedder::{DIM, EmbedFuture, Embedder};
use crate::error::RetrievalError;

#[derive(Clone)]
pub struct EmbedSettings {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub registry_name: String,
    pub deadline: Duration,
    pub concurrency: usize,
    pub priority: Option<i32>,
}

impl fmt::Debug for EmbedSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbedSettings")
            .field("model", &self.model)
            .field("registry_name", &self.registry_name)
            .field("deadline", &self.deadline)
            .field("concurrency", &self.concurrency)
            .field("priority", &self.priority)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct EmbedClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    registry_name: String,
    deadline: Duration,
    limiter: LlmLimiter,
    priority: Option<i32>,
    dim: usize,
}

impl fmt::Debug for EmbedClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbedClient")
            .field("model", &self.model)
            .field("dim", &self.dim)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct Reply {
    data: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    #[serde(default)]
    index: usize,
    embedding: Vec<f32>,
}

impl EmbedClient {
    pub fn new(settings: EmbedSettings) -> Result<Self, RetrievalError> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|_| RetrievalError::EmbedderDown)?;
        Ok(Self {
            http,
            base_url: settings.base_url.trim_end_matches('/').to_string(),
            api_key: settings.api_key,
            model: settings.model,
            registry_name: settings.registry_name,
            deadline: settings.deadline,
            limiter: LlmLimiter::new(settings.concurrency.max(1)),
            priority: settings.priority,
            dim: DIM,
        })
    }

    pub async fn embed_one(&self, text: &str) -> Result<Vec<f32>, RetrievalError> {
        let mut all = self.embed_batch(&[text]).await?;
        all.pop().ok_or(RetrievalError::EmbedMalformed)
    }

    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, RetrievalError> {
        let started = Instant::now();
        let outcome = tokio::time::timeout(self.deadline, self.send(texts)).await;
        let result = outcome.unwrap_or(Err(RetrievalError::EmbedderDown));
        tracing::info!(
            model = %self.model,
            inputs = texts.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            ok = result.is_ok(),
            "embed"
        );
        result
    }

    async fn send(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, RetrievalError> {
        let _permit = self
            .limiter
            .acquire()
            .await
            .map_err(|_| RetrievalError::EmbedderDown)?;
        let mut body = json!({ "model": self.model, "input": texts });
        if let Some(p) = self.priority {
            body["priority"] = json!(p);
        }
        let response = self
            .http
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|_| RetrievalError::EmbedderDown)?;
        let status = response.status();
        if !status.is_success() {
            return Err(RetrievalError::EmbedHttp(status.as_u16()));
        }
        let mut reply: Reply = response
            .json()
            .await
            .map_err(|_| RetrievalError::EmbedMalformed)?;
        if reply.data.len() != texts.len() {
            return Err(RetrievalError::EmbedMalformed);
        }
        reply.data.sort_by_key(|item| item.index);
        let mut out = Vec::with_capacity(reply.data.len());
        for item in reply.data {
            if item.embedding.len() != self.dim {
                return Err(RetrievalError::Dimension(item.embedding.len()));
            }
            out.push(item.embedding);
        }
        Ok(out)
    }
}

impl Embedder for EmbedClient {
    fn registry_name(&self) -> &str {
        &self.registry_name
    }

    fn embed<'a>(&'a self, text: &'a str) -> EmbedFuture<'a> {
        Box::pin(self.embed_one(text))
    }
}

#[cfg(test)]
#[path = "embed_client_tests.rs"]
mod tests;
