use std::collections::HashMap;
use std::time::Duration;

use super::{PeriodFacts, RawVerdict, check_with_periods};
use crate::arith::rule_missing;
use crate::error::EngineError;
use crate::graph::ClaimGraph;
use crate::schema::{Burden, ClaimNode, GateType, LeafResult, NodeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub facts: RawVerdict,
    pub burden: RawVerdict,
    pub fixed: Vec<(NodeId, Burden)>,
}

fn open_leaf(
    node: &ClaimNode,
    leaves: &HashMap<NodeId, LeafResult>,
    assumptions: &HashMap<NodeId, bool>,
) -> bool {
    node.gate == GateType::Leaf
        && !assumptions.contains_key(&node.id)
        && matches!(leaves.get(&node.id), Some(LeafResult::Unknown { failure }) if !rule_missing(failure))
}

pub fn burden_fixes(
    subtree: &ClaimGraph,
    leaves: &HashMap<NodeId, LeafResult>,
    assumptions: &HashMap<NodeId, bool>,
) -> Vec<(NodeId, Burden)> {
    let partners_open = |id: &NodeId| {
        subtree
            .nodes()
            .filter(|n| n.negation_of.as_ref() == Some(id))
            .all(|n| open_leaf(n, leaves, assumptions))
    };
    subtree
        .leaves()
        .filter(|l| !subtree.is_reduction(&l.id))
        .filter(|l| open_leaf(l, leaves, assumptions))
        .filter(|l| l.burden != Burden::Pending)
        .filter(|l| l.negation_of.is_none() && partners_open(&l.id))
        .map(|l| (l.id.clone(), l.burden))
        .collect()
}

pub fn check_both(
    subtree: &ClaimGraph,
    leaves: &HashMap<NodeId, LeafResult>,
    periods: &HashMap<NodeId, PeriodFacts>,
    assumptions: &HashMap<NodeId, bool>,
    timeout: Duration,
) -> Result<Checked, EngineError> {
    let facts = check_with_periods(subtree, leaves, periods, assumptions, timeout)?;
    let fixed = burden_fixes(subtree, leaves, assumptions);
    if fixed.is_empty() {
        return Ok(Checked {
            burden: facts.clone(),
            facts,
            fixed,
        });
    }
    let mut applied = assumptions.clone();
    applied.extend(fixed.iter().map(|(id, _)| (id.clone(), false)));
    let burden = check_with_periods(subtree, leaves, periods, &applied, timeout)?;
    Ok(Checked {
        facts,
        burden,
        fixed,
    })
}
