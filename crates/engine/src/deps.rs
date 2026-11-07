use std::future::Future;
use std::sync::Arc;

use docmodel::{Anchor, FactSource, locate_quote};
use llm::{LlmClient, LlmLimiter};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::schema::LeafResult;
use crate::settings::EngineSettings;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DepError {
    #[error("외부 기능을 쓸 수 없음: {0}")]
    Unavailable(String),
    #[error("외부 기능 오류: {0}")]
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserFact {
    pub fact_id: String,
    pub message_id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passage {
    pub index: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityHit {
    pub source: String,
    pub text: String,
    pub citation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AuthoritySnapshot {
    pub hits: Vec<AuthorityHit>,
    pub snapshot_hash: String,
    pub degraded: bool,
}

pub trait FactProvider: Send + Sync {
    fn paged_text(&self, session: &str) -> impl Future<Output = Result<Arc<str>, DepError>> + Send;

    fn candidates(
        &self,
        session: &str,
        proposition: &str,
        k: usize,
    ) -> impl Future<Output = Result<Vec<Passage>, DepError>> + Send;

    fn locate(
        &self,
        session: &str,
        quote: &str,
    ) -> impl Future<Output = Result<Option<Anchor>, DepError>> + Send;

    fn locate_user(&self, fact: &UserFact, quote: &str) -> Option<Anchor> {
        let span = locate_quote(&fact.text, quote)?;
        Some(Anchor {
            source: FactSource::UserMessage {
                message_id: fact.message_id.clone(),
            },
            page: None,
            bbox: None,
            path: None,
            span: Some(span),
        })
    }
}

pub trait AuthoritySearch: Send + Sync {
    fn search(
        &self,
        query: &str,
        k: usize,
    ) -> impl Future<Output = Result<AuthoritySnapshot, DepError>> + Send;
}

pub trait LeafCache: Send + Sync {
    fn get(&self, key: &str) -> impl Future<Output = Result<Option<LeafResult>, DepError>> + Send;

    fn put(
        &self,
        key: &str,
        value: &LeafResult,
    ) -> impl Future<Output = Result<(), DepError>> + Send;
}

pub struct EngineDeps<F: FactProvider, A: AuthoritySearch, C: LeafCache> {
    pub llm: LlmClient,
    pub limiter: LlmLimiter,
    pub facts: F,
    pub authority: A,
    pub cache: C,
    pub settings: EngineSettings,
}
