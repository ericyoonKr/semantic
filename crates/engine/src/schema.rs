use std::fmt;

use docmodel::Anchor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClaimType(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum GateType {
    Leaf,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Relation {
    Element,
    Defense,
    Rebuttal,
    Reduction,
}

impl Relation {
    pub const ALL: [Relation; 4] = [
        Relation::Element,
        Relation::Defense,
        Relation::Rebuttal,
        Relation::Reduction,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Element => "element",
            Relation::Defense => "defense",
            Relation::Rebuttal => "rebuttal",
            Relation::Reduction => "reduction",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Burden {
    Pending,
    Plaintiff,
    Defendant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodRule {
    Administrative,
    AdministrativeRestrictive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub law: String,
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ArithSpec {
    WithinDays { from: String, to: String, days: u32 },
    WithinYears { from: String, to: String, years: u32 },
    ElapsedYears { from: String, to: String, years: u32 },
}

impl ArithSpec {
    pub fn elapsed(&self) -> bool {
        matches!(self, ArithSpec::ElapsedYears { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimNode {
    pub id: NodeId,
    pub claim_type: ClaimType,
    pub label: String,
    pub gate: GateType,
    pub burden: Burden,
    pub proposition: Option<String>,
    pub ask: Option<String>,
    pub negation_of: Option<NodeId>,
    pub arithmetic: Option<ArithSpec>,
    #[serde(default)]
    pub period_rule: Option<PeriodRule>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimEdge {
    pub parent: NodeId,
    pub child: NodeId,
    pub relation: Relation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExpectedCounts {
    pub nodes: usize,
    pub edges: usize,
    pub leaves: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trit {
    True,
    False,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureReason {
    LlmTimeout,
    LlmParseFailed,
    Contradiction,
    NoEvidence(String),
    QuoteNotFound,
    ModelServerDown,
    Cancelled(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LeafResult {
    Judged {
        value: bool,
        anchor: Anchor,
        quote: String,
    },
    Unknown {
        failure: FailureReason,
    },
}

pub fn cache_key(
    rule_id: &str,
    fact_hash: &str,
    authority_hash: &str,
    model_id: &str,
    prompt_version: &str,
) -> String {
    let mut hasher = Sha256::new();
    for part in [rule_id, fact_hash, authority_hash, model_id, prompt_version] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::cache_key;

    #[test]
    fn cache_key_changes_with_every_part_and_keeps_boundaries() {
        let base = cache_key("r", "f", "a", "m", "p");
        assert_eq!(base.len(), 64);
        assert_eq!(base, cache_key("r", "f", "a", "m", "p"));
        assert_ne!(base, cache_key("r2", "f", "a", "m", "p"));
        assert_ne!(base, cache_key("r", "f2", "a", "m", "p"));
        assert_ne!(base, cache_key("r", "f", "a2", "m", "p"));
        assert_ne!(base, cache_key("r", "f", "a", "m2", "p"));
        assert_ne!(base, cache_key("r", "f", "a", "m", "p2"));
        assert_ne!(cache_key("ab", "c", "", "", ""), cache_key("a", "bc", "", "", ""));
    }
}
