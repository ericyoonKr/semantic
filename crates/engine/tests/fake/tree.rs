#![allow(dead_code)]

use std::collections::HashMap;
use std::time::Duration;

use docmodel::{Anchor, FactSource};
use engine::{
    Burden, ClaimEdge, ClaimGraph, ClaimNode, ClaimType, EngineError, ExpectedCounts,
    FailureReason, GateType, LeafResult, NodeId, Relation, Verdict,
};

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

pub const TIMEOUT: Duration = Duration::from_secs(5);

pub fn id(s: &str) -> NodeId {
    NodeId::new(s)
}

pub fn node(name: &str, gate: GateType) -> ClaimNode {
    let leaf = gate == GateType::Leaf;
    ClaimNode {
        id: id(name),
        claim_type: ClaimType("T".into()),
        label: format!("label-{name}"),
        gate,
        burden: Burden::Pending,
        proposition: leaf.then(|| format!("proposition-{name}")),
        ask: leaf.then(|| format!("ask-{name}")),
        negation_of: None,
        arithmetic: None,
        period_rule: None,
        evidence: Vec::new(),
    }
}

pub fn edge(parent: &str, child: &str, relation: Relation) -> ClaimEdge {
    ClaimEdge {
        parent: id(parent),
        child: id(child),
        relation,
    }
}

pub fn rows() -> (Vec<ClaimNode>, Vec<ClaimEdge>) {
    use GateType::{And, Leaf, Or};
    use Relation::{Defense, Element, Rebuttal};
    let nodes = vec![
        node("ROOT", And),
        node("L1", Leaf),
        node("L2", Leaf),
        node("G", Or),
        node("L3", Leaf),
        node("L4", Leaf),
        node("D1", And),
        node("L5", Leaf),
        node("L6", Leaf),
        node("R1", Leaf),
    ];
    let edges = vec![
        edge("ROOT", "L1", Element),
        edge("ROOT", "L2", Element),
        edge("ROOT", "G", Element),
        edge("G", "L3", Element),
        edge("G", "L4", Element),
        edge("ROOT", "D1", Defense),
        edge("D1", "L5", Element),
        edge("D1", "L6", Element),
        edge("D1", "R1", Rebuttal),
    ];
    (nodes, edges)
}

pub const COUNTS: ExpectedCounts = ExpectedCounts {
    nodes: 10,
    edges: 9,
    leaves: 7,
};

pub fn graph() -> Result<ClaimGraph, EngineError> {
    let (nodes, edges) = rows();
    ClaimGraph::from_rows(nodes, edges, &COUNTS)
}

pub fn judged(value: bool) -> LeafResult {
    LeafResult::Judged {
        value,
        anchor: Anchor {
            source: FactSource::UserMessage {
                message_id: "m1".into(),
            },
            page: None,
            bbox: None,
            path: None,
            span: None,
        },
        quote: "q".into(),
    }
}

pub fn unknown() -> LeafResult {
    LeafResult::Unknown {
        failure: FailureReason::NoEvidence("none".into()),
    }
}

pub fn facts(pairs: &[(&str, LeafResult)]) -> HashMap<NodeId, LeafResult> {
    pairs.iter().map(|(k, v)| (id(k), v.clone())).collect()
}

pub fn judge(
    graph: &ClaimGraph,
    root: &str,
    leaves: &HashMap<NodeId, LeafResult>,
    assumptions: &HashMap<NodeId, bool>,
) -> Result<Verdict, EngineError> {
    let sub = graph.subtree(&id(root))?;
    let raw = engine::check(&sub, leaves, assumptions, TIMEOUT)?;
    engine::from_raw(raw, graph, &sub, leaves, assumptions)
}

pub fn judge_both(
    graph: &ClaimGraph,
    root: &str,
    leaves: &HashMap<NodeId, LeafResult>,
    assumptions: &HashMap<NodeId, bool>,
) -> Result<Verdict, EngineError> {
    let sub = graph.subtree(&id(root))?;
    let checked = engine::check_both(&sub, leaves, &HashMap::new(), assumptions, TIMEOUT)?;
    engine::from_checked(checked, graph, &sub, leaves, assumptions)
}

pub fn ids_of<T>(pairs: &[(NodeId, T)]) -> Vec<&str> {
    pairs.iter().map(|(n, _)| n.as_str()).collect()
}
