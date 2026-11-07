use std::fmt;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use parsers::GuardLimits;

use crate::error::ConfigError;

const MIB: u64 = 1024 * 1024;
const DEFAULT_BIND: &str = "127.0.0.1:8080";
const DATA_FOLDER: &str = "semantic-data";

pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn matches(&self, given: &[u8]) -> bool {
        let want = self.0.as_bytes();
        if want.len() != given.len() {
            return false;
        }
        want.iter()
            .zip(given)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub guard: GuardLimits,
    pub json_body_bytes: usize,
    pub upload_body_bytes: usize,
    pub upload_concurrency: usize,
    pub session_channel: usize,
    pub session_idle: Duration,
    pub upload_deadline: Duration,
    pub kg_deadline: Duration,
    pub shutdown_grace: Duration,
    pub outline_level: u8,
    pub outline_lines: usize,
    pub filename_chars: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Threads {
    pub tokio_workers: usize,
    pub tokio_blocking: usize,
    pub rayon: usize,
}

#[derive(Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub kg_dir: PathBuf,
    pub kg_consumer_schema: String,
    pub kg_manifest_schema: String,
    pub internal_key: Secret,
    pub bind: SocketAddr,
    pub limits: Limits,
    pub threads: Threads,
}

struct Reader<F> {
    get: F,
    missing: Vec<&'static str>,
}

impl<F: Fn(&str) -> Option<String>> Reader<F> {
    fn text(&self, name: &str) -> Option<String> {
        (self.get)(name)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    fn required(&mut self, name: &'static str) -> String {
        match self.text(name) {
            Some(v) => v,
            None => {
                self.missing.push(name);
                String::new()
            }
        }
    }

    fn positive<T: FromStr + PartialOrd + Default>(
        &self,
        name: &'static str,
        default: T,
    ) -> Result<T, ConfigError> {
        let v = match self.text(name) {
            None => default,
            Some(v) => v.parse().map_err(|_| ConfigError::Invalid(name))?,
        };
        if v > T::default() {
            Ok(v)
        } else {
            Err(ConfigError::Invalid(name))
        }
    }

    fn secs(&self, name: &'static str, default: u64) -> Result<Duration, ConfigError> {
        self.positive(name, default).map(Duration::from_secs)
    }
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
        let limits = read_limits(&r)?;
        let threads = Threads {
            tokio_workers: r.positive("TOKIO_WORKERS", 8)?,
            tokio_blocking: r.positive("TOKIO_BLOCKING_THREADS", 32)?,
            rayon: r.positive("RAYON_THREADS", 12)?,
        };
        Ok(Self {
            data_dir,
            kg_dir,
            kg_consumer_schema,
            kg_manifest_schema,
            internal_key,
            bind,
            limits,
            threads,
        })
    }

