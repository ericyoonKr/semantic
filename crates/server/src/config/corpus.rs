use std::sync::Arc;
use std::time::Duration;

use retrieval::query::keyword::KeywordIndex;
use retrieval::query::keyword::error::{log_case_index_unset, log_open_failure};
use retrieval::{CasePhrase, CaseWeights, Corpus, CorpusDb, EmbedClient, EmbedSettings, Embedder, LawWeights, NoEmbedder};

use llm::vertex::VertexEndpoint;

use super::Reader;
use crate::error::ConfigError;

const DEFAULT_REGISTRY_MODEL: &str = "BAAI/bge-m3";
const DEFAULT_KEYWORD_MAX_DF: f64 = 0.02;
const DEFAULT_CASE_TEXT_RARE: usize = 3;

#[derive(Debug, Clone)]
pub struct CorpusConfig {
    pub db: Option<CorpusDb>,
    pub embed: Option<EmbedSettings>,
    pub pool_size: u32,
    pub search_deadline: Duration,
    pub citations_max: i64,
    pub vector_search: bool,
    pub law_scope: bool,
    pub unit_vectors: bool,
    pub case_vectors: bool,
    pub case_text_rare: usize,
    pub case_phrase: CasePhrase,
    pub law_weights: LawWeights,
    pub law_weights_annex: bool,
    pub term_stem: bool,
    pub keyword_index: Option<std::path::PathBuf>,
    pub keyword_max_df: f64,
    pub case_keyword_index: Option<std::path::PathBuf>,
    pub case_weights: CaseWeights,
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
        keyword_index: r.text("CORPUS_KEYWORD_INDEX").map(std::path::PathBuf::from),
        keyword_max_df: match r.text("CORPUS_KEYWORD_MAX_DF") {
            None => DEFAULT_KEYWORD_MAX_DF,
            Some(v) => match v.parse::<f64>() {
                Ok(x) if x > 0.0 && x <= 1.0 => x,
                _ => return Err(ConfigError::Invalid("CORPUS_KEYWORD_MAX_DF")),
            },
        },
        case_keyword_index: r.text("CORPUS_CASE_KEYWORD_INDEX").map(std::path::PathBuf::from),
        case_weights: match r.text("CORPUS_CASE_WEIGHTS") {
            None => CaseWeights::default(),
            Some(v) => CaseWeights::parse(&v).ok_or(ConfigError::Invalid("CORPUS_CASE_WEIGHTS"))?,
        },
        term_stem: match r.text("CORPUS_TERM_STEM").as_deref() {
            None | Some("off") => false,
            Some("on") => true,
            Some(_) => return Err(ConfigError::Invalid("CORPUS_TERM_STEM")),
        },
        law_scope: match r.text("CORPUS_LAW_SCOPE").as_deref() {
            None | Some("on") => true,
            Some("off") => false,
            Some(_) => return Err(ConfigError::Invalid("CORPUS_LAW_SCOPE")),
        },
        case_text_rare: match r.text("CORPUS_CASE_TEXT_RARE") {
            None => DEFAULT_CASE_TEXT_RARE,
            Some(v) => match v.parse::<usize>() {
                Ok(n) if n <= 8 => n,
                _ => return Err(ConfigError::Invalid("CORPUS_CASE_TEXT_RARE")),
            },
        },
        case_phrase: match r.text("CORPUS_CASE_PHRASE").as_deref() {
            None | Some("off") => CasePhrase::Off,
            Some("add") => CasePhrase::Add,
            Some("replace") => CasePhrase::Replace,
            Some(_) => return Err(ConfigError::Invalid("CORPUS_CASE_PHRASE")),
        },
        case_vectors: match r.text("CORPUS_CASE_VECTOR").as_deref() {
            None | Some("off") => false,
            Some("on") => true,
            Some(_) => return Err(ConfigError::Invalid("CORPUS_CASE_VECTOR")),
        },
        unit_vectors: match r.text("CORPUS_UNIT_VECTOR").as_deref() {
            None | Some("on") => true,
            Some("off") => false,
            Some(_) => return Err(ConfigError::Invalid("CORPUS_UNIT_VECTOR")),
        },
        vector_search: match r.text("CORPUS_VECTOR_SEARCH").as_deref() {
            None | Some("on") => true,
            Some("off") => false,
            Some(_) => return Err(ConfigError::Invalid("CORPUS_VECTOR_SEARCH")),
        },
    })
}

