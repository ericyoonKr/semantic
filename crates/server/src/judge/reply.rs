use engine::{ClaimGraph, GateType, NodeId, Outcome, Verdict};
use serde::Serialize;

use crate::verdict_text::{label_of, reason};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodeLine {
    pub node_id: NodeId,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnknownLine {
    pub node_id: NodeId,
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssumedLine {
    pub node_id: NodeId,
    pub label: String,
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerdictView {
    pub outcome: Outcome,
    pub scope: NodeId,
    pub unsat_core: Vec<NodeLine>,
    pub unknown_leaves: Vec<UnknownLine>,
    pub assumed: Vec<AssumedLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NeededFact {
    pub node_id: NodeId,
    pub ask: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathStep {
    pub node_id: NodeId,
    pub label: String,
    pub gate: GateType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JudgeReply {
    pub verdict: Option<VerdictView>,
    pub text: String,
    pub needed_facts: Vec<NeededFact>,
    pub path: Vec<PathStep>,
}

pub fn first_path(tree: &ClaimGraph, node: &NodeId) -> Vec<PathStep> {
    tree.path_to_roots(node)
        .ok()
        .and_then(|paths| paths.into_iter().next())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| {
            let n = tree.node(&id)?;
            Some(PathStep {
                label: n.label.clone(),
                gate: n.gate,
                node_id: id,
            })
        })
        .collect()
}

pub fn view(v: &Verdict, tree: &ClaimGraph) -> VerdictView {
    let line = |id: &NodeId| NodeLine {
        node_id: id.clone(),
        label: label_of(tree, id),
    };
    VerdictView {
        outcome: v.outcome,
        scope: v.scope.clone(),
        unsat_core: v.unsat_core.iter().map(|(id, _)| line(id)).collect(),
        unknown_leaves: v
            .unknown_leaves
            .iter()
            .map(|(id, f)| UnknownLine {
                node_id: id.clone(),
                label: label_of(tree, id),
                reason: reason(f),
            })
            .collect(),
        assumed: v
            .assumed
            .iter()
            .map(|(id, value)| AssumedLine {
                node_id: id.clone(),
                label: label_of(tree, id),
                value: *value,
            })
            .collect(),
    }
}

pub fn judged(v: &Verdict, tree: &ClaimGraph, text: String) -> JudgeReply {
    JudgeReply {
        verdict: Some(view(v, tree)),
        text,
        needed_facts: v
            .needed_facts
            .iter()
            .map(|(id, ask)| NeededFact {
                node_id: id.clone(),
                ask: ask.clone(),
            })
            .collect(),
        path: first_path(tree, &v.scope),
    }
}

pub fn not_run(tree: &ClaimGraph, root: &NodeId, text: String) -> JudgeReply {
    JudgeReply {
        verdict: None,
        text,
        needed_facts: Vec::new(),
        path: first_path(tree, root),
    }
}
