use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::error::LlmError;
use crate::vertex::{GenerateRequest, RetryPolicy, VertexEndpoint, generate};

const JSON_ONLY: &str = "앞의 지시에 맞는 JSON 하나만 출력하라. 다른 글자는 쓰지 마라.";

const VERTEX_BACKOFF_CAP: Duration = Duration::from_secs(30);

#[derive(Clone)]
enum Provider {
    OpenAi { base_url: String, api_key: String },
    Vertex { endpoint: VertexEndpoint, thinking_budget: Option<i32> },
}

#[derive(Clone)]
pub struct LlmClient {
    http: reqwest::Client,
    provider: Provider,
    model: String,
}

impl std::fmt::Debug for LlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let provider = match &self.provider {
            Provider::OpenAi { base_url, .. } => format!("openai {base_url}"),
            Provider::Vertex { endpoint, .. } => format!("vertex {endpoint:?}"),
        };
        f.debug_struct("LlmClient").field("provider", &provider).field("model", &self.model).finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub struct CallOptions {
    pub max_tokens: u32,
    pub deadline: Duration,
    pub retries: u32,
    pub backoff_base: Duration,
    pub priority: Option<i32>,
    pub json_mode: bool,
}

impl Default for CallOptions {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            deadline: Duration::from_secs(45),
            retries: 3,
            backoff_base: Duration::from_millis(500),
            priority: None,
            json_mode: true,
        }
    }
}

impl LlmClient {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|_| LlmError::Unavailable)?;
        Ok(Self {
            http,
            provider: Provider::OpenAi { base_url: base_url.into().trim_end_matches('/').to_string(), api_key: api_key.into() },
            model: model.into(),
        })
    }

    pub fn vertex(endpoint: VertexEndpoint, model: impl Into<String>, thinking_budget: Option<i32>) -> Result<Self, LlmError> {
        if !endpoint.has_key() {
            return Err(LlmError::Auth(0));
        }
        let http = reqwest::Client::builder().build().map_err(|_| LlmError::Unavailable)?;
        Ok(Self { http, provider: Provider::Vertex { endpoint, thinking_budget }, model: model.into() })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub async fn complete_json<T: DeserializeOwned>(&self, system: &str, user: &str, opts: &CallOptions) -> Result<T, LlmError> {
        let started = Instant::now();
        let outcome = tokio::time::timeout(opts.deadline, self.json_with_reask(system, user, opts)).await;
        let result = match outcome {
            Ok(inner) => inner,
            Err(_) => Err(LlmError::Timeout),
        };
        tracing::info!(
            model = %self.model,
            elapsed_ms = started.elapsed().as_millis() as u64,
            ok = result.is_ok(),
            "llm.complete_json"
        );
        result
    }

    async fn json_with_reask<T: DeserializeOwned>(&self, system: &str, user: &str, opts: &CallOptions) -> Result<T, LlmError> {
        let first = self.chat_with_retry(system, user, opts).await?;
        if let Some(value) = parse_json(&first) {
            return Ok(value);
        }
        let again = format!("{user}\n\n{JSON_ONLY}");
        let second = self.chat_with_retry(system, &again, opts).await?;
        parse_json(&second).ok_or(LlmError::ParseFailed)
    }

    async fn chat_with_retry(&self, system: &str, user: &str, opts: &CallOptions) -> Result<String, LlmError> {
        if let Provider::Vertex { endpoint, thinking_budget } = &self.provider {
            let policy = RetryPolicy { retries: opts.retries, base: opts.backoff_base, cap: VERTEX_BACKOFF_CAP };
            let req = GenerateRequest { system, user, max_tokens: opts.max_tokens, json_mode: opts.json_mode, thinking_budget: *thinking_budget };
            let reply = policy
                .run(&self.model, "generate", || generate::generate_once(&self.http, endpoint, &self.model, &req))
                .await?;
            return Ok(reply.text);
        }
        let mut attempt: u32 = 0;
        loop {
            match self.chat_once(system, user, opts).await {
                Ok(text) => return Ok(text),
                Err(error) if error.is_retryable() && attempt < opts.retries => {
                    let wait = opts.backoff_base.saturating_mul(2u32.saturating_pow(attempt));
                    tracing::warn!(model = %self.model, attempt, status = %error, "llm.retry");
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn chat_once(&self, system: &str, user: &str, opts: &CallOptions) -> Result<String, LlmError> {
        let Provider::OpenAi { base_url, api_key } = &self.provider else {
            return Err(LlmError::Unavailable);
        };
        let mut body = json!({
            "model": self.model,
            "temperature": 0,
            "max_tokens": opts.max_tokens,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
        });
        if opts.json_mode {
            body["response_format"] = json!({"type": "json_object"});
        }
        if let Some(priority) = opts.priority {
            body["priority"] = json!(priority);
        }
        let response = self
            .http
            .post(format!("{base_url}/chat/completions"))
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(classify_send_error)?;
        let status = response.status();
        if status.as_u16() == 429 {
            return Err(LlmError::RateLimited);
        }
        if !status.is_success() {
            return Err(LlmError::Http(status.as_u16()));
        }
        let payload: Value = response.json().await.map_err(|_| LlmError::ParseFailed)?;
        payload["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_owned)
            .ok_or(LlmError::ParseFailed)
    }
}

fn classify_send_error(error: reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        LlmError::Unavailable
    }
}

fn parse_json<T: DeserializeOwned>(text: &str) -> Option<T> {
    if let Ok(value) = serde_json::from_str::<T>(text.trim()) {
        return Some(value);
    }
    let start = text.find(['{', '['])?;
    let end = text.rfind(['}', ']'])?;
    if end <= start {
        return None;
    }
    serde_json::from_str::<T>(&text[start..=end]).ok()
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