pub fn vertex_endpoint<F: Fn(&str) -> Option<String>>(r: &Reader<F>, api_key: String) -> VertexEndpoint {
    VertexEndpoint::new(api_key, r.text("VERTEX_PROJECT"), r.text("VERTEX_LOCATION"), r.text("VERTEX_HOST"))
}

fn read_embed<F: Fn(&str) -> Option<String>>(
    r: &Reader<F>,
    deadline: Duration,
) -> Result<Option<EmbedSettings>, ConfigError> {
    let (vertex, gemini) = match r.text("EMBED_PROVIDER").as_deref() {
        None | Some("openai") => (false, false),
        Some("vertex") => (true, false),
        Some("gemini") => (true, true),
        Some(_) => return Err(ConfigError::Invalid("EMBED_PROVIDER")),
    };
    let names: &[&str] = if vertex { &["EMBED_MODEL", "EMBED_API_KEY"] } else { &["EMBED_BASE_URL", "EMBED_MODEL", "EMBED_API_KEY"] };
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
    let base_url = if vertex { String::new() } else { it.next().unwrap_or_default() };
    let (Some(model), Some(api_key)) = (it.next(), it.next()) else {
        return Err(ConfigError::Missing(names.join(", ")));
    };
    let endpoint = match (vertex, gemini) {
        (true, true) => Some(VertexEndpoint::gemini(api_key.clone(), r.text("GEMINI_HOST"))),
        (true, false) => Some(vertex_endpoint(r, api_key.clone())),
        _ => None,
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
        vertex: endpoint,
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
        let mut corpus = Corpus::new(pool, embedder)
            .with_weights(weights)
            .with_stemming(self.term_stem)
            .with_law_scope(self.law_scope)
            .with_unit_vectors(self.unit_vectors)
            .with_case_vectors(self.case_vectors)
            .with_case_text_rare(self.case_text_rare)
            .with_case_phrase(self.case_phrase)
            .with_case_weights(self.case_weights);
        if let Some(dir) = &self.keyword_index {
            match KeywordIndex::open(dir) {
                Ok(index) => {
                    tracing::info!(dir = %dir.display(), max_df = self.keyword_max_df, "keyword index ready");
                    corpus = corpus.with_keyword(index.with_max_df_ratio(self.keyword_max_df));
                }
                Err(e) => log_open_failure(&e),
            }
        }
        match &self.case_keyword_index {
            Some(dir) => match KeywordIndex::open(dir) {
                Ok(index) => {
                    tracing::info!(dir = %dir.display(), positions = index.has_positions(), phrase = ?self.case_phrase, "case keyword index ready");
                    corpus = corpus.with_case_keyword(index);
                }
                Err(e) => log_open_failure(&e),
            },
            None => log_case_index_unset(),
        }
        tracing::info!(holding = self.case_weights.holding, overruled = self.case_weights.overruled, "case rank weights");
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
        assert_eq!(cfg.case_weights, CaseWeights::default());
        assert!(cfg.case_keyword_index.is_none());
        assert!(cfg.unit_vectors);
        assert!(!cfg.case_vectors);
        assert_eq!(cfg.case_text_rare, DEFAULT_CASE_TEXT_RARE);
        assert_eq!(cfg.case_phrase, CasePhrase::Off);
        assert_eq!(cfg.keyword_max_df, DEFAULT_KEYWORD_MAX_DF);
        Ok(())
    }

    #[test]
    fn unit_vector_branch_is_read_as_on_or_off_only() -> Result<(), ConfigError> {
        assert!(read(&reader(&[("CORPUS_UNIT_VECTOR", "on")]))?.unit_vectors);
        assert!(!read(&reader(&[("CORPUS_UNIT_VECTOR", "off")]))?.unit_vectors);
        assert!(matches!(read(&reader(&[("CORPUS_UNIT_VECTOR", "yes")])), Err(ConfigError::Invalid("CORPUS_UNIT_VECTOR"))));
        Ok(())
    }

    #[test]
    fn case_text_rare_reads_zero_to_eight_only() -> Result<(), ConfigError> {
        assert_eq!(read(&reader(&[("CORPUS_CASE_TEXT_RARE", "4")]))?.case_text_rare, 4);
        assert!(matches!(read(&reader(&[("CORPUS_CASE_TEXT_RARE", "9")])), Err(ConfigError::Invalid("CORPUS_CASE_TEXT_RARE"))));
        Ok(())
    }

    #[test]
    fn case_phrase_reads_off_add_or_replace_only() -> Result<(), ConfigError> {
        assert_eq!(read(&reader(&[("CORPUS_CASE_PHRASE", "off")]))?.case_phrase, CasePhrase::Off);
        assert_eq!(read(&reader(&[("CORPUS_CASE_PHRASE", "add")]))?.case_phrase, CasePhrase::Add);
        assert_eq!(read(&reader(&[("CORPUS_CASE_PHRASE", "replace")]))?.case_phrase, CasePhrase::Replace);
        assert!(matches!(read(&reader(&[("CORPUS_CASE_PHRASE", "on")])), Err(ConfigError::Invalid("CORPUS_CASE_PHRASE"))));
        Ok(())
    }

    #[test]
    fn case_vector_branch_is_read_as_on_or_off_only() -> Result<(), ConfigError> {
        assert!(read(&reader(&[("CORPUS_CASE_VECTOR", "on")]))?.case_vectors);
        assert!(!read(&reader(&[("CORPUS_CASE_VECTOR", "off")]))?.case_vectors);
        assert!(matches!(read(&reader(&[("CORPUS_CASE_VECTOR", "1")])), Err(ConfigError::Invalid("CORPUS_CASE_VECTOR"))));
        Ok(())
    }

    #[test]
    fn case_weights_and_case_index_are_read() -> Result<(), ConfigError> {
        let cfg = read(&reader(&[("CORPUS_CASE_WEIGHTS", "1.5,0.4"), ("CORPUS_CASE_KEYWORD_INDEX", "/x/cases")]))?;
        assert_eq!(cfg.case_weights, CaseWeights { holding: 1.5, overruled: 0.4 });
        assert_eq!(cfg.case_keyword_index, Some(std::path::PathBuf::from("/x/cases")));
        assert!(matches!(read(&reader(&[("CORPUS_CASE_WEIGHTS", "0.5,0.4")])), Err(ConfigError::Invalid("CORPUS_CASE_WEIGHTS"))));
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

    #[test]
    fn vertex_embed_needs_model_and_key_only_and_hides_key() -> Result<(), ConfigError> {
        let cfg = read(&reader(&[
            ("EMBED_PROVIDER", "vertex"),
            ("EMBED_MODEL", "gemini-embedding-001"),
            ("EMBED_API_KEY", "vk-secret-9"),
            ("VERTEX_LOCATION", "global"),
        ]))?;
        let embed = cfg.embed.ok_or(ConfigError::Invalid("EMBED_MODEL"))?;
        assert!(embed.vertex.is_some());
        assert!(!format!("{embed:?}").contains("vk-secret-9"));
        let missing = read(&reader(&[("EMBED_PROVIDER", "vertex"), ("EMBED_MODEL", "m")]));
        assert!(matches!(missing, Err(ConfigError::Missing(ref m)) if m == "EMBED_API_KEY"));
        assert!(matches!(read(&reader(&[("EMBED_PROVIDER", "other")])), Err(ConfigError::Invalid("EMBED_PROVIDER"))));
        let gm = read(&reader(&[("EMBED_PROVIDER", "gemini"), ("EMBED_MODEL", "gemini-embedding-001"), ("EMBED_API_KEY", "vk-secret-9")]))?;
        let flavor = gm.embed.and_then(|e| e.vertex).map(|v| v.flavor());
        assert_eq!(flavor, Some(llm::vertex::endpoint::Flavor::GeminiApi));
        Ok(())
    }

}
