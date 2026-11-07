use std::collections::HashMap;
use std::fmt::Write;

use engine::{ClaimGraph, FailureReason, NodeId, Outcome, Verdict};

pub fn label_of(tree: &ClaimGraph, id: &NodeId) -> String {
    tree.node(id).map(|n| n.label.clone()).unwrap_or_default()
}

fn named(tree: &ClaimGraph, id: &NodeId) -> String {
    let label = label_of(tree, id);
    if label.is_empty() {
        id.to_string()
    } else {
        format!("{id} {label}")
    }
}

pub fn reason(f: &FailureReason) -> String {
    match f {
        FailureReason::LlmTimeout => "모델 응답 시간 초과".to_string(),
        FailureReason::LlmParseFailed => "모델 응답을 읽지 못함".to_string(),
        FailureReason::Contradiction => "긍정·부정 판정이 서로 맞지 않음".to_string(),
        FailureReason::NoEvidence(s) | FailureReason::Cancelled(s) => s.clone(),
        FailureReason::QuoteNotFound => "인용문이 원문에 없음".to_string(),
        FailureReason::ModelServerDown => "모델 서버 응답 없음".to_string(),
    }
}

fn outcome_word(o: Outcome) -> &'static str {
    match o {
        Outcome::Established => "성립",
        Outcome::NotEstablished => "불성립",
        Outcome::Reserved => "결론 유보",
    }
}

fn truth(v: bool) -> &'static str {
    if v { "참" } else { "거짓" }
}

pub fn unavailable(reason: &str) -> String {
    format!("판정 불가: {reason}")
}

fn core(out: &mut String, v: &Verdict, tree: &ClaimGraph) {
    if v.outcome != Outcome::NotEstablished || v.unsat_core.is_empty() {
        return;
    }
    out.push_str("불성립 이유:\n");
    for (id, _) in &v.unsat_core {
        let mark = if v.assumed.iter().any(|(a, _)| a == id) {
            " (가정)"
        } else {
            ""
        };
        let _ = writeln!(out, "- {}{mark}", named(tree, id));
    }
}

fn unknown(out: &mut String, v: &Verdict, tree: &ClaimGraph) {
    if v.unknown_leaves.is_empty() {
        return;
    }
    let total = tree
        .subtree(&v.scope)
        .map(|s| s.leaves().count())
        .unwrap_or(v.unknown_leaves.len());
    let _ = writeln!(
        out,
        "판정하지 못한 리프 (리프 {total}개 중 {}개):",
        v.unknown_leaves.len()
    );
    for (id, f) in &v.unknown_leaves {
        let _ = writeln!(out, "- {}: {} — 판정 불가", named(tree, id), reason(f));
    }
}

fn assumed(out: &mut String, v: &Verdict, tree: &ClaimGraph, said: &HashMap<NodeId, String>) {
    if v.assumed.is_empty() {
        return;
    }
    out.push_str("가정한 값 (문서 근거 없음·가정):\n");
    for (id, value) in &v.assumed {
        let _ = match said.get(id) {
            Some(words) => writeln!(
                out,
                "- {} = {} (사용자가 말한 원문: 「{words}」)",
                named(tree, id),
                truth(*value)
            ),
            None => writeln!(out, "- {} = {}", named(tree, id), truth(*value)),
        };
    }
}

fn hint(out: &mut String, v: &Verdict, tree: &ClaimGraph) {
    let Some(hint) = v.model_hint.as_ref().filter(|h| !h.is_empty()) else {
        return;
    };
    out.push_str("성립에 필요한 사실 조합 (가능한 조합 중 하나):\n");
    for (id, value) in hint {
        let _ = writeln!(out, "- {} = {}", named(tree, id), truth(*value));
    }
}

