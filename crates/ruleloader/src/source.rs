use std::fs;
use std::path::Path;

use engine::{
    ArithSpec, Burden, ClaimEdge, ClaimNode, ClaimType, Evidence, ExpectedCounts, GateType,
    NodeId, PeriodRule, Relation,
};
use serde::{Deserialize, Serialize};

use crate::error::LoaderError;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleFile {
    pub claim_type: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub burden_pending: bool,
    pub expected: ExpectedCounts,
    pub nodes: Vec<RawNode>,
    pub edges: Vec<RawEdge>,
    #[serde(default)]
    pub excluded_from_tree: Vec<Excluded>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawNode {
    pub id: NodeId,
    pub gate: GateType,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negation_of: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arithmetic: Option<ArithSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_rule: Option<PeriodRule>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub needs_split: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burden: Option<Burden>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RawEdge {
    pub parent: NodeId,
    pub child: NodeId,
    pub relation: Relation,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Excluded {
    pub item: String,
    pub reason: String,
}

pub struct Rows {
    pub nodes: Vec<ClaimNode>,
    pub edges: Vec<ClaimEdge>,
    pub burden_missing: Vec<NodeId>,
}

pub fn read(path: &Path) -> Result<RuleFile, LoaderError> {
    let text = fs::read_to_string(path).map_err(|source| LoaderError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&text).map_err(|source| LoaderError::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

pub fn parse(text: &str) -> Result<RuleFile, serde_yaml::Error> {
    serde_yaml::from_str(text)
}

impl RuleFile {
    pub fn rows(&self) -> Rows {
        let mut burden_missing = Vec::new();
        let nodes = self
            .nodes
            .iter()
            .map(|n| {
                let burden = match (n.burden, self.burden_pending) {
                    (Some(b), _) => b,
                    (None, true) => Burden::Pending,
                    (None, false) => {
                        burden_missing.push(n.id.clone());
                        Burden::Pending
                    }
                };
                ClaimNode {
                    id: n.id.clone(),
                    claim_type: ClaimType(self.claim_type.clone()),
                    label: n.label.clone(),
                    gate: n.gate,
                    burden,
                    proposition: n.proposition.clone(),
                    ask: n.ask.clone(),
                    negation_of: n.negation_of.clone(),
                    arithmetic: n.arithmetic.clone(),
                    period_rule: n.period_rule,
                    evidence: n.evidence.clone(),
                }
            })
            .collect();
        let edges = self
            .edges
            .iter()
            .map(|e| ClaimEdge {
                parent: e.parent.clone(),
                child: e.child.clone(),
                relation: e.relation,
            })
            .collect();
        Rows {
            nodes,
            edges,
            burden_missing,
        }
    }
}
