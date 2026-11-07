use engine::{Burden, ClaimGraph, EngineError, ExpectedCounts, GateType, GraphIssue, NodeId};

use crate::source::RuleFile;

pub const CONCLUSION_WORDS: [&str; 4] = ["위법하다", "인정된다", "처분성이 있다", "재량을 남용했다"];

#[derive(Debug)]
pub struct Report {
    pub claim_type: String,
    pub expected: ExpectedCounts,
    pub actual: ExpectedCounts,
    pub structure: Vec<GraphIssue>,
    pub other: Vec<String>,
    pub banned: Vec<(NodeId, &'static str)>,
    pub burden_pending: Vec<NodeId>,
    pub burden_missing: Vec<NodeId>,
}

impl Report {
    pub fn structure_ok(&self) -> bool {
        self.structure.is_empty() && self.other.is_empty()
    }

    pub fn check_ok(&self) -> bool {
        self.structure_ok() && self.banned.is_empty() && self.burden_missing.is_empty()
    }

    pub fn build_ok(&self) -> bool {
        self.check_ok() && self.burden_pending.is_empty()
    }
}

pub fn validate(file: &RuleFile) -> Report {
    let rows = file.rows();
    let actual = ExpectedCounts {
        nodes: rows.nodes.len(),
        edges: rows.edges.len(),
        leaves: rows.nodes.iter().filter(|n| n.gate == GateType::Leaf).count(),
    };
    let banned = rows
        .nodes
        .iter()
        .flat_map(|n| {
            let text = n.proposition.as_deref().unwrap_or_default();
            banned_words(text).into_iter().map(|w| (n.id.clone(), w))
        })
        .collect();
    let burden_pending = if rows.burden_missing.is_empty() {
        rows.nodes
            .iter()
            .filter(|n| n.burden == Burden::Pending)
            .map(|n| n.id.clone())
            .collect()
    } else {
        Vec::new()
    };
    let (structure, other) = match ClaimGraph::from_rows(rows.nodes, rows.edges, &file.expected) {
        Ok(_) => (Vec::new(), Vec::new()),
        Err(EngineError::Graph(issues)) => (issues, Vec::new()),
        Err(e) => (Vec::new(), vec![e.to_string()]),
    };
    Report {
        claim_type: file.claim_type.clone(),
        expected: file.expected,
        actual,
        structure,
        other,
        banned,
        burden_pending,
        burden_missing: rows.burden_missing,
    }
}

pub fn banned_words(text: &str) -> Vec<&'static str> {
    CONCLUSION_WORDS
        .into_iter()
        .filter(|w| text.contains(w))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{banned_words, validate};
    use crate::source::parse;

    const TREE: &str = "claim_type: T1
burden_pending: true
expected: { nodes: 3, edges: 2, leaves: 2 }
nodes:
  - { id: ROOT, gate: AND, label: root }
  - { id: L1, gate: LEAF, label: one, proposition: p1, ask: a1 }
  - { id: L2, gate: LEAF, label: two, proposition: p2, ask: a2 }
edges:
  - { parent: ROOT, child: L1, relation: element }
  - { parent: ROOT, child: L2, relation: element }
";

    #[test]
    fn pending_burden_passes_check_but_not_build() -> Result<(), serde_yaml::Error> {
        let report = validate(&parse(TREE)?);
        assert!(report.check_ok());
        assert!(!report.build_ok());
        assert_eq!(report.burden_pending.len(), 3);
        Ok(())
    }

    #[test]
    fn missing_burden_count_mismatch_and_conclusion_fail_check() -> Result<(), serde_yaml::Error> {
        let text = TREE
            .replace("burden_pending: true\n", "")
            .replace("nodes: 3,", "nodes: 4,")
            .replace("p2", "그 처분은 위법하다");
        let report = validate(&parse(&text)?);
        assert!(!report.check_ok());
        assert_eq!(report.burden_missing.len(), 3);
        assert_eq!(report.banned.len(), 1);
        assert_eq!(report.structure.len(), 1);
        Ok(())
    }

    #[test]
    fn unknown_yaml_field_is_refused() {
        assert!(parse(&TREE.replace("ask: a1", "aks: a1")).is_err());
    }

    #[test]
    fn banned_word_check_passes_clean_and_stops_conclusions() {
        let clean = [
            "그 사람이 그 날 문서를 받았다",
            "그 행위가 규정을 어겼다",
            "법이 정한 기간 안에 신청했다",
            "위법 여부를 묻는 문장이 아니다",
            "인정 절차를 거쳤다",
        ];
        let bad = [
            "그 처분은 위법하다",
            "요건이 인정된다",
            "그 행위에 처분성이 있다",
            "행정청이 재량을 남용했다",
            "위법하다고 볼 수 있고 인정된다",
        ];
        let passed = clean.iter().filter(|t| banned_words(t).is_empty()).count();
        let caught = bad.iter().filter(|t| !banned_words(t).is_empty()).count();
        assert_eq!((passed, caught), (clean.len(), bad.len()));
        assert_eq!(banned_words(bad[4]), vec!["위법하다", "인정된다"]);
    }
}
