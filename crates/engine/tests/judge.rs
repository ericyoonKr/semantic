#[path = "fake/tree.rs"]
mod tree;

use std::collections::HashMap;

use engine::{ClaimGraph, EngineError, ExpectedCounts, GateType, Outcome, Relation};
use tree::{TestResult, edge, facts, graph, id, ids_of, judge, judged, node, unknown};

fn base(overrides: &[(&str, engine::LeafResult)]) -> HashMap<engine::NodeId, engine::LeafResult> {
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
fn all_leaves_true_is_established() -> TestResult {
    let g = graph()?;
    let leaves = facts(&[
        ("L1", judged(true)),
        ("L2", judged(true)),
        ("L3", judged(true)),
        ("L4", judged(true)),
        ("L5", judged(true)),
        ("L6", judged(true)),
        ("R1", judged(true)),
    ]);
    let v = judge(&g, "ROOT", &leaves, &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Established);
    assert!(v.unsat_core.is_empty());
    assert!(v.unknown_leaves.is_empty());
    assert!(v.needed_facts.is_empty());
    assert_eq!(v.scope, id("ROOT"));
    Ok(())
}

#[test]
fn one_and_leaf_false_is_not_established_with_core() -> TestResult {
    let g = graph()?;
    let v = judge(&g, "ROOT", &base(&[("L2", judged(false))]), &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::NotEstablished);
    assert!(ids_of(&v.unsat_core).contains(&"L2"));
    assert_eq!(
        v.unsat_core.iter().find(|(n, _)| n.as_str() == "L2").map(|(_, l)| l.as_str()),
        Some("label-L2")
    );
    Ok(())
}

#[test]
fn both_or_leaves_false_is_not_established() -> TestResult {
    let g = graph()?;
    let leaves = base(&[("L3", judged(false)), ("L4", judged(false))]);
    let v = judge(&g, "ROOT", &leaves, &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::NotEstablished);
    let core = ids_of(&v.unsat_core);
    assert!(core.contains(&"L3") && core.contains(&"L4"));
    Ok(())
}

#[test]
fn unknown_leaf_that_swings_root_is_reserved_with_needed_fact() -> TestResult {
    let g = graph()?;
    let v = judge(&g, "ROOT", &base(&[("L1", unknown())]), &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Reserved);
    assert_eq!(ids_of(&v.unknown_leaves), vec!["L1"]);
    assert_eq!(v.needed_facts, vec![(id("L1"), "ask-L1".to_string())]);
    assert_eq!(v.model_hint, Some(vec![(id("L1"), true)]));
    assert!(v.unsat_core.is_empty());
    Ok(())
}

#[test]
fn unknown_leaf_that_cannot_swing_root_stays_established() -> TestResult {
    let g = graph()?;
    let v = judge(&g, "ROOT", &base(&[("L3", unknown())]), &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Established);
    assert!(v.needed_facts.is_empty());
    assert_eq!(v.model_hint, None);
    Ok(())
}

#[test]
fn reserved_asks_only_leaves_that_can_change_the_outcome() -> TestResult {
    let g = graph()?;
    let v = judge(&g, "ROOT", &base(&[("L1", unknown()), ("L3", unknown())]), &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Reserved);
    assert_eq!(ids_of(&v.unknown_leaves), vec!["L1", "L3"]);
    assert_eq!(v.needed_facts, vec![(id("L1"), "ask-L1".to_string())]);
    Ok(())
}

#[test]
fn defense_true_is_not_established() -> TestResult {
    let g = graph()?;
    let leaves = base(&[("L5", judged(true)), ("L6", judged(true))]);
    let v = judge(&g, "ROOT", &leaves, &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::NotEstablished);
    let core = ids_of(&v.unsat_core);
    assert!(core.contains(&"L5") && core.contains(&"L6") && core.contains(&"R1"));
    Ok(())
}

#[test]
fn rebuttal_true_restores_establishment() -> TestResult {
    let g = graph()?;
    let leaves = base(&[
        ("L5", judged(true)),
        ("L6", judged(true)),
        ("R1", judged(true)),
    ]);
    let v = judge(&g, "ROOT", &leaves, &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Established);
    Ok(())
}

#[test]
fn assumption_on_unknown_leaf_is_marked_assumed() -> TestResult {
    let g = graph()?;
    let leaves = base(&[("L1", unknown())]);
    let assume = HashMap::from([(id("L1"), true)]);
    let v = judge(&g, "ROOT", &leaves, &assume)?;
    assert_eq!(v.outcome, Outcome::Established);
    assert_eq!(v.assumed, vec![(id("L1"), true)]);
    assert_eq!(ids_of(&v.unknown_leaves), vec!["L1"]);
    let v = judge(&g, "ROOT", &leaves, &HashMap::from([(id("L1"), false)]))?;
    assert_eq!(v.outcome, Outcome::NotEstablished);
    assert!(ids_of(&v.unsat_core).contains(&"L1"));
    Ok(())
}

#[test]
fn assumption_on_inner_node_fixes_unknown_children() -> TestResult {
    let g = graph()?;
    let leaves = base(&[("L3", unknown()), ("L4", unknown())]);
    let v = judge(&g, "ROOT", &leaves, &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Reserved);
    let v = judge(&g, "ROOT", &leaves, &HashMap::from([(id("G"), true)]))?;
    assert_eq!(v.outcome, Outcome::Established);
    assert_eq!(v.assumed, vec![(id("G"), true)]);
    Ok(())
}

#[test]
fn subtree_scope_and_path_to_root() -> TestResult {
    let g = graph()?;
    let leaves = base(&[("L5", judged(true)), ("L6", judged(true))]);
    let v = judge(&g, "D1", &leaves, &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Established);
    assert_eq!(v.scope, id("D1"));
    assert_eq!(
        v.path,
        vec![vec![
            (id("D1"), "label-D1".to_string(), GateType::And),
            (id("ROOT"), "label-ROOT".to_string(), GateType::And),
        ]]
    );
    let sub = g.subtree(&id("G"))?;
    assert_eq!(sub.counts(), ExpectedCounts { nodes: 3, edges: 2, leaves: 2 });
    Ok(())
}

#[test]
fn negation_pair_both_true_is_reserved_with_core() -> TestResult {
    let mut second = node("N2", GateType::Leaf);
    second.negation_of = Some(id("N1"));
    let nodes = vec![node("TOP", GateType::And), node("N1", GateType::Leaf), second];
    let edges = vec![
        edge("TOP", "N1", Relation::Element),
        edge("TOP", "N2", Relation::Element),
    ];
    let g = ClaimGraph::from_rows(nodes, edges, &ExpectedCounts { nodes: 3, edges: 2, leaves: 2 })?;
    let leaves = facts(&[("N1", judged(true)), ("N2", judged(true))]);
    let v = judge(&g, "TOP", &leaves, &HashMap::new())?;
    assert_eq!(v.outcome, Outcome::Reserved);
    let core = ids_of(&v.unsat_core);
    assert!(core.contains(&"N1") && core.contains(&"N2"));
    Ok(())
}

#[test]
fn missing_leaf_result_is_an_error() -> TestResult {
    let g = graph()?;
    let mut leaves = base(&[]);
    leaves.remove(&id("L4"));
    let err = judge(&g, "ROOT", &leaves, &HashMap::new());
    assert_eq!(err, Err(EngineError::MissingLeafResult(id("L4"))));
    Ok(())
}
