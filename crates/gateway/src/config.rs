use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::GatewayError;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub app_db_url: String,
    pub auth_schema: String,
    pub agent_url: String,
    pub internal_key: String,
    pub web_dir: Option<PathBuf>,
    pub ask_deadline: Duration,
    pub call_deadline: Duration,
    pub upload_limit_bytes: usize,
    pub login_per_minute: u32,
    pub hidden_spans: Vec<(String, String)>,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    pub fn from_env() -> Result<Self, GatewayError> {
        let missing: Vec<&str> = ["APP_DB_URL", "INTERNAL_KEY"].into_iter().filter(|n| var(n).is_none()).collect();
        if !missing.is_empty() {
            return Err(GatewayError::Config(missing.join(", ")));
        }
        let bind: SocketAddr = var("GATEWAY_BIND")
            .unwrap_or_else(|| "127.0.0.1:8090".into())
            .parse()
            .map_err(|_| GatewayError::Config("GATEWAY_BIND".into()))?;
        let hidden = var("GATEWAY_HIDDEN_SPANS").unwrap_or_else(|| "<think>|</think>".into());
        let hidden_spans = hidden
            .split(';')
            .filter_map(|pair| pair.split_once('|').map(|(a, b)| (a.to_string(), b.to_string())))
            .collect();
        Ok(Self {
            bind,
            app_db_url: var("APP_DB_URL").unwrap_or_default(),
            auth_schema: var("AUTH_SCHEMA").unwrap_or_else(|| "auth".into()),
            agent_url: var("AGENT_URL").unwrap_or_else(|| "http://127.0.0.1:8000".into()).trim_end_matches('/').to_string(),
            internal_key: var("INTERNAL_KEY").unwrap_or_default(),
            web_dir: var("WEB_DIR").map(PathBuf::from),
            ask_deadline: Duration::from_secs(300),
            call_deadline: Duration::from_secs(190),
            upload_limit_bytes: 201 * 1024 * 1024,
            login_per_minute: 10,
            hidden_spans,
        })
    }
}
