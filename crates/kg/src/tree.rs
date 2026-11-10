use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

use crate::error::KgError;
use crate::query::{Direction, MAX_LIMIT};
use crate::set::KgSet;
use crate::traverse::ExpandQuery;

const ELEMENT: &str = "element";
const DEFENSE: &str = "defense";
const REBUTTAL: &str = "rebuttal";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Truth {
    Met,
    NotMet,
    Unknown,
    Claimed,
}

impl Truth {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "충족" => Some(Truth::Met),
            "불충족" => Some(Truth::NotMet),
            "모름" => Some(Truth::Unknown),
            "주장됨" => Some(Truth::Claimed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Truth::Met => "충족",
            Truth::NotMet => "불충족",
            Truth::Unknown => "모름",
            Truth::Claimed => "주장됨",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Gate {
    And,
    Or,
    Leaf,
}

impl Gate {
    fn of(data: &Value) -> Gate {
        match data.get("gate").and_then(Value::as_str) {
            Some("AND") => Gate::And,
            Some("OR") => Gate::Or,
            _ => Gate::Leaf,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeNode {
    pub id: String,
    pub name: String,
    pub gate: Gate,
    pub elements: Vec<String>,
    pub blockers: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tree {
    pub root: String,
    pub nodes: BTreeMap<String, TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeTruth {
    pub id: String,
    pub name: String,
    pub value: Truth,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TreeOutcome {
    pub root: String,
    pub value: Truth,
    pub nodes: Vec<NodeTruth>,
    pub open_leaves: Vec<String>,
}

pub fn fold(gate: Gate, children: &[Truth]) -> Truth {
    let any = |t: Truth| children.contains(&t);
    match gate {
        Gate::Leaf => Truth::Unknown,
        Gate::And if children.is_empty() => Truth::Unknown,
        Gate::And if any(Truth::NotMet) => Truth::NotMet,
        Gate::And if any(Truth::Unknown) => Truth::Unknown,
        Gate::And if any(Truth::Claimed) => Truth::Claimed,
        Gate::And => Truth::Met,
        Gate::Or if any(Truth::Met) => Truth::Met,
        Gate::Or if any(Truth::Claimed) => Truth::Claimed,
        Gate::Or if any(Truth::Unknown) || children.is_empty() => Truth::Unknown,
        Gate::Or => Truth::NotMet,
    }
}

pub fn judge(tree: &Tree, states: &BTreeMap<String, Truth>) -> TreeOutcome {
    let mut done: BTreeMap<String, (Truth, Vec<String>)> = BTreeMap::new();
    let value = settle(tree, &tree.root, states, &mut done, &mut BTreeSet::new());
    let nodes = done
        .iter()
        .filter_map(|(id, (v, blocked))| {
            tree.nodes.get(id).map(|n| NodeTruth {
                id: id.clone(),
                name: n.name.clone(),
                value: *v,
                blocked_by: blocked.clone(),
            })
        })
        .collect();
    let open_leaves = tree
        .nodes
        .values()
        .filter(|n| n.gate == Gate::Leaf)
        .filter(|n| matches!(states.get(&n.id), None | Some(Truth::Unknown | Truth::Claimed)))
        .map(|n| n.id.clone())
        .collect();
    TreeOutcome {
        root: tree.root.clone(),
        value,
        nodes,
        open_leaves,
    }
}

fn settle(
    tree: &Tree,
    id: &str,
    states: &BTreeMap<String, Truth>,
    done: &mut BTreeMap<String, (Truth, Vec<String>)>,
    path: &mut BTreeSet<String>,
) -> Truth {
    if let Some((v, _)) = done.get(id) {
        return *v;
    }
    let Some(node) = tree.nodes.get(id) else {
        return Truth::Unknown;
    };
    if !path.insert(id.to_string()) {
        return Truth::Unknown;
    }
    let base = match node.gate {
        Gate::Leaf => states.get(id).copied().unwrap_or(Truth::Unknown),
        gate => {
            let children: Vec<Truth> = node
                .elements
                .iter()
                .map(|c| settle(tree, c, states, done, path))
                .collect();
            fold(gate, &children)
        }
    };
    let blocked_by: Vec<String> = node
        .blockers
        .iter()
        .filter(|(c, _)| settle(tree, c, states, done, path) == Truth::Met)
        .map(|(c, _)| c.clone())
        .collect();
    path.remove(id);
    let value = if blocked_by.is_empty() { base } else { Truth::NotMet };
    done.insert(id.to_string(), (value, blocked_by));
    value
}

impl KgSet {
    pub fn tree(&self, graph: &str, root: &str, max_nodes: usize) -> Result<Tree, KgError> {
        let mut nodes = BTreeMap::new();
        let mut todo = vec![root.to_string()];
        while let Some(id) = todo.pop() {
            if nodes.contains_key(&id) {
                continue;
            }
            if nodes.len() >= max_nodes {
                return Err(KgError::Argument("max_nodes"));
            }
            let hit = self.node(graph, &id)?.ok_or(KgError::UnknownNode)?;
            let q = ExpandQuery {
                relation: None,
                direction: Direction::Out,
                depth: 1,
                limit: MAX_LIMIT,
                as_of: None,
            };
            let ex = self.expand(graph, &id, &q)?;
            let mut elements = Vec::new();
            let mut blockers = Vec::new();
            for e in ex.edges.iter().filter(|e| e.src == id) {
                match e.relation.as_str() {
                    ELEMENT => elements.push(e.dst.clone()),
                    DEFENSE | REBUTTAL => blockers.push((e.dst.clone(), e.relation.clone())),
                    _ => continue,
                }
                todo.push(e.dst.clone());
            }
            nodes.insert(
                id.clone(),
                TreeNode {
                    id: id.clone(),
                    name: hit.name,
                    gate: Gate::of(&hit.data),
                    elements,
                    blockers,
                },
            );
        }
        Ok(Tree {
            root: root.to_string(),
            nodes,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Gate, Tree, TreeNode, Truth, fold, judge};

    fn node(id: &str, gate: Gate, elements: &[&str], blockers: &[&str]) -> (String, TreeNode) {
        (
            id.to_string(),
            TreeNode {
                id: id.to_string(),
                name: id.to_string(),
                gate,
                elements: elements.iter().map(|s| s.to_string()).collect(),
                blockers: blockers.iter().map(|s| (s.to_string(), "defense".to_string())).collect(),
            },
        )
    }

    fn tree() -> Tree {
        Tree {
            root: "R".into(),
            nodes: [
                node("R", Gate::And, &["A", "O"], &["D"]),
                node("A", Gate::Leaf, &[], &[]),
                node("O", Gate::Or, &["B", "C"], &[]),
                node("B", Gate::Leaf, &[], &[]),
                node("C", Gate::Leaf, &[], &[]),
                node("D", Gate::Leaf, &[], &[]),
            ]
            .into_iter()
            .collect(),
        }
    }

    fn states(pairs: &[(&str, Truth)]) -> BTreeMap<String, Truth> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn fold_and_or_follow_three_valued_rules_with_claimed() {
        use Truth::*;
        assert_eq!(fold(Gate::And, &[Met, Met]), Met);
        assert_eq!(fold(Gate::And, &[Met, Unknown]), Unknown);
        assert_eq!(fold(Gate::And, &[Unknown, NotMet]), NotMet);
        assert_eq!(fold(Gate::And, &[Met, Claimed]), Claimed);
        assert_eq!(fold(Gate::Or, &[NotMet, Met]), Met);
        assert_eq!(fold(Gate::Or, &[NotMet, Unknown]), Unknown);
        assert_eq!(fold(Gate::Or, &[NotMet, NotMet]), NotMet);
        assert_eq!(fold(Gate::Or, &[Claimed, Unknown]), Claimed);
    }

    #[test]
    fn met_defense_blocks_root_and_unknown_defense_does_not() {
        let t = tree();
        let met = judge(&t, &states(&[("A", Truth::Met), ("B", Truth::Met), ("D", Truth::Met)]));
        assert_eq!(met.value, Truth::NotMet);
        let root = met.nodes.iter().find(|n| n.id == "R").map(|n| n.blocked_by.clone());
        assert_eq!(root, Some(vec!["D".to_string()]));
        let open = judge(&t, &states(&[("A", Truth::Met), ("B", Truth::Met)]));
        assert_eq!(open.value, Truth::Met);
        assert_eq!(open.open_leaves, vec!["C".to_string(), "D".to_string()]);
    }

    #[test]
    fn unknown_element_keeps_root_unknown_until_settled() {
        let t = tree();
        assert_eq!(judge(&t, &states(&[("A", Truth::Met)])).value, Truth::Unknown);
        assert_eq!(judge(&t, &states(&[("A", Truth::NotMet)])).value, Truth::NotMet);
    }
}