    pub fn rocksdb_dir(&self) -> PathBuf {
        self.data_dir.join("rocksdb")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
}

fn read_limits<F: Fn(&str) -> Option<String>>(r: &Reader<F>) -> Result<Limits, ConfigError> {
    let base = GuardLimits::default();
    let parse_secs: u64 = r.positive("PARSE_TIME_LIMIT_SECS", 120)?;
    let guard = GuardLimits {
        max_file_bytes: r.positive("UPLOAD_MAX_BYTES", base.max_file_bytes)?,
        max_pages: r.positive("UPLOAD_MAX_PAGES", base.max_pages)?,
        max_chars: r.positive("UPLOAD_MAX_CHARS", base.max_chars)?,
        max_member_bytes: r.positive("ZIP_MEMBER_MAX_BYTES", base.max_member_bytes)?,
        max_total_bytes: r.positive("ZIP_TOTAL_MAX_BYTES", base.max_total_bytes)?,
        max_ratio: r.positive("ZIP_MAX_RATIO", base.max_ratio)?,
        time_limit: Some(Duration::from_secs(parse_secs)),
    };
    let upload_body_bytes = usize::try_from(guard.max_file_bytes.saturating_add(MIB))
        .map_err(|_| ConfigError::Invalid("UPLOAD_MAX_BYTES"))?;
    let limits = Limits {
        guard,
        json_body_bytes: r.positive("JSON_BODY_MAX_BYTES", 64 * 1024)?,
        upload_body_bytes,
        upload_concurrency: r.positive("UPLOAD_CONCURRENCY", 2)?,
        session_channel: r.positive("SESSION_CHANNEL", 16)?,
        session_idle: r.secs("SESSION_IDLE_SECS", 600)?,
        upload_deadline: r.secs("UPLOAD_DEADLINE_SECS", 170)?,
        kg_deadline: r.secs("KG_DEADLINE_SECS", 20)?,
        shutdown_grace: r.secs("SHUTDOWN_GRACE_SECS", 30)?,
        outline_level: r.positive("OUTLINE_MAX_LEVEL", 2)?,
        outline_lines: r.positive("OUTLINE_MAX_LINES", 60)?,
        filename_chars: r.positive("FILENAME_MAX_CHARS", crate::upload::filename::MAX_CHARS)?,
    };
    if Duration::from_secs(parse_secs) >= limits.upload_deadline {
        return Err(ConfigError::DeadlineOrder(
            "PARSE_TIME_LIMIT_SECS",
            "UPLOAD_DEADLINE_SECS",
        ));
    }
    Ok(limits)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| map.get(name).cloned()
    }

    const BASE: [(&str, &str); 4] = [
        ("DATA_DIR", "d"),
        ("KG_CONSUMER_SCHEMA", "c"),
        ("KG_MANIFEST_SCHEMA", "m"),
        ("INTERNAL_KEY", "secret-value-123"),
    ];

    #[test]
    fn missing_names_are_listed_without_values() {
        let err = Config::from_lookup(lookup(&[("INTERNAL_KEY", "secret-value-123")]));
        let text = err.map(|_| String::new()).unwrap_or_else(|e| e.to_string());
        assert!(
            text.contains("KG_CONSUMER_SCHEMA, KG_MANIFEST_SCHEMA"),
            "{text}"
        );
        assert!(!text.contains("secret-value-123"));
    }

    #[test]
    fn defaults_follow_the_pc_values_and_hide_the_key() -> Result<(), ConfigError> {
        let cfg = Config::from_lookup(lookup(&BASE))?;
        assert_eq!(cfg.limits.upload_concurrency, 2);
        assert_eq!(cfg.limits.session_channel, 16);
        assert_eq!(cfg.threads.tokio_workers, 8);
        assert_eq!(cfg.threads.tokio_blocking, 32);
        assert_eq!(cfg.threads.rayon, 12);
        assert_eq!(cfg.kg_dir, PathBuf::from("d").join("kg"));
        assert!(cfg.bind.ip().is_loopback());
        assert!(!format!("{cfg:?}").contains("secret-value-123"));
        assert!(cfg.internal_key.matches(b"secret-value-123"));
        assert!(!cfg.internal_key.matches(b"secret-value-124"));
        Ok(())
    }

    #[test]
    fn bad_values_name_the_setting() {
        let with = |extra: (&str, &str)| {
            let mut pairs = BASE.to_vec();
            pairs.push(extra);
            Config::from_lookup(lookup(&pairs))
        };
        assert!(matches!(
            with(("BIND_ADDR", "0.0.0.0:80")),
            Err(ConfigError::NotLoopback)
        ));
        assert!(matches!(
            with(("UPLOAD_CONCURRENCY", "0")),
            Err(ConfigError::Invalid("UPLOAD_CONCURRENCY"))
        ));
        assert!(matches!(
            with(("SESSION_CHANNEL", "many")),
            Err(ConfigError::Invalid("SESSION_CHANNEL"))
        ));
        assert!(matches!(
            with(("PARSE_TIME_LIMIT_SECS", "500")),
            Err(ConfigError::DeadlineOrder(_, _))
        ));
    }
}
