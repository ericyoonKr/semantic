use engine::{EngineDeps, EngineSettings};
use llm::{LlmClient, LlmLimiter};
use sessdb::SessDb;

use crate::deps::{CorpusAuthority, LeafStore, SessionFacts};
use crate::judge::config::JudgeConfig;

pub type ServerDeps = EngineDeps<SessionFacts, CorpusAuthority, LeafStore>;

pub struct EngineParts {
    pub llm: LlmClient,
    pub limiter: LlmLimiter,
    pub settings: EngineSettings,
    pub cache: LeafStore,
}

impl EngineParts {
    pub fn build(cfg: &JudgeConfig, db: &SessDb) -> Option<Self> {
        let Some(engine) = cfg.engine.as_ref() else {
            tracing::warn!("model settings are missing; judging is off");
            return None;
        };
        let llm = match LlmClient::new(
            &engine.llm_base_url,
            &engine.llm_api_key,
            &engine.llm_model,
        ) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "model client could not be built; judging is off");
                return None;
            }
        };
        Some(Self {
            llm,
            limiter: LlmLimiter::new(cfg.leaf_permits),
            settings: EngineSettings::new(
                engine.fact_budget_tokens,
                engine.candidate_k,
                engine.authority_k,
                engine.solver_timeout,
            ),
            cache: LeafStore::new(db.clone(), cfg.leaf_cache_entries),
        })
    }

    pub fn deps(&self, db: &SessDb, session: &str, corpus: Option<retrieval::Corpus>) -> ServerDeps {
        EngineDeps {
            llm: self.llm.clone(),
            limiter: self.limiter.clone(),
            facts: SessionFacts::new(db.clone(), session),
            authority: CorpusAuthority::new(corpus),
            cache: self.cache.clone(),
            settings: self.settings.clone(),
        }
    }
}
