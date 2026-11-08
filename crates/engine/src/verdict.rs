use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::compile::{Checked, RawVerdict, SolverResult};
use crate::error::EngineError;
use crate::graph::ClaimGraph;
use crate::period::PeriodReport;
use crate::schema::{Burden, FailureReason, GateType, LeafResult, NodeId, Trit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Established,
    NotEstablished,
    Reserved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub outcome: Outcome,
    pub scope: NodeId,
    pub unsat_core: Vec<(NodeId, String)>,
    pub unknown_leaves: Vec<(NodeId, FailureReason)>,
    pub assumed: Vec<(NodeId, bool)>,
    pub model_hint: Option<Vec<(NodeId, bool)>>,
    pub needed_facts: Vec<(NodeId, String)>,
    pub path: Vec<Vec<(NodeId, String, GateType)>>,
    #[serde(default)]
    pub periods: Vec<PeriodReport>,
    #[serde(default)]
    pub burden: Option<BurdenVerdict>,
    #[serde(default)]
    pub reductions: Vec<ReductionNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BurdenVerdict {
    pub outcome: Outcome,
    pub unsat_core: Vec<(NodeId, String)>,
    pub fixed: Vec<(NodeId, Burden)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReductionNote {
    pub node_id: NodeId,
    pub value: Trit,
    pub failure: Option<FailureReason>,
}

pub fn outcome_of(holds: SolverResult, fails: SolverResult) -> Outcome {
    match (holds, fails) {
        (SolverResult::Sat, SolverResult::Unsat) => Outcome::Established,
        (SolverResult::Unsat, SolverResult::Sat) => Outcome::NotEstablished,
        _ => Outcome::Reserved,
    }
}

pub fn from_raw(
    raw: RawVerdict,
    graph: &ClaimGraph,
    subtree: &ClaimGraph,
    leaves: &HashMap<NodeId, LeafResult>,
    assumptions: &HashMap<NodeId, bool>,
) -> Result<Verdict, EngineError> {
    let outcome = outcome_of(raw.holds, raw.fails);
    let label = |id: &NodeId| subtree.node(id).map(|n| n.label.clone()).unwrap_or_default();
    let unsat_core = raw.core.iter().map(|id| (id.clone(), label(id))).collect();
    let mut unknown_leaves = Vec::new();
    let mut needed_facts = Vec::new();
    let mut reductions = Vec::new();
    for leaf in subtree.leaves() {
        let reduction = subtree.is_reduction(&leaf.id);
        let result = leaves.get(&leaf.id);
        if reduction {
            reductions.push(reduction_note(&leaf.id, result));
        }
        if let Some(LeafResult::Unknown { failure }) = result {
            if !reduction {
                unknown_leaves.push((leaf.id.clone(), failure.clone()));
            }
            if let Some(ask) = &leaf.ask {
                needed_facts.push((leaf.id.clone(), ask.clone()));
            }
        }
    }
    let assumed = subtree
        .nodes()
        .filter_map(|n| assumptions.get(&n.id).map(|v| (n.id.clone(), *v)))
        .collect();
    let model_hint = match (outcome, raw.holds) {
        (Outcome::Reserved, SolverResult::Sat) => raw.model,
        _ => None,
    };
    let path = graph
        .path_to_roots(&raw.scope)?
        .into_iter()
        .map(|ids| {
            ids.into_iter()
                .filter_map(|id| {
                    let node = graph.node(&id)?;
                    Some((id, node.label.clone(), node.gate))
                })
                .collect()
        })
        .collect();
    Ok(Verdict {
        outcome,
        scope: raw.scope,
        unsat_core,
        unknown_leaves,
        assumed,
        model_hint,
        needed_facts,
        path,
        periods: Vec::new(),
        burden: None,
        reductions,
    })
}

fn reduction_note(id: &NodeId, result: Option<&LeafResult>) -> ReductionNote {
    let (value, failure) = match result {
        Some(LeafResult::Judged { value: true, .. }) => (Trit::True, None),
        Some(LeafResult::Judged { value: false, .. }) => (Trit::False, None),
        Some(LeafResult::Unknown { failure }) => (Trit::Unknown, Some(failure.clone())),
        None => (Trit::Unknown, None),
    };
    ReductionNote {
        node_id: id.clone(),
        value,
        failure,
    }
}

pub fn from_checked(
    checked: Checked,
    graph: &ClaimGraph,
    subtree: &ClaimGraph,
    leaves: &HashMap<NodeId, LeafResult>,
    assumptions: &HashMap<NodeId, bool>,
) -> Result<Verdict, EngineError> {
    let Checked {
        facts,
        burden,
        fixed,
    } = checked;
    let mut verdict = from_raw(facts, graph, subtree, leaves, assumptions)?;
    let label = |id: &NodeId| subtree.node(id).map(|n| n.label.clone()).unwrap_or_default();
    verdict.burden = Some(BurdenVerdict {
        outcome: outcome_of(burden.holds, burden.fails),
        unsat_core: burden.core.iter().map(|id| (id.clone(), label(id))).collect(),
        fixed,
    });
    Ok(verdict)
}
