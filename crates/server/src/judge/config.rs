use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use engine::settings::LEAF_DEADLINE;
use llm::vertex::VertexEndpoint;

use crate::error::ConfigError;

pub const ENGINE_SETTINGS: [&str; 6] = [
    "LLM_BASE_URL",
    "LLM_MODEL",
    "LEAF_FACT_BUDGET_TOKENS",
    "LEAF_CANDIDATE_K",
    "LEAF_AUTHORITY_K",
    "SOLVER_TIMEOUT_MS",
];

pub struct EngineConfig {
    pub vertex: Option<VertexEndpoint>,
    pub thinking_budget: Option<i32>,
    pub llm_base_url: String,
    pub llm_model: String,
    pub llm_api_key: String,
    pub fact_budget_tokens: usize,
    pub candidate_k: usize,
    pub authority_k: usize,
    pub solver_timeout: Duration,
}

impl fmt::Debug for EngineConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineConfig")
            .field("llm_model", &self.llm_model)
            .field("vertex", &self.vertex)
            .field("fact_budget_tokens", &self.fact_budget_tokens)
            .field("candidate_k", &self.candidate_k)
            .field("authority_k", &self.authority_k)
            .field("solver_timeout", &self.solver_timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct JudgeConfig {
    pub engine: Option<EngineConfig>,
    pub z3_concurrency: usize,
    pub leaf_permits: usize,
    pub leaf_cache_entries: u64,
    pub deadline: Duration,
    pub wrapup: Duration,
    pub holidays_file: Option<PathBuf>,
}

fn text(get: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    get(name)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn positive<T: FromStr + PartialOrd + Default>(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
    default: Option<T>,
) -> Result<T, ConfigError> {
    let v = match (text(get, name), default) {
        (Some(v), _) => v.parse().map_err(|_| ConfigError::Invalid(name))?,
        (None, Some(d)) => d,
        (None, None) => return Err(ConfigError::Missing(name.to_string())),
    };
    if v > T::default() {
        Ok(v)
    } else {
        Err(ConfigError::Invalid(name))
    }
}

fn read_engine(get: &impl Fn(&str) -> Option<String>) -> Result<Option<EngineConfig>, ConfigError> {
    let (vertex, gemini) = match text(get, "LLM_PROVIDER").as_deref() {
        None | Some("openai") => (false, false),
        Some("vertex") => (true, false),
        Some("gemini") => (true, true),
        Some(_) => return Err(ConfigError::Invalid("LLM_PROVIDER")),
    };
    let wanted: Vec<&str> = if vertex {
        ENGINE_SETTINGS.iter().copied().filter(|n| *n != "LLM_BASE_URL").chain(["LLM_API_KEY"]).collect()
    } else {
        ENGINE_SETTINGS.to_vec()
    };
    let missing: Vec<&str> = wanted.iter().copied().filter(|n| text(get, n).is_none()).collect();
    if missing.len() == wanted.len() {
        return Ok(None);
    }
    if !missing.is_empty() {
        return Err(ConfigError::Missing(missing.join(", ")));
    }
    let solver_ms: u64 = positive(get, "SOLVER_TIMEOUT_MS", None)?;
    let api_key = text(get, "LLM_API_KEY").unwrap_or_default();
    let endpoint = match (vertex, gemini) {
        (true, true) => Some(VertexEndpoint::gemini(api_key.clone(), text(get, "GEMINI_HOST"))),
        (true, false) => Some(VertexEndpoint::new(api_key.clone(), text(get, "VERTEX_PROJECT"), text(get, "VERTEX_LOCATION"), text(get, "VERTEX_HOST"))),
        _ => None,
    };
    let thinking_budget = text(get, "LLM_THINKING_BUDGET")
        .map(|v| v.parse::<i32>().map_err(|_| ConfigError::Invalid("LLM_THINKING_BUDGET")))
        .transpose()?;
    Ok(Some(EngineConfig {
        vertex: endpoint,
        thinking_budget,
        llm_base_url: text(get, "LLM_BASE_URL").unwrap_or_default(),
        llm_model: text(get, "LLM_MODEL").unwrap_or_default(),
        llm_api_key: api_key,
        fact_budget_tokens: positive(get, "LEAF_FACT_BUDGET_TOKENS", None)?,
        candidate_k: positive(get, "LEAF_CANDIDATE_K", None)?,
        authority_k: positive(get, "LEAF_AUTHORITY_K", None)?,
        solver_timeout: Duration::from_millis(solver_ms),
    }))
}

pub fn read(get: &impl Fn(&str) -> Option<String>) -> Result<JudgeConfig, ConfigError> {
    let deadline: u64 = positive(get, "JUDGE_DEADLINE_SECS", Some(170))?;
    let wrapup: u64 = positive(get, "JUDGE_WRAPUP_SECS", Some(10))?;
    let cfg = JudgeConfig {
        engine: read_engine(get)?,
        z3_concurrency: positive(get, "Z3_CONCURRENCY", Some(4))?,
        leaf_permits: positive(get, "LEAF_GLOBAL_PERMITS", Some(12))?,
        leaf_cache_entries: positive(get, "LEAF_CACHE_ENTRIES", Some(4096))?,
        deadline: Duration::from_secs(deadline),
        wrapup: Duration::from_secs(wrapup),
        holidays_file: text(get, "HOLIDAYS_FILE").map(PathBuf::from),
    };
    if cfg.wrapup >= cfg.deadline {
        return Err(ConfigError::DeadlineOrder(
            "JUDGE_WRAPUP_SECS",
            "JUDGE_DEADLINE_SECS",
        ));
    }
    if LEAF_DEADLINE >= cfg.deadline.saturating_sub(cfg.wrapup) {
        return Err(ConfigError::DeadlineOrder(
            "LEAF_DEADLINE",
            "JUDGE_DEADLINE_SECS",
        ));
    }
    if let Some(engine) = &cfg.engine
        && engine.solver_timeout >= cfg.wrapup
    {
        return Err(ConfigError::DeadlineOrder(
            "SOLVER_TIMEOUT_MS",
            "JUDGE_WRAPUP_SECS",
        ));
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn with(pairs: &[(&str, &str)]) -> Result<JudgeConfig, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        read(&move |name: &str| map.get(name).cloned())
    }

    const ENGINE: [(&str, &str); 7] = [
        ("LLM_BASE_URL", "http://localhost:1"),
        ("LLM_MODEL", "m"),
        ("LLM_API_KEY", "key-value-987"),
        ("LEAF_FACT_BUDGET_TOKENS", "1000"),
        ("LEAF_CANDIDATE_K", "4"),
        ("LEAF_AUTHORITY_K", "3"),
        ("SOLVER_TIMEOUT_MS", "2000"),
    ];

    #[test]
    fn defaults_follow_the_pc_values_and_engine_is_off_without_settings() -> Result<(), ConfigError> {
        let cfg = with(&[])?;
        assert!(cfg.engine.is_none());
        assert_eq!(cfg.z3_concurrency, 4);
        assert_eq!(cfg.leaf_permits, 12);
        assert_eq!(cfg.deadline, Duration::from_secs(170));
        assert!(cfg.holidays_file.is_none());
        let named = with(&[("HOLIDAYS_FILE", " h.json ")])?;
        assert_eq!(named.holidays_file, Some(PathBuf::from("h.json")));
        let on = with(&ENGINE)?;
        assert!(on.engine.as_ref().is_some_and(|e| e.candidate_k == 4));
        assert!(!format!("{on:?}").contains("key-value-987"));
        Ok(())
    }

    #[test]
    fn partial_engine_settings_and_bad_order_are_refused() {
        let text = with(&[("LLM_MODEL", "m")])
            .map(|_| String::new())
            .unwrap_or_else(|e| e.to_string());
        assert!(text.contains("LLM_BASE_URL"), "{text}");
        assert!(!text.contains("LLM_MODEL,"), "{text}");
        assert!(matches!(
            with(&[("JUDGE_WRAPUP_SECS", "170")]),
            Err(ConfigError::DeadlineOrder(_, _))
        ));
        assert!(matches!(
            with(&[("JUDGE_DEADLINE_SECS", "65")]),
            Err(ConfigError::DeadlineOrder("LEAF_DEADLINE", _))
        ));
        let mut slow = ENGINE.to_vec();
        slow.push(("SOLVER_TIMEOUT_MS", "20000"));
        assert!(matches!(
            with(&slow),
            Err(ConfigError::DeadlineOrder("SOLVER_TIMEOUT_MS", _))
        ));
        assert!(matches!(
            with(&[("Z3_CONCURRENCY", "0")]),
            Err(ConfigError::Invalid("Z3_CONCURRENCY"))
        ));
    }

    #[test]
    fn vertex_engine_needs_no_base_url_but_needs_key() {
        let base = [
            ("LLM_PROVIDER", "vertex"),
            ("LLM_MODEL", "gemini-x"),
            ("LEAF_FACT_BUDGET_TOKENS", "100"),
            ("LEAF_CANDIDATE_K", "3"),
            ("LEAF_AUTHORITY_K", "3"),
            ("SOLVER_TIMEOUT_MS", "100"),
        ];
        let no_key = with(&base).map(|c| c.engine.is_some());
        assert!(matches!(no_key, Err(ConfigError::Missing(ref m)) if m == "LLM_API_KEY"), "{no_key:?}");
        let mut keyed = base.to_vec();
        keyed.push(("LLM_API_KEY", "vk-secret-2"));
        let cfg = with(&keyed);
        let engine = cfg.as_ref().ok().and_then(|c| c.engine.as_ref());
        assert!(engine.is_some_and(|e| e.vertex.is_some()));
        let mut gm = keyed.clone();
        gm[0] = ("LLM_PROVIDER", "gemini");
        let flavor = with(&gm).ok().and_then(|c| c.engine).and_then(|e| e.vertex).map(|v| v.flavor());
        assert_eq!(flavor, Some(llm::vertex::endpoint::Flavor::GeminiApi));
        assert!(!format!("{cfg:?}").contains("vk-secret-2"));
    }

}
