use std::sync::Arc;
use std::time::Duration;

use retrieval::{Corpus, CorpusDb, EmbedClient, EmbedSettings, Embedder, LawWeights, NoEmbedder};

use super::Reader;
use crate::error::ConfigError;

const DEFAULT_REGISTRY_MODEL: &str = "BAAI/bge-m3";

#[derive(Debug, Clone)]
pub struct CorpusConfig {
    pub db: Option<CorpusDb>,
    pub embed: Option<EmbedSettings>,
    pub pool_size: u32,
    pub search_deadline: Duration,
    pub citations_max: i64,
    pub vector_search: bool,
    pub law_weights: LawWeights,
    pub law_weights_annex: bool,
    pub term_stem: bool,
}

pub(super) fn read<F: Fn(&str) -> Option<String>>(
    r: &Reader<F>,
) -> Result<CorpusConfig, ConfigError> {
    let db = r
        .text("CORPUS_PG_URL")
        .map(|url| CorpusDb::parse(&url).map_err(|_| ConfigError::Invalid("CORPUS_PG_URL")))
        .transpose()?;
    let search_deadline = r.secs("SEARCH_DEADLINE_SECS", 20)?;
    let embed_deadline = r.secs("EMBED_DEADLINE_SECS", 10)?;
    if embed_deadline >= search_deadline {
        return Err(ConfigError::DeadlineOrder(
            "EMBED_DEADLINE_SECS",
            "SEARCH_DEADLINE_SECS",
        ));
    }
    Ok(CorpusConfig {
        db,
        embed: read_embed(r, embed_deadline)?,
        pool_size: r.positive("CORPUS_POOL_SIZE", 8)?,
        search_deadline,
        citations_max: r.positive("CITATIONS_MAX", 50)?,
        law_weights: match r.text("CORPUS_LAW_WEIGHTS") {
            None => LawWeights::default(),
            Some(v) => LawWeights::parse(&v).ok_or(ConfigError::Invalid("CORPUS_LAW_WEIGHTS"))?,
        },
        law_weights_annex: match r.text("CORPUS_LAW_WEIGHTS_ANNEX").as_deref() {
            None | Some("on") => true,
            Some("off") => false,
            Some(_) => return Err(ConfigError::Invalid("CORPUS_LAW_WEIGHTS_ANNEX")),
        },
        term_stem: match r.text("CORPUS_TERM_STEM").as_deref() {
            None | Some("off") => false,
            Some("on") => true,
            Some(_) => return Err(ConfigError::Invalid("CORPUS_TERM_STEM")),
        },
        vector_search: match r.text("CORPUS_VECTOR_SEARCH").as_deref() {
            None | Some("on") => true,
            Some("off") => false,
            Some(_) => return Err(ConfigError::Invalid("CORPUS_VECTOR_SEARCH")),
        },
    })
}

fn read_embed<F: Fn(&str) -> Option<String>>(
    r: &Reader<F>,
    deadline: Duration,
) -> Result<Option<EmbedSettings>, ConfigError> {
    let names = ["EMBED_BASE_URL", "EMBED_MODEL", "EMBED_API_KEY"];
    let values: Vec<Option<String>> = names.iter().map(|n| r.text(n)).collect();
    let missing: Vec<&str> = names
        .iter()
        .zip(&values)
        .filter(|(_, v)| v.is_none())
        .map(|(n, _)| *n)
        .collect();
    if missing.len() == names.len() {
        return Ok(None);
    }
    if !missing.is_empty() {
        return Err(ConfigError::Missing(missing.join(", ")));
    }
    let mut it = values.into_iter().flatten();
    let (Some(base_url), Some(model), Some(api_key)) = (it.next(), it.next(), it.next()) else {
        return Err(ConfigError::Missing(names.join(", ")));
    };
    let priority = r
        .text("EMBED_PRIORITY")
        .map(|v| v.parse::<i32>().map_err(|_| ConfigError::Invalid("EMBED_PRIORITY")))
        .transpose()?;
    Ok(Some(EmbedSettings {
        base_url,
        model,
        api_key,
        registry_name: r
            .text("CORPUS_EMBED_MODEL")
            .unwrap_or_else(|| DEFAULT_REGISTRY_MODEL.to_string()),
        deadline,
        concurrency: r.positive("EMBED_CONCURRENCY", 4)?,
        priority,
    }))
}

