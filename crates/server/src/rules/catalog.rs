use engine::{ClaimGraph, Evidence, GateType, NodeId};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogItem {
    pub claim_type: String,
    pub node_id: NodeId,
    pub label: String,
    pub gate: GateType,
    pub evidence: Vec<Evidence>,
}

pub fn items(claim_type: &str, tree: &ClaimGraph) -> Vec<CatalogItem> {
    let order = tree
        .topo_order()
        .unwrap_or_else(|_| tree.nodes().map(|n| n.id.clone()).collect());
    order
        .iter()
        .filter_map(|id| tree.node(id))
        .map(|n| CatalogItem {
            claim_type: claim_type.to_string(),
            node_id: n.id.clone(),
            label: n.label.clone(),
            gate: n.gate,
            evidence: n.evidence.clone(),
        })
        .collect()
}
