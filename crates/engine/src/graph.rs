mod guard;
mod walk;

use std::collections::HashMap;

use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;

use crate::error::{EngineError, GraphIssue};
use crate::schema::{ClaimEdge, ClaimNode, ExpectedCounts, GateType, NodeId, Relation};

#[derive(Debug, Clone)]
pub struct ClaimGraph {
    g: DiGraph<ClaimNode, Relation>,
    by_id: HashMap<NodeId, NodeIndex>,
}

impl ClaimGraph {
    pub fn from_rows(
        nodes: Vec<ClaimNode>,
        edges: Vec<ClaimEdge>,
        expected: &ExpectedCounts,
    ) -> Result<Self, EngineError> {
        let (graph, mut issues) = Self::assemble(nodes, edges);
        issues.extend(guard::structure(&graph));
        issues.extend(guard::counts(&graph, expected));
        if issues.is_empty() {
            Ok(graph)
        } else {
            Err(EngineError::Graph(issues))
        }
    }

    pub(crate) fn from_valid_rows(
        nodes: Vec<ClaimNode>,
        edges: Vec<ClaimEdge>,
    ) -> Result<Self, EngineError> {
        let (graph, mut issues) = Self::assemble(nodes, edges);
        issues.extend(guard::structure(&graph));
        if issues.is_empty() {
            Ok(graph)
        } else {
            Err(EngineError::Graph(issues))
        }
    }

    fn assemble(nodes: Vec<ClaimNode>, edges: Vec<ClaimEdge>) -> (Self, Vec<GraphIssue>) {
        let mut issues = Vec::new();
        let mut g = DiGraph::new();
        let mut by_id = HashMap::new();
        for node in nodes {
            if by_id.contains_key(&node.id) {
                issues.push(GraphIssue::DuplicateNode(node.id));
                continue;
            }
            let id = node.id.clone();
            by_id.insert(id, g.add_node(node));
        }
        for edge in edges {
            let (Some(&p), Some(&c)) = (by_id.get(&edge.parent), by_id.get(&edge.child)) else {
                issues.push(GraphIssue::UnknownEndpoint {
                    parent: edge.parent,
                    child: edge.child,
                });
                continue;
            };
            if g.find_edge(p, c).is_some() {
                issues.push(GraphIssue::DuplicateEdge {
                    parent: edge.parent,
                    child: edge.child,
                });
                continue;
            }
            g.add_edge(p, c, edge.relation);
        }
        (Self { g, by_id }, issues)
    }

    pub fn node(&self, id: &NodeId) -> Option<&ClaimNode> {
        self.by_id.get(id).and_then(|i| self.g.node_weight(*i))
    }

    pub fn contains(&self, id: &NodeId) -> bool {
        self.by_id.contains_key(id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &ClaimNode> {
        self.g.node_indices().filter_map(|i| self.g.node_weight(i))
    }

    pub fn leaves(&self) -> impl Iterator<Item = &ClaimNode> {
        self.nodes().filter(|n| n.gate == GateType::Leaf)
    }

    pub fn edges(&self) -> Vec<ClaimEdge> {
        self.g
            .edge_references()
            .map(|e| ClaimEdge {
                parent: self.g[e.source()].id.clone(),
                child: self.g[e.target()].id.clone(),
                relation: *e.weight(),
            })
            .collect()
    }

    pub fn counts(&self) -> ExpectedCounts {
        ExpectedCounts {
            nodes: self.g.node_count(),
            edges: self.g.edge_count(),
            leaves: self.leaves().count(),
        }
    }

    pub fn children(&self, id: &NodeId, relation: Relation) -> Vec<&ClaimNode> {
        self.neighbors(id, Direction::Outgoing)
            .into_iter()
            .filter(|(r, _)| *r == relation)
            .map(|(_, n)| n)
            .collect()
    }

    pub fn parents(&self, id: &NodeId) -> Vec<(Relation, &ClaimNode)> {
        self.neighbors(id, Direction::Incoming)
    }

    pub fn is_reduction(&self, id: &NodeId) -> bool {
        let parents = self.parents(id);
        !parents.is_empty() && parents.iter().all(|(r, _)| *r == Relation::Reduction)
    }

    pub fn roots(&self) -> Vec<NodeId> {
        self.g
            .node_indices()
            .filter(|i| {
                self.g
                    .neighbors_directed(*i, Direction::Incoming)
                    .next()
                    .is_none()
            })
            .map(|i| self.g[i].id.clone())
            .collect()
    }

    fn neighbors(&self, id: &NodeId, dir: Direction) -> Vec<(Relation, &ClaimNode)> {
        let Some(&idx) = self.by_id.get(id) else {
            return Vec::new();
        };
        let mut found: Vec<_> = self
            .g
            .edges_directed(idx, dir)
            .map(|e| {
                let other = if dir == Direction::Outgoing {
                    e.target()
                } else {
                    e.source()
                };
                (e.id(), *e.weight(), other)
            })
            .collect();
        found.sort_by_key(|(edge, _, _)| *edge);
        found
            .into_iter()
            .map(|(_, r, other)| (r, &self.g[other]))
            .collect()
    }

    pub(crate) fn inner(&self) -> &DiGraph<ClaimNode, Relation> {
        &self.g
    }

    pub(crate) fn index(&self, id: &NodeId) -> Option<NodeIndex> {
        self.by_id.get(id).copied()
    }
}
