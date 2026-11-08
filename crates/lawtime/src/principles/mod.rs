mod kinds;
mod load;
mod raw;

use std::collections::BTreeMap;

use serde::Serialize;

pub use kinds::{NodeKind, Relation};
pub use raw::RawMeta;

use crate::Error;

pub const PATH_ENV: &str = "LAWTIME_PRINCIPLES";

#[derive(Debug, Clone)]
pub struct Source {
    pub node: String,
    pub quote: String,
    pub ok: bool,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub desc: Option<String>,
    pub field: Option<String>,
    pub times: Vec<String>,
    pub sources: Vec<Source>,
    pub meta: Option<RawMeta>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub rel: Relation,
    pub quote: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Quote {
    pub source: String,
    pub source_label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GraphRef {
    pub id: String,
    pub label: String,
    pub quotes: Vec<Quote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCheck {
    pub quote_match: String,
    pub cycles: usize,
    pub quote_checks: usize,
    pub quote_checks_found: usize,
}

#[derive(Debug, Clone)]
pub struct Graph {
    nodes: BTreeMap<String, Node>,
    edges: Vec<Edge>,
    laws: BTreeMap<String, RawMeta>,
    check: GraphCheck,
    base_date: String,
    source_sha256: String,
}

impl Graph {
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let raw: raw::RawGraph = serde_json::from_str(text)?;
        load::build(raw)
    }

    pub fn from_path(path: &std::path::Path) -> Result<Self, Error> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    pub fn from_env() -> Result<Self, Error> {
        let path = std::env::var_os(PATH_ENV).ok_or(Error::MissingEnv(PATH_ENV))?;
        Self::from_path(std::path::Path::new(&path))
    }

    pub fn base_date(&self) -> &str {
        &self.base_date
    }

    pub fn laws(&self) -> &BTreeMap<String, RawMeta> {
        &self.laws
    }

    pub fn check(&self) -> &GraphCheck {
        &self.check
    }

    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn rule(&self, id: &str) -> Result<&Node, Error> {
        match self.nodes.get(id) {
            Some(n) if n.kind.is_rule() => Ok(n),
            _ => Err(Error::Graph(format!("원칙·예외 노드가 없음: {id}"))),
        }
    }

    pub fn field_label(&self, field_id: &str) -> Result<&str, Error> {
        match self.nodes.get(field_id) {
            Some(n) if n.kind == NodeKind::Field => Ok(&n.label),
            _ => Err(Error::Graph(format!("분야 노드가 없음: {field_id}"))),
        }
    }

    pub fn targets(&self, from: &str, rel: Relation) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|e| e.from == from && e.rel == rel)
            .map(|e| e.to.as_str())
            .collect()
    }

    pub fn count(&self, kind: NodeKind) -> usize {
        self.nodes.values().filter(|n| n.kind == kind).count()
    }

    pub fn reference(&self, id: &str) -> Result<GraphRef, Error> {
        let node = self
            .nodes
            .get(id)
            .ok_or_else(|| Error::Graph(format!("노드가 없음: {id}")))?;
        let quotes = node
            .sources
            .iter()
            .filter(|s| s.ok)
            .map(|s| Quote {
                source: s.node.clone(),
                source_label: self
                    .nodes
                    .get(&s.node)
                    .map_or_else(String::new, |n| n.label.clone()),
                text: s.quote.clone(),
            })
            .collect();
        Ok(GraphRef { id: node.id.clone(), label: node.label.clone(), quotes })
    }
}
