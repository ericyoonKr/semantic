use std::collections::HashSet;

use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;

use super::ClaimGraph;
use crate::error::{EngineError, GraphIssue};
use crate::schema::{ClaimEdge, NodeId};

impl ClaimGraph {
    pub fn subtree(&self, root: &NodeId) -> Result<ClaimGraph, EngineError> {
        let start = self
            .index(root)
            .ok_or_else(|| EngineError::UnknownNode(root.clone()))?;
        let g = self.inner();
        let mut keep: HashSet<NodeIndex> = HashSet::from([start]);
        let mut stack = vec![start];
        while let Some(at) = stack.pop() {
            for next in g.neighbors_directed(at, Direction::Outgoing) {
                if keep.insert(next) {
                    stack.push(next);
                }
            }
        }
        let nodes = g
            .node_indices()
            .filter(|i| keep.contains(i))
            .map(|i| g[i].clone())
            .collect();
        let edges = g
            .edge_references()
            .filter(|e| keep.contains(&e.source()) && keep.contains(&e.target()))
            .map(|e| ClaimEdge {
                parent: g[e.source()].id.clone(),
                child: g[e.target()].id.clone(),
                relation: *e.weight(),
            })
            .collect();
        ClaimGraph::from_valid_rows(nodes, edges)
    }

    pub fn path_to_roots(&self, node: &NodeId) -> Result<Vec<Vec<NodeId>>, EngineError> {
        if !self.contains(node) {
            return Err(EngineError::UnknownNode(node.clone()));
        }
        let mut paths = Vec::new();
        let mut stack = vec![vec![node.clone()]];
        while let Some(path) = stack.pop() {
            let Some(last) = path.last() else {
                continue;
            };
            let parents = self.parents(last);
            if parents.is_empty() {
                paths.push(path);
                continue;
            }
            for (_, parent) in parents.into_iter().rev() {
                if path.contains(&parent.id) {
                    return Err(EngineError::Graph(vec![GraphIssue::MixedCycle]));
                }
                let mut longer = path.clone();
                longer.push(parent.id.clone());
                stack.push(longer);
            }
        }
        Ok(paths)
    }

    pub fn topo_order(&self) -> Result<Vec<NodeId>, EngineError> {
        let g = self.inner();
        toposort(g, None)
            .map(|order| order.into_iter().map(|i| g[i].id.clone()).collect())
            .map_err(|_| EngineError::Graph(vec![GraphIssue::MixedCycle]))
    }
}
