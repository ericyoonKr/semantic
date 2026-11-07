#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use docmodel::{Anchor, FactSource, FileId, locate_quote};
use engine::{
    AuthorityHit, AuthoritySearch, AuthoritySnapshot, ClaimGraph, DepError, EngineDeps,
    EngineError, EngineSettings, ExpectedCounts, FactProvider, GateType, LeafCache, LeafResult,
    NodeId, Prepared, Relation, SlicePlan, UserFact,
};
use llm::{LlmClient, LlmLimiter};

use super::tree::{edge, node};

pub const FILE: &str = "file-1";

pub struct Doc {
    pub text: Arc<str>,
}

impl FactProvider for Doc {
    async fn paged_text(&self, _session: &str) -> Result<Arc<str>, DepError> {
        Ok(self.text.clone())
    }

    async fn candidates(
        &self,
        _session: &str,
        _proposition: &str,
        _k: usize,
    ) -> Result<Vec<engine::Passage>, DepError> {
        Ok(Vec::new())
    }

    async fn locate(&self, _session: &str, quote: &str) -> Result<Option<Anchor>, DepError> {
        Ok(locate_quote(&self.text, quote).map(|span| Anchor {
            source: FactSource::File(FileId(FILE.into())),
            page: None,
            bbox: None,
            path: None,
            span: Some(span),
        }))
    }
}

pub struct Auth;

impl AuthoritySearch for Auth {
    async fn search(&self, _query: &str, _k: usize) -> Result<AuthoritySnapshot, DepError> {
        Ok(AuthoritySnapshot {
            hits: vec![AuthorityHit {
                source: "corpus".into(),
                text: "authority text".into(),
                citation: "citation-1".into(),
            }],
            snapshot_hash: "auth-hash-1".into(),
            degraded: false,
        })
    }
}

#[derive(Default)]
pub struct MemCache {
    pub map: Mutex<HashMap<String, LeafResult>>,
    pub gets: AtomicUsize,
    pub puts: AtomicUsize,
}

impl MemCache {
    pub fn len(&self) -> usize {
        self.map.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn values(&self) -> Vec<LeafResult> {
        self.map
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }
}

impl LeafCache for MemCache {
    async fn get(&self, key: &str) -> Result<Option<LeafResult>, DepError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.map
            .lock()
            .map(|m| m.get(key).cloned())
            .map_err(|_| DepError::Other("lock".into()))
    }

    async fn put(&self, key: &str, value: &LeafResult) -> Result<(), DepError> {
        self.puts.fetch_add(1, Ordering::SeqCst);
        self.map
            .lock()
            .map(|mut m| {
                m.insert(key.to_string(), value.clone());
            })
            .map_err(|_| DepError::Other("lock".into()))
    }
}

pub type Deps = EngineDeps<Doc, Auth, MemCache>;

pub fn deps(url: &str, text: &str) -> Result<Deps, llm::LlmError> {
    let mut settings = EngineSettings::new(100_000, 3, 2, Duration::from_secs(5));
    settings.call.retries = 0;
    settings.call.backoff_base = Duration::from_millis(1);
    settings.call.deadline = Duration::from_secs(5);
    settings.leaf_deadline = Duration::from_secs(10);
    Ok(EngineDeps {
        llm: LlmClient::new(url, "k", "m")?,
        limiter: LlmLimiter::new(12),
        facts: Doc { text: text.into() },
        authority: Auth,
        cache: MemCache::default(),
        settings,
    })
}

pub fn proposition(name: &str) -> String {
    format!("proposition-{name}")
}

pub fn flat(
    gate: GateType,
    leaves: &[&str],
    negations: &[(&str, &str)],
) -> Result<ClaimGraph, EngineError> {
    let mut nodes = vec![node("ROOT", gate)];
    let mut edges = Vec::new();
    for name in leaves {
        let mut leaf = node(name, GateType::Leaf);
        if let Some((_, target)) = negations.iter().find(|(n, _)| n == name) {
            leaf.negation_of = Some(NodeId::new(*target));
        }
        nodes.push(leaf);
        edges.push(edge("ROOT", name, Relation::Element));
    }
    let counts = ExpectedCounts {
        nodes: nodes.len(),
        edges: edges.len(),
        leaves: leaves.len(),
    };
    ClaimGraph::from_rows(nodes, edges, &counts)
}

pub async fn prepare(graph: &ClaimGraph, deps: &Deps, user_facts: Vec<UserFact>) -> Vec<Prepared> {
    let plan = SlicePlan {
        budget_tokens: deps.settings.fact_budget_tokens,
        candidate_k: deps.settings.candidate_k,
        authority_k: deps.settings.authority_k,
        model_id: deps.llm.model(),
        prompt_version: engine::prompt_version(),
    };
    engine::build_leaf_requests(
        graph,
        "session-1",
        user_facts.into(),
        &deps.facts,
        &deps.authority,
        &plan,
    )
    .await
}