fn paths(out: &mut String, v: &Verdict) {
    if v.path.is_empty() {
        return;
    }
    out.push_str("루트까지의 경로:\n");
    for path in &v.path {
        let line = path
            .iter()
            .map(|(id, label, _)| {
                if label.is_empty() {
                    id.to_string()
                } else {
                    format!("{id} {label}")
                }
            })
            .collect::<Vec<_>>()
            .join(" → ");
        let _ = writeln!(out, "- {line}");
    }
}

pub fn render(
    v: &Verdict,
    tree: &ClaimGraph,
    said: &HashMap<NodeId, String>,
    interrupted: Option<&str>,
) -> String {
    let mut out = String::new();
    if let Some(r) = interrupted {
        let _ = writeln!(out, "{}", unavailable(r));
    }
    let _ = writeln!(
        out,
        "결론: {} — {}",
        named(tree, &v.scope),
        outcome_word(v.outcome)
    );
    let _ = writeln!(
        out,
        "이 판정은 {} 서브트리에 한정되며, 그 밖의 요건은 판정하지 않았음.",
        v.scope
    );
    core(&mut out, v, tree);
    unknown(&mut out, v, tree);
    assumed(&mut out, v, tree, said);
    hint(&mut out, v, tree);
    paths(&mut out, v);
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use engine::{Burden, ClaimEdge, ClaimNode, ClaimType, ExpectedCounts, GateType, Relation};

    use super::*;

    fn node(id: &str, gate: GateType) -> ClaimNode {
        let leaf = gate == GateType::Leaf;
        ClaimNode {
            id: NodeId::new(id),
            claim_type: ClaimType("T".into()),
            label: format!("{id}-label"),
            gate,
            burden: Burden::Pending,
            proposition: leaf.then(|| "p".to_string()),
            ask: leaf.then(|| "a".to_string()),
            negation_of: None,
            arithmetic: None,
            evidence: Vec::new(),
        }
    }

    fn tree() -> Result<ClaimGraph, engine::EngineError> {
        let edge = |c: &str| ClaimEdge {
            parent: NodeId::new("R"),
            child: NodeId::new(c),
            relation: Relation::Element,
        };
        ClaimGraph::from_rows(
            vec![node("R", GateType::And), node("A", GateType::Leaf), node("B", GateType::Leaf)],
            vec![edge("A"), edge("B")],
            &ExpectedCounts {
                nodes: 3,
                edges: 2,
                leaves: 2,
            },
        )
    }

    #[test]
    fn every_unknown_leaf_assumption_and_scope_is_written() -> Result<(), engine::EngineError> {
        let t = tree()?;
        let v = Verdict {
            outcome: Outcome::Reserved,
            scope: NodeId::new("R"),
            unsat_core: Vec::new(),
            unknown_leaves: vec![(NodeId::new("B"), FailureReason::QuoteNotFound)],
            assumed: vec![(NodeId::new("A"), true)],
            model_hint: Some(vec![(NodeId::new("B"), true)]),
            needed_facts: vec![(NodeId::new("B"), "a".into())],
            path: vec![vec![(NodeId::new("R"), "R-label".into(), GateType::And)]],
        };
        let said = HashMap::from([(NodeId::new("A"), "그렇다 치고".to_string())]);
        let text = render(&v, &t, &said, Some("문서 수정판 도착"));
        assert!(text.starts_with("판정 불가: 문서 수정판 도착\n결론: R R-label — 결론 유보"), "{text}");
        assert!(text.contains("이 판정은 R 서브트리에 한정되며"), "{text}");
        assert!(text.contains("리프 2개 중 1개"), "{text}");
        assert!(text.contains("- B B-label: 인용문이 원문에 없음 — 판정 불가"), "{text}");
        assert!(text.contains("- A A-label = 참 (사용자가 말한 원문: 「그렇다 치고」)"), "{text}");
        assert!(text.contains("성립에 필요한 사실 조합"), "{text}");
        assert!(!text.contains("\n\n"), "{text}");
        Ok(())
    }
}
