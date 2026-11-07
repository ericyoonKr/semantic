#[path = "fake/tree.rs"]
mod tree;

use engine::{ClaimGraph, CountKind, EngineError, ExpectedCounts, GateType, GraphIssue, Relation};
use tree::{COUNTS, TestResult, edge, graph, id, node, rows};

fn issues(result: Result<ClaimGraph, EngineError>) -> Vec<GraphIssue> {
    match result {
        Err(EngineError::Graph(found)) => found,
        _ => Vec::new(),
    }
}

#[test]
fn valid_tree_loads_with_exact_counts() -> TestResult {
    let g = graph()?;
    assert_eq!(g.counts(), COUNTS);
    assert_eq!(g.roots(), vec![id("ROOT")]);
    Ok(())
}

#[test]
fn cycle_inside_one_layer_is_rejected() {
    let (mut nodes, mut edges) = rows();
    nodes.push(node("C1", GateType::And));
    edges.push(edge("G", "C1", Relation::Element));
    edges.push(edge("C1", "G", Relation::Element));
    let counts = ExpectedCounts { nodes: 11, edges: 11, leaves: 7 };
    let found = issues(ClaimGraph::from_rows(nodes, edges, &counts));
    assert!(found.contains(&GraphIssue::LayerCycle(Relation::Element)));
}

#[test]
fn cycle_across_layers_is_rejected() {
    let (nodes, mut edges) = rows();
    edges.push(edge("L5", "ROOT", Relation::Defense));
    let counts = ExpectedCounts { nodes: 10, edges: 10, leaves: 7 };
    let found = issues(ClaimGraph::from_rows(nodes, edges, &counts));
    assert_eq!(found, vec![GraphIssue::MixedCycle]);
}

#[test]
fn expected_count_mismatch_is_rejected() {
    let (nodes, edges) = rows();
    let counts = ExpectedCounts { nodes: 11, edges: 9, leaves: 6 };
    let found = issues(ClaimGraph::from_rows(nodes, edges, &counts));
    assert_eq!(
        found,
        vec![
            GraphIssue::CountMismatch { kind: CountKind::Nodes, expected: 11, actual: 10 },
            GraphIssue::CountMismatch { kind: CountKind::Leaves, expected: 6, actual: 7 },
        ]
    );
}

#[test]
fn leaf_without_proposition_or_ask_is_rejected() {
    let (mut nodes, edges) = rows();
    for n in nodes.iter_mut() {
        if n.id == id("L1") {
            n.ask = Some("  ".into());
        }
        if n.id == id("L2") {
            n.proposition = None;
        }
    }
    let found = issues(ClaimGraph::from_rows(nodes, edges, &COUNTS));
    assert_eq!(
        found,
        vec![
            GraphIssue::LeafWithoutAsk(id("L1")),
            GraphIssue::LeafWithoutProposition(id("L2")),
        ]
    );
}

#[test]
fn gate_without_elements_and_leaf_with_elements_are_rejected() {
    let (mut nodes, mut edges) = rows();
    nodes.push(node("E1", GateType::Or));
    edges.push(edge("ROOT", "E1", Relation::Element));
    edges.push(edge("L1", "L2", Relation::Element));
    let counts = ExpectedCounts { nodes: 11, edges: 11, leaves: 7 };
    let found = issues(ClaimGraph::from_rows(nodes, edges, &counts));
    assert!(found.contains(&GraphIssue::GateWithoutElements(id("E1"))));
    assert!(found.contains(&GraphIssue::LeafWithElements(id("L1"))));
}

#[test]
fn unknown_endpoint_and_duplicate_node_are_rejected() {
    let (mut nodes, mut edges) = rows();
    nodes.push(node("L1", GateType::Leaf));
    edges.push(edge("ROOT", "X9", Relation::Element));
    let found = issues(ClaimGraph::from_rows(nodes, edges, &COUNTS));
    assert!(found.contains(&GraphIssue::DuplicateNode(id("L1"))));
    assert!(found.contains(&GraphIssue::UnknownEndpoint { parent: id("ROOT"), child: id("X9") }));
}

#[test]
fn shared_leaf_has_two_paths() -> TestResult {
    let (nodes, mut edges) = rows();
    edges.push(edge("D1", "L1", Relation::Element));
    let counts = ExpectedCounts { nodes: 10, edges: 10, leaves: 7 };
    let g = ClaimGraph::from_rows(nodes, edges, &counts)?;
    let paths = g.path_to_roots(&id("L1"))?;
    assert_eq!(
        paths,
        vec![vec![id("L1"), id("ROOT")], vec![id("L1"), id("D1"), id("ROOT")]]
    );
    Ok(())
}
