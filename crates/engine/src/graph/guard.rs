use petgraph::algo::is_cyclic_directed;
use petgraph::visit::EdgeFiltered;

use super::ClaimGraph;
use crate::error::{CountKind, GraphIssue};
use crate::schema::{ExpectedCounts, GateType, Relation};

pub(super) fn structure(graph: &ClaimGraph) -> Vec<GraphIssue> {
    let mut issues = cycles(graph);
    for node in graph.nodes() {
        let elements = graph.children(&node.id, Relation::Element).len();
        match node.gate {
            GateType::Leaf => {
                if is_blank(node.proposition.as_deref()) {
                    issues.push(GraphIssue::LeafWithoutProposition(node.id.clone()));
                }
                if is_blank(node.ask.as_deref()) {
                    issues.push(GraphIssue::LeafWithoutAsk(node.id.clone()));
                }
                if elements > 0 {
                    issues.push(GraphIssue::LeafWithElements(node.id.clone()));
                }
            }
            GateType::And | GateType::Or => {
                if elements == 0 {
                    issues.push(GraphIssue::GateWithoutElements(node.id.clone()));
                }
            }
        }
        if let Some(target) = &node.negation_of
            && !graph.contains(target)
        {
            issues.push(GraphIssue::NegationTargetMissing(node.id.clone()));
        }
    }
    issues
}

pub(super) fn counts(graph: &ClaimGraph, expected: &ExpectedCounts) -> Vec<GraphIssue> {
    let actual = graph.counts();
    [
        (CountKind::Nodes, expected.nodes, actual.nodes),
        (CountKind::Edges, expected.edges, actual.edges),
        (CountKind::Leaves, expected.leaves, actual.leaves),
    ]
    .into_iter()
    .filter(|(_, e, a)| e != a)
    .map(|(kind, expected, actual)| GraphIssue::CountMismatch {
        kind,
        expected,
        actual,
    })
    .collect()
}

fn cycles(graph: &ClaimGraph) -> Vec<GraphIssue> {
    let g = graph.inner();
    let mut issues: Vec<GraphIssue> = Relation::ALL
        .into_iter()
        .filter(|r| is_cyclic_directed(&EdgeFiltered::from_fn(g, |e| e.weight() == r)))
        .map(GraphIssue::LayerCycle)
        .collect();
    if issues.is_empty() && is_cyclic_directed(g) {
        issues.push(GraphIssue::MixedCycle);
    }
    issues
}

fn is_blank(text: Option<&str>) -> bool {
    text.is_none_or(|t| t.trim().is_empty())
}
