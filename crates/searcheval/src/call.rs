use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::sample::UnitParams;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Laws,
    Cases,
    Unit,
}

impl Target {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "laws" => Some(Self::Laws),
            "cases" => Some(Self::Cases),
            "unit" => Some(Self::Unit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Call {
    pub id: String,
    pub target: Target,
    pub query: String,
    pub as_of: Option<String>,
    pub decided_to: Option<String>,
    pub params: Option<UnitParams>,
    pub k: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ok,
    NotFound,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub id: String,
    pub target: Target,
    pub status: Status,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default = "unknown_latency")]
    pub latency_ms: f64,
    #[serde(default)]
    pub hits: Vec<Value>,
}

fn unknown_latency() -> f64 {
    f64::NAN
}

pub struct Http {
    client: reqwest::Client,
    base: String,
    key: String,
}

impl Http {
    pub fn new(base: &str, key: &str, timeout: Duration) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .context("HTTP 클라이언트를 만들지 못했습니다")?;
        Ok(Self {
            client,
            base: base.trim_end_matches('/').to_string(),
            key: key.to_string(),
        })
    }

    pub async fn run(&self, call: &Call) -> Outcome {
        let started = Instant::now();
        let sent = match call.target {
            Target::Laws => {
                let mut body = json!({"query": call.query, "k": call.k});
                if let Some(d) = &call.as_of {
                    body["as_of"] = json!(d);
                }
                self.post("/search/laws", body).await
            }
            Target::Cases => {
                let mut body = json!({"query": call.query, "k": call.k});
                if let Some(d) = &call.decided_to {
                    body["decided_to"] = json!(d);
                }
                self.post("/search/cases", body).await
            }
            Target::Unit => {
                let mut q: Vec<(&str, String)> = Vec::new();
                if let Some(p) = &call.params {
                    q.push(("law", p.law.clone()));
                    q.push(("unit", p.unit.clone()));
                }
                if let Some(d) = &call.as_of {
                    q.push(("as_of", d.clone()));
                }
                self.get("/laws/unit", &q).await
            }
        };
        let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
        let (status, reason, hits) = match sent {
            Ok((code, body)) => classify(call.target, code, body),
            Err(e) => (Status::Failed, Some(transport_reason(&e)), Vec::new()),
        };
        Outcome {
            id: call.id.clone(),
            target: call.target,
            status,
            reason,
            latency_ms,
            hits,
        }
    }

    async fn post(&self, path: &str, body: Value) -> reqwest::Result<(u16, Value)> {
        let res = self
            .client
            .post(format!("{}{}", self.base, path))
            .header("x-internal-key", &self.key)
            .json(&body)
            .send()
            .await?;
        read(res).await
    }

    async fn get(&self, path: &str, q: &[(&str, String)]) -> reqwest::Result<(u16, Value)> {
        let res = self
            .client
            .get(format!("{}{}", self.base, path))
            .header("x-internal-key", &self.key)
            .query(q)
            .send()
            .await?;
        read(res).await
    }
}

async fn read(res: reqwest::Response) -> reqwest::Result<(u16, Value)> {
    let code = res.status().as_u16();
    let text = res.text().await?;
    Ok((code, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

fn transport_reason(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "시간 초과".to_string()
    } else if e.is_connect() {
        "서버에 연결되지 않음".to_string()
    } else {
        "요청 오류".to_string()
    }
}

fn classify(target: Target, code: u16, body: Value) -> (Status, Option<String>, Vec<Value>) {
    let msg = body
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string);
    if (200..300).contains(&code) {
        let hits = match target {
            Target::Unit => vec![body],
            _ => body
                .get("hits")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        };
        return (Status::Ok, None, hits);
    }
    let reason = format!("HTTP {code}: {}", msg.clone().unwrap_or_default());
    match msg {
        Some(m) if code == 400 && m.contains("찾지 못했습니다") => {
            (Status::NotFound, Some(reason), Vec::new())
        }
        _ => (Status::Failed, Some(reason), Vec::new()),
    }
}

pub fn load_recorded(path: &Path) -> Result<HashMap<(String, Target), Outcome>> {
    let rows: Vec<Outcome> = crate::sample::read_jsonl(path)?;
    Ok(rows
        .into_iter()
        .map(|o| ((o.id.clone(), o.target), o))
        .collect())
}

pub fn from_recorded(map: &HashMap<(String, Target), Outcome>, call: &Call) -> Outcome {
    map.get(&(call.id.clone(), call.target))
        .cloned()
        .unwrap_or_else(|| Outcome {
            id: call.id.clone(),
            target: call.target,
            status: Status::Failed,
            reason: Some("기록 파일에 없음".to_string()),
            latency_ms: f64::NAN,
            hits: Vec::new(),
        })
}

pub fn save(path: &Path, outcomes: &[Outcome]) -> Result<()> {
    let mut text = String::new();
    for o in outcomes {
        text.push_str(&serde_json::to_string(o)?);
        text.push('\n');
    }
    std::fs::write(path, text).with_context(|| format!("기록을 쓰지 못했습니다: {}", path.display()))
}
