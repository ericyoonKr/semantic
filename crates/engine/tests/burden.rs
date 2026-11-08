#[path = "fake/tree.rs"]
mod tree;

use std::collections::HashMap;

use engine::{
    Burden, ClaimGraph, EngineError, ExpectedCounts, FailureReason, GateType, GraphIssue, LeafResult,
    NodeId, Outcome, Relation, Trit,
};
use tree::{COUNTS, TestResult, edge, facts, id, ids_of, judge_both, judged, node, rows, unknown};

fn burdened() -> Result<ClaimGraph, EngineError> {
    let (mut nodes, edges) = rows();
    for n in &mut nodes {
        n.burden = match n.id.as_str() {
            "D1" | "L5" | "L6" => Burden::Defendant,
            _ => Burden::Plaintiff,
        };
    }
    ClaimGraph::from_rows(nodes, edges, &COUNTS)
}

fn base(overrides: &[(&str, LeafResult)]) -> HashMap<NodeId, LeafResult> {
    let mut all = facts(&[
        ("L1", judged(true)),
        ("L2", judged(true)),
        ("L3", judged(true)),
        ("L4", judged(true)),
        ("L5", judged(false)),
        ("L6", judged(false)),
        ("R1", judged(false)),
    ]);
    all.extend(facts(overrides));
    all
}

#[test]
fn unproven_plaintiff_leaf_reserves_facts_but_fails_under_burden() -> TestResult {
    let g = burdened()?;
    let v = judge_both(&g, "ROOT", &base(&[("L1", unknown())]), &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Reserved);
    let b = v.burden.ok_or("burden verdict missing")?;
    assert_eq!(b.outcome, Outcome::NotEstablished);
    assert_eq!(b.fixed, vec![(id("L1"), Burden::Plaintiff)]);
    assert!(ids_of(&b.unsat_core).contains(&"L1"));
    Ok(())
}

#[test]
fn unproven_defense_leaf_lets_the_claim_stand_under_burden() -> TestResult {
    let g = burdened()?;
    let leaves = base(&[("L5", unknown()), ("L6", judged(true))]);
    let v = judge_both(&g, "ROOT", &leaves, &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Reserved);
    let b = v.burden.ok_or("burden verdict missing")?;
    assert_eq!(b.outcome, Outcome::Established);
    assert_eq!(b.fixed, vec![(id("L5"), Burden::Defendant)]);
    Ok(())
}

#[test]
fn judged_assumed_pending_and_negated_leaves_are_not_fixed() -> TestResult {
    let g = burdened()?;
    let leaves = base(&[("L1", unknown()), ("L2", unknown())]);
    let assume = HashMap::from([(id("L1"), true)]);
    let v = judge_both(&g, "ROOT", &leaves, &assume)?;
    let b = v.burden.ok_or("burden verdict missing")?;
    assert_eq!(b.fixed, vec![(id("L2"), Burden::Plaintiff)]);
    assert_eq!(b.outcome, Outcome::NotEstablished);

    let pending = tree::graph()?;
    let v = judge_both(&pending, "ROOT", &base(&[("L1", unknown())]), &HashMap::new())?;
    let b = v.burden.ok_or("burden verdict missing")?;
    assert!(b.fixed.is_empty());
    assert_eq!(b.outcome, Outcome::Reserved);

    let mut n2 = node("N2", GateType::Leaf);
    n2.negation_of = Some(id("N1"));
    n2.burden = Burden::Defendant;
    let mut n1 = node("N1", GateType::Leaf);
    n1.burden = Burden::Plaintiff;
    let mut top = node("TOP", GateType::And);
    top.burden = Burden::Plaintiff;
    let pair = ClaimGraph::from_rows(
        vec![top, n1, n2],
        vec![edge("TOP", "N1", Relation::Element), edge("TOP", "N2", Relation::Defense)],
        &ExpectedCounts { nodes: 3, edges: 2, leaves: 2 },
    )?;
    let both_open = facts(&[("N1", unknown()), ("N2", unknown())]);
    let v = judge_both(&pair, "TOP", &both_open, &HashMap::new())?;
    let b = v.burden.ok_or("burden verdict missing")?;
    assert_eq!(b.fixed, vec![(id("N1"), Burden::Plaintiff)]);
    assert_eq!(b.outcome, Outcome::NotEstablished);
    let partner_known = facts(&[("N1", unknown()), ("N2", judged(false))]);
    let v = judge_both(&pair, "TOP", &partner_known, &HashMap::new())?;
    let b = v.burden.ok_or("burden verdict missing")?;
    assert!(b.fixed.is_empty());
    assert_eq!((v.outcome, b.outcome), (Outcome::Established, Outcome::Established));
    Ok(())
}

