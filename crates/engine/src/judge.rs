use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::compile::{RawVerdict, SolverResult, check};
use crate::deps::{AuthoritySearch, EngineDeps, FactProvider, LeafCache, UserFact};
use crate::error::EngineError;
use crate::graph::ClaimGraph;
use crate::leaf::{CANCELLED, judge_all};
use crate::prompt::prompt_version;
use crate::schema::{FailureReason, LeafResult, NodeId};
use crate::slices::{SlicePlan, build_leaf_requests};
use crate::verdict::{Verdict, from_raw};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JudgeRequest {
    pub session: String,
    pub root: NodeId,
    pub assumptions: HashMap<NodeId, bool>,
    pub user_facts: Vec<UserFact>,
}

pub async fn judge<F, A, C>(
    graph: &ClaimGraph,
    req: JudgeRequest,
    deps: &EngineDeps<F, A, C>,
    cancel: &CancellationToken,
) -> Result<Verdict, EngineError>
where
    F: FactProvider,
    A: AuthoritySearch,
    C: LeafCache,
{
    let subtree = graph.subtree(&req.root)?;
    if let Some(stray) = req.assumptions.keys().find(|id| !subtree.contains(id)) {
        return Err(EngineError::UnknownNode(stray.clone()));
    }
    if let Some(leaf) = subtree.leaves().find(|l| l.arithmetic.is_some()) {
        return Err(EngineError::ArithmeticUnsupported(leaf.id.clone()));
    }
    let plan = SlicePlan {
        budget_tokens: deps.settings.fact_budget_tokens,
        candidate_k: deps.settings.candidate_k,
        authority_k: deps.settings.authority_k,
        model_id: deps.llm.model(),
        prompt_version: prompt_version(),
    };
    let user_facts: Arc<[UserFact]> = req.user_facts.into();
    let prepared = tokio::select! {
        biased;
        () = cancel.cancelled() => None,
        prepared = build_leaf_requests(
            &subtree,
            &req.session,
            user_facts,
            &deps.facts,
            &deps.authority,
            &plan,
        ) => Some(prepared),
    };
    let leaves = match prepared {
        Some(prepared) => judge_all(prepared, deps, &req.session, cancel).await,
        None => cancelled_all(&subtree),
    };
    let (raw, subtree, leaves, assumptions) =
        solve(subtree, leaves, req.assumptions, &req.root, deps).await?;
    from_raw(raw, graph, &subtree, &leaves, &assumptions)
}

type Solved = (
    RawVerdict,
    ClaimGraph,
    HashMap<NodeId, LeafResult>,
    HashMap<NodeId, bool>,
);

async fn solve<F, A, C>(
    subtree: ClaimGraph,
    leaves: HashMap<NodeId, LeafResult>,
    assumptions: HashMap<NodeId, bool>,
    root: &NodeId,
    deps: &EngineDeps<F, A, C>,
) -> Result<Solved, EngineError>
where
    F: FactProvider,
    A: AuthoritySearch,
    C: LeafCache,
{
    let timeout = deps.settings.solver_timeout;
    let shared = Arc::new((subtree, leaves, assumptions));
    let input = shared.clone();
    let joined =
        tokio::task::spawn_blocking(move || check(&input.0, &input.1, &input.2, timeout)).await;
    let raw = match joined {
        Ok(result) => result?,
        Err(error) => {
            tracing::error!(%error, "engine.judge.solver");
            RawVerdict {
                scope: root.clone(),
                holds: SolverResult::Unknown,
                fails: SolverResult::Unknown,
                core: Vec::new(),
                model: None,
            }
        }
    };
    let (subtree, leaves, assumptions) = match Arc::try_unwrap(shared) {
        Ok(owned) => owned,
        Err(still_shared) => (*still_shared).clone(),
    };
    Ok((raw, subtree, leaves, assumptions))
}

fn cancelled_all(subtree: &ClaimGraph) -> HashMap<NodeId, LeafResult> {
    subtree
        .leaves()
        .map(|leaf| {
            (
                leaf.id.clone(),
                LeafResult::Unknown {
                    failure: FailureReason::Cancelled(CANCELLED.to_string()),
                },
            )
        })
        .collect()
}
