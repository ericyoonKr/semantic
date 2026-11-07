use std::sync::Arc;

use futures::future::join_all;
use sha2::{Digest, Sha256};

use crate::deps::{AuthoritySearch, AuthoritySnapshot, DepError, FactProvider, Passage, UserFact};
use crate::graph::ClaimGraph;
use crate::prompt::hex;
use crate::schema::{ClaimNode, FailureReason, NodeId, cache_key};

pub const FACTS_UNAVAILABLE: &str = "사실 자료를 가져오지 못함";
pub const AUTHORITY_UNAVAILABLE: &str = "근거 자료를 검색하지 못함";
pub const NO_FACTS: &str = "업로드 문서와 사용자 진술이 없음";

#[derive(Debug, Clone, PartialEq)]
pub enum FactDocument {
    Absent,
    Whole(Arc<str>),
    Passages(Vec<Passage>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FactSlice {
    pub document: FactDocument,
    pub user_facts: Arc<[UserFact]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LeafRequest {
    pub node_id: NodeId,
    pub proposition: String,
    pub negation_of: Option<NodeId>,
    pub facts: FactSlice,
    pub authority: AuthoritySnapshot,
    pub cache_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prepared {
    Ready(LeafRequest),
    NoEvidence {
        node_id: NodeId,
        cache_key: String,
    },
    Failed {
        node_id: NodeId,
        failure: FailureReason,
    },
}

impl Prepared {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Prepared::Ready(req) => &req.node_id,
            Prepared::NoEvidence { node_id, .. } | Prepared::Failed { node_id, .. } => node_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SlicePlan<'a> {
    pub budget_tokens: usize,
    pub candidate_k: usize,
    pub authority_k: usize,
    pub model_id: &'a str,
    pub prompt_version: &'a str,
}

pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count()
}

struct Shared<'a, F, A> {
    session: &'a str,
    text: Arc<str>,
    whole: bool,
    doc_hash: String,
    user_facts: Arc<[UserFact]>,
    user_hash: String,
    facts: &'a F,
    authority: &'a A,
    plan: &'a SlicePlan<'a>,
}

pub async fn build_leaf_requests<F: FactProvider, A: AuthoritySearch>(
    subtree: &ClaimGraph,
    session: &str,
    user_facts: Arc<[UserFact]>,
    facts: &F,
    authority: &A,
    plan: &SlicePlan<'_>,
) -> Vec<Prepared> {
    let leaves: Vec<&ClaimNode> = subtree.leaves().collect();
    let text = match facts.paged_text(session).await {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(%error, "engine.slices.paged_text");
            return leaves
                .iter()
                .map(|leaf| failed(&leaf.id, FACTS_UNAVAILABLE))
                .collect();
        }
    };
    let whole = estimate_tokens(&text) <= plan.budget_tokens;
    let doc_hash = if whole && has_text(&text) {
        digest(&[b"whole", text.as_bytes()])
    } else {
        String::new()
    };
    let shared = Shared {
        session,
        user_hash: user_facts_hash(&user_facts),
        user_facts,
        text,
        whole,
        doc_hash,
        facts,
        authority,
        plan,
    };
    join_all(leaves.iter().map(|leaf| prepare(leaf, &shared))).await
}

async fn prepare<F: FactProvider, A: AuthoritySearch>(
    leaf: &ClaimNode,
    shared: &Shared<'_, F, A>,
) -> Prepared {
    let proposition = leaf.proposition.clone().unwrap_or_default();
    if !has_text(&shared.text) && shared.user_facts.is_empty() {
        return no_evidence(leaf, shared, &FactDocument::Absent);
    }
    let document = async {
        if !has_text(&shared.text) {
            Ok(FactDocument::Absent)
        } else if shared.whole {
            Ok(FactDocument::Whole(shared.text.clone()))
        } else {
            shared
                .facts
                .candidates(shared.session, &proposition, shared.plan.candidate_k)
                .await
                .map(|found| {
                    if found.iter().all(|p| !has_text(&p.text)) {
                        FactDocument::Absent
                    } else {
                        FactDocument::Passages(found)
                    }
                })
        }
    };
    let search = async {
        if leaf.arithmetic.is_some() {
            Ok(AuthoritySnapshot::default())
        } else {
            shared
                .authority
                .search(&proposition, shared.plan.authority_k)
                .await
        }
    };
    let (document, snapshot) = tokio::join!(document, search);
    let document = match document {
        Ok(document) => document,
        Err(error) => return dep_failed(&leaf.id, FACTS_UNAVAILABLE, &error),
    };
    if document == FactDocument::Absent && shared.user_facts.is_empty() {
        return no_evidence(leaf, shared, &document);
    }
    let authority = match snapshot {
        Ok(snapshot) => snapshot,
        Err(error) => return dep_failed(&leaf.id, AUTHORITY_UNAVAILABLE, &error),
    };
    let key = cache_key(
        leaf.id.as_str(),
        &fact_hash(&document, shared),
        &authority.snapshot_hash,
        shared.plan.model_id,
        shared.plan.prompt_version,
    );
    Prepared::Ready(LeafRequest {
        node_id: leaf.id.clone(),
        proposition,
        negation_of: leaf.negation_of.clone(),
        facts: FactSlice {
            document,
            user_facts: shared.user_facts.clone(),
        },
        authority,
        cache_key: key,
    })
}

fn no_evidence<F, A>(
    leaf: &ClaimNode,
    shared: &Shared<'_, F, A>,
    document: &FactDocument,
) -> Prepared {
    Prepared::NoEvidence {
        node_id: leaf.id.clone(),
        cache_key: cache_key(
            leaf.id.as_str(),
            &fact_hash(document, shared),
            "",
            shared.plan.model_id,
            shared.plan.prompt_version,
        ),
    }
}

fn failed(id: &NodeId, reason: &str) -> Prepared {
    Prepared::Failed {
        node_id: id.clone(),
        failure: FailureReason::NoEvidence(reason.to_string()),
    }
}

fn dep_failed(id: &NodeId, reason: &str, error: &DepError) -> Prepared {
    tracing::warn!(node = %id, %error, "engine.slices.dependency");
    failed(id, reason)
}

fn has_text(text: &str) -> bool {
    !text.trim().is_empty()
}

fn fact_hash<F, A>(document: &FactDocument, shared: &Shared<'_, F, A>) -> String {
    let parts = match document {
        FactDocument::Absent => Parts::new().add(b"absent"),
        FactDocument::Whole(_) => Parts::new().add(b"whole").add(shared.doc_hash.as_bytes()),
        FactDocument::Passages(passages) => passages
            .iter()
            .fold(Parts::new().add(b"passages"), |parts, p| {
                parts.add(&p.index.to_le_bytes()).add(p.text.as_bytes())
            }),
    };
    parts.add(shared.user_hash.as_bytes()).finish()
}

fn user_facts_hash(facts: &[UserFact]) -> String {
    facts
        .iter()
        .fold(Parts::new(), |parts, f| {
            parts
                .add(f.fact_id.as_bytes())
                .add(f.message_id.as_bytes())
                .add(f.text.as_bytes())
        })
        .finish()
}

fn digest(parts: &[&[u8]]) -> String {
    parts
        .iter()
        .fold(Parts::new(), |acc, p| acc.add(p))
        .finish()
}

struct Parts(Sha256);

impl Parts {
    fn new() -> Self {
        Self(Sha256::new())
    }

    fn add(mut self, part: &[u8]) -> Self {
        self.0.update((part.len() as u64).to_le_bytes());
        self.0.update(part);
        self
    }

    fn finish(self) -> String {
        hex(&self.0.finalize())
    }
}