fn with_reduction() -> Result<ClaimGraph, EngineError> {
    ClaimGraph::from_rows(
        vec![
            node("ROOT", GateType::And),
            node("A", GateType::Leaf),
            node("B", GateType::Leaf),
            node("C", GateType::Leaf),
        ],
        vec![
            edge("ROOT", "A", Relation::Element),
            edge("ROOT", "B", Relation::Element),
            edge("ROOT", "C", Relation::Reduction),
        ],
        &ExpectedCounts { nodes: 4, edges: 3, leaves: 3 },
    )
}

#[test]
fn reduction_leaf_is_reported_but_left_out_of_the_conclusion() -> TestResult {
    let g = with_reduction()?;
    for (c, outcome) in [(judged(true), Trit::True), (unknown(), Trit::Unknown), (judged(false), Trit::False)] {
        let leaves = facts(&[("A", judged(true)), ("B", judged(true)), ("C", c.clone())]);
        let v = judge_both(&g, "ROOT", &leaves, &HashMap::new())?;
        assert_eq!(v.outcome, Outcome::Established);
        assert!(v.unknown_leaves.is_empty());
        assert_eq!(v.model_hint, None);
        assert_eq!(ids_of(&v.needed_facts), if outcome == Trit::Unknown { vec!["C"] } else { vec![] });
        assert_eq!(v.reductions.len(), 1);
        assert_eq!((v.reductions[0].node_id.as_str(), v.reductions[0].value), ("C", outcome));
        let b = v.burden.ok_or("burden verdict missing")?;
        assert!(b.fixed.is_empty());
        assert_eq!(b.outcome, Outcome::Established);
    }
    let leaves = facts(&[("A", judged(false)), ("B", judged(true)), ("C", judged(true))]);
    let v = judge_both(&g, "ROOT", &leaves, &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::NotEstablished);
    assert_eq!(ids_of(&v.unsat_core), vec!["A"]);
    Ok(())
}

#[test]
fn reduction_edge_to_a_gate_or_shared_leaf_is_refused() {
    let counts = ExpectedCounts { nodes: 4, edges: 4, leaves: 2 };
    let gate = ClaimGraph::from_rows(
        vec![
            node("ROOT", GateType::And),
            node("A", GateType::Leaf),
            node("G", GateType::And),
            node("C", GateType::Leaf),
        ],
        vec![
            edge("ROOT", "A", Relation::Element),
            edge("ROOT", "G", Relation::Reduction),
            edge("G", "C", Relation::Element),
            edge("ROOT", "C", Relation::Reduction),
        ],
        &counts,
    );
    let found = match gate {
        Err(EngineError::Graph(found)) => found,
        _ => Vec::new(),
    };
    assert!(found.contains(&GraphIssue::ReductionMisplaced(id("G"))), "{found:?}");
    assert!(found.contains(&GraphIssue::ReductionMisplaced(id("C"))), "{found:?}");
}

#[test]
fn leaf_open_for_a_missing_rule_stays_open_under_burden() -> TestResult {
    let g = burdened()?;
    for why in [
        engine::arith::compute::HOLIDAYS_OPEN.to_string(),
        format!("{}; {}", engine::arith::compute::START_OPEN, engine::arith::compute::HOLIDAYS_OPEN),
        engine::arith::RESTRICTIVE_OPEN.to_string(),
    ] {
        let open = LeafResult::Unknown { failure: FailureReason::NoEvidence(why) };
        let v = judge_both(&g, "ROOT", &base(&[("L1", open)]), &HashMap::new())?;
        let b = v.burden.ok_or("burden verdict missing")?;
        assert_eq!(b.outcome, Outcome::Reserved);
        assert!(b.fixed.is_empty());
    }
    let facts_only = LeafResult::Unknown {
        failure: FailureReason::NoEvidence(engine::arith::compute::START_OPEN.to_string()),
    };
    let v = judge_both(&g, "ROOT", &base(&[("L1", facts_only)]), &HashMap::new())?;
    let b = v.burden.ok_or("burden verdict missing")?;
    assert_eq!(b.outcome, Outcome::NotEstablished);
    Ok(())
}
