use std::fmt::Write;

use engine::{Burden, ClaimGraph, NodeId, Outcome, Trit, Verdict};

use crate::verdict_text::{named, outcome_word, reason};

pub fn burden_word(b: Burden) -> &'static str {
    match b {
        Burden::Plaintiff => "원고가 입증할 사실",
        Burden::Defendant => "피고가 입증할 항변",
        Burden::Pending => "입증책임 미정",
    }
}

pub fn burden_of(tree: &ClaimGraph, id: &NodeId) -> Burden {
    tree.node(id).map(|n| n.burden).unwrap_or(Burden::Pending)
}

pub fn conclusion(out: &mut String, v: &Verdict, tree: &ClaimGraph) {
    let Some(b) = &v.burden else {
        return;
    };
    let _ = writeln!(
        out,
        "입증책임을 적용한 결론: {} — {}",
        named(tree, &v.scope),
        outcome_word(b.outcome)
    );
}

pub fn details(out: &mut String, v: &Verdict, tree: &ClaimGraph) {
    let Some(b) = &v.burden else {
        return;
    };
    if !b.fixed.is_empty() {
        out.push_str("입증책임을 적용하며 입증되지 않아 거짓으로 본 리프:\n");
        for (id, burden) in &b.fixed {
            let _ = writeln!(out, "- {} ({})", named(tree, id), burden_word(*burden));
        }
    }
    if b.outcome == Outcome::NotEstablished && !b.unsat_core.is_empty() {
        out.push_str("입증책임을 적용했을 때 불성립 이유:\n");
        for (id, _) in &b.unsat_core {
            let _ = writeln!(out, "- {}", named(tree, id));
        }
    }
}

pub fn reductions(out: &mut String, v: &Verdict, tree: &ClaimGraph) {
    let shown: Vec<_> = v
        .reductions
        .iter()
        .filter(|r| r.value != Trit::False)
        .collect();
    if shown.is_empty() {
        return;
    }
    out.push_str("배상액을 줄이는 사유 (성립 여부 판단에는 넣지 않음):\n");
    for r in shown {
        let state = match (&r.value, &r.failure) {
            (Trit::True, _) => "참".to_string(),
            (_, Some(f)) => format!("{} — 판정 불가", reason(f)),
            (_, None) => "판정 불가".to_string(),
        };
        let _ = writeln!(
            out,
            "- {}: {state} — 배상액이 줄어들 수 있음",
            named(tree, &r.node_id)
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use engine::{
        BurdenVerdict, ClaimEdge, ClaimNode, ClaimType, ExpectedCounts, FailureReason, GateType,
        ReductionNote, Relation,
    };

    use super::*;
    use crate::verdict_text::render;

    fn node(id: &str, gate: GateType, burden: Burden) -> ClaimNode {
        let leaf = gate == GateType::Leaf;
        ClaimNode {
            id: NodeId::new(id),
            claim_type: ClaimType("T".into()),
            label: format!("{id}-label"),
            gate,
            burden,
            proposition: leaf.then(|| "p".to_string()),
            ask: leaf.then(|| "a".to_string()),
            negation_of: None,
            arithmetic: None,
            period_rule: None,
            evidence: Vec::new(),
        }
    }

    fn tree() -> Result<ClaimGraph, engine::EngineError> {
        let edge = |c: &str, relation: Relation| ClaimEdge {
            parent: NodeId::new("R"),
            child: NodeId::new(c),
            relation,
        };
        ClaimGraph::from_rows(
            vec![
                node("R", GateType::And, Burden::Plaintiff),
                node("A", GateType::Leaf, Burden::Plaintiff),
                node("D", GateType::Leaf, Burden::Defendant),
                node("C", GateType::Leaf, Burden::Defendant),
            ],
            vec![
                edge("A", Relation::Element),
                edge("D", Relation::Defense),
                edge("C", Relation::Reduction),
            ],
            &ExpectedCounts {
                nodes: 4,
                edges: 3,
                leaves: 3,
            },
        )
    }

    #[test]
    fn both_conclusions_burdens_and_reduction_are_written() -> Result<(), engine::EngineError> {
        let t = tree()?;
        let open = FailureReason::QuoteNotFound;
        let v = Verdict {
            outcome: Outcome::Reserved,
            scope: NodeId::new("R"),
            unsat_core: Vec::new(),
            unknown_leaves: vec![(NodeId::new("A"), open.clone()), (NodeId::new("D"), open.clone())],
            assumed: Vec::new(),
            model_hint: None,
            needed_facts: Vec::new(),
            path: Vec::new(),
            periods: Vec::new(),
            burden: Some(BurdenVerdict {
                outcome: Outcome::NotEstablished,
                unsat_core: vec![(NodeId::new("A"), "A-label".into())],
                fixed: vec![(NodeId::new("A"), Burden::Plaintiff), (NodeId::new("D"), Burden::Defendant)],
            }),
            reductions: vec![ReductionNote {
                node_id: NodeId::new("C"),
                value: Trit::Unknown,
                failure: Some(open),
            }],
        };
        let text = render(&v, &t, &HashMap::new(), None);
        assert!(
            text.starts_with("결론: R R-label — 결론 유보\n입증책임을 적용한 결론: R R-label — 불성립\n"),
            "{text}"
        );
        assert!(text.contains("- A A-label: 인용문이 원문에 없음 — 판정 불가 (원고가 입증할 사실)"), "{text}");
        assert!(text.contains("- D D-label: 인용문이 원문에 없음 — 판정 불가 (피고가 입증할 항변)"), "{text}");
        assert!(text.contains("입증되지 않아 거짓으로 본 리프:\n- A A-label (원고가 입증할 사실)\n- D D-label (피고가 입증할 항변)"), "{text}");
        assert!(text.contains("입증책임을 적용했을 때 불성립 이유:\n- A A-label"), "{text}");
        assert!(text.contains("- C C-label: 인용문이 원문에 없음 — 판정 불가 — 배상액이 줄어들 수 있음"), "{text}");
        let mut judged_false = v.clone();
        judged_false.reductions[0].value = Trit::False;
        judged_false.reductions[0].failure = None;
        let text = render(&judged_false, &t, &HashMap::new(), None);
        assert!(!text.contains("배상액"), "{text}");
        Ok(())
    }
}
