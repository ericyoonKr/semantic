use std::collections::HashMap;
use std::path::PathBuf;

use super::Config;
use crate::error::ConfigError;

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