impl CorpusConfig {
    pub fn open(&self) -> Option<Corpus> {
        let Some(db) = &self.db else {
            tracing::warn!("CORPUS_PG_URL is not set; law and case search will report failure");
            return None;
        };
        let pool = db.connect_lazy(self.pool_size, self.search_deadline);
        let embedder: Arc<dyn Embedder> = match &self.embed {
            Some(settings) => match EmbedClient::new(settings.clone()) {
                Ok(client) => Arc::new(client),
                Err(e) => {
                    tracing::warn!(error = %e, "embedding client could not be built; vector search off");
                    Arc::new(NoEmbedder)
                }
            },
            None => {
                tracing::warn!("embedding server is not configured; vector search off");
                Arc::new(NoEmbedder)
            }
        };
        let weights = if self.law_weights_annex { self.law_weights } else { self.law_weights.articles_only() };
        let corpus = Corpus::new(pool, embedder).with_weights(weights).with_stemming(self.term_stem);
        if !self.law_weights.is_flat() {
            tracing::info!(weights = ?self.law_weights, "law kind weights on");
        }
        if self.vector_search {
            Some(corpus)
        } else {
            tracing::warn!("CORPUS_VECTOR_SEARCH=off; law and case search use text only");
            Some(corpus.without_vectors())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn reader(pairs: &[(&str, &str)]) -> Reader<impl Fn(&str) -> Option<String> + use<>> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Reader {
            get: move |name: &str| map.get(name).cloned(),
            missing: Vec::new(),
        }
    }

    #[test]
    fn nothing_set_turns_search_and_vectors_off() -> Result<(), ConfigError> {
        let cfg = read(&reader(&[]))?;
        assert!(cfg.db.is_none());
        assert!(cfg.embed.is_none());
        assert_eq!(cfg.pool_size, 8);
        assert_eq!(cfg.search_deadline, Duration::from_secs(20));
        assert_eq!(cfg.citations_max, 50);
        assert!(cfg.vector_search);
        Ok(())
    }

    #[test]
    fn partial_embedding_settings_name_what_is_missing_without_values() {
        let err = read(&reader(&[("EMBED_BASE_URL", "http://h/v1"), ("EMBED_API_KEY", "k-secret-1")]));
        let text = err.map(|_| String::new()).unwrap_or_else(|e| e.to_string());
        assert!(text.contains("EMBED_MODEL"), "{text}");
        assert!(!text.contains("k-secret-1"));
    }

    #[test]
    fn full_settings_are_read_and_hidden_in_debug() -> Result<(), ConfigError> {
        let cfg = read(&reader(&[
            ("CORPUS_PG_URL", "postgres://reader:db-secret-9@dbhost/corpus"),
            ("EMBED_BASE_URL", "http://h/v1"),
            ("EMBED_MODEL", "served"),
            ("EMBED_API_KEY", "k-secret-1"),
            ("EMBED_PRIORITY", "1"),
        ]))?;
        let embed = cfg.embed.clone().ok_or(ConfigError::Invalid("EMBED_MODEL"))?;
        assert_eq!(embed.registry_name, DEFAULT_REGISTRY_MODEL);
        assert_eq!(embed.priority, Some(1));
        assert_eq!(embed.concurrency, 4);
        assert_eq!(embed.deadline, Duration::from_secs(10));
        let shown = format!("{cfg:?}");
        assert!(!shown.contains("db-secret-9"), "{shown}");
        assert!(!shown.contains("k-secret-1"), "{shown}");
        Ok(())
    }

    #[test]
    fn bad_values_and_deadline_order_are_rejected() {
        assert!(matches!(read(&reader(&[("CORPUS_VECTOR_SEARCH", "off")])), Ok(c) if !c.vector_search));
        assert!(matches!(
            read(&reader(&[("CORPUS_VECTOR_SEARCH", "maybe")])),
            Err(ConfigError::Invalid("CORPUS_VECTOR_SEARCH"))
        ));
        assert!(matches!(
            read(&reader(&[("CORPUS_PG_URL", "not a url")])),
            Err(ConfigError::Invalid("CORPUS_PG_URL"))
        ));
        assert!(matches!(
            read(&reader(&[("EMBED_DEADLINE_SECS", "30")])),
            Err(ConfigError::DeadlineOrder(_, _))
        ));
    }
}
