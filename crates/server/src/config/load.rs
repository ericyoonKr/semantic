use std::net::SocketAddr;
use std::path::PathBuf;

use super::{CorpusConfig, Limits, Reader, Secret, Threads, corpus, limits};
use crate::error::ConfigError;

const DEFAULT_BIND: &str = "127.0.0.1:8080";
const DATA_FOLDER: &str = "semantic-data";

#[derive(Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub kg_dir: PathBuf,
    pub kg_consumer_schema: String,
    pub kg_manifest_schema: String,
    pub kg_source_schema: Option<String>,
    pub internal_key: Secret,
    pub bind: SocketAddr,
    pub limits: Limits,
    pub threads: Threads,
    pub judge: crate::judge::JudgeConfig,
    pub corpus: CorpusConfig,
    pub pdfium_dir: Option<PathBuf>,
    pub lawtime_principles: Option<PathBuf>,
    pub law_aliases: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let mut r = Reader {
            get,
            missing: Vec::new(),
        };
        let home = r.text("USERPROFILE").or_else(|| r.text("HOME"));
        let data_dir = match r.text("DATA_DIR") {
            Some(v) => Some(PathBuf::from(v)),
            None => home.map(|h| PathBuf::from(h).join(DATA_FOLDER)),
        };
        let kg_consumer_schema = r.required("KG_CONSUMER_SCHEMA");
        let kg_manifest_schema = r.required("KG_MANIFEST_SCHEMA");
        let kg_source_schema = r.text("KG_SOURCE_SCHEMA");
        let internal_key = Secret::new(r.required("INTERNAL_KEY"));
        let Some(data_dir) = data_dir else {
            r.missing.push("DATA_DIR");
            return Err(ConfigError::Missing(r.missing.join(", ")));
        };
        if !r.missing.is_empty() {
            return Err(ConfigError::Missing(r.missing.join(", ")));
        }
        let kg_dir = r
            .text("KG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("kg"));
        let bind: SocketAddr = r
            .text("BIND_ADDR")
            .unwrap_or_else(|| DEFAULT_BIND.to_string())
            .parse()
            .map_err(|_| ConfigError::Invalid("BIND_ADDR"))?;
        if !bind.ip().is_loopback() {
            return Err(ConfigError::NotLoopback);
        }
        let limits = limits::read_limits(&r)?;
        let threads = Threads {
            tokio_workers: r.positive("TOKIO_WORKERS", 8)?,
            tokio_blocking: r.positive("TOKIO_BLOCKING_THREADS", 32)?,
            rayon: r.positive("RAYON_THREADS", 12)?,
        };
        let judge = crate::judge::config::read(&r.get)?;
        let corpus = corpus::read(&r)?;
        let pdfium_dir = r.text("PDFIUM_DIR").map(PathBuf::from);
        let lawtime_principles = r.text("LAWTIME_PRINCIPLES").map(PathBuf::from);
        let law_aliases = r.text("LAW_ALIASES").map(PathBuf::from);
        Ok(Self {
            data_dir,
            kg_dir,
            kg_consumer_schema,
            kg_manifest_schema,
            kg_source_schema,
            internal_key,
            bind,
            limits,
            threads,
            judge,
            corpus,
            pdfium_dir,
            lawtime_principles,
            law_aliases,
        })
    }

    pub fn rocksdb_dir(&self) -> PathBuf {
        self.data_dir.join("rocksdb")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
}
