use std::collections::{BTreeMap, BTreeSet};

use super::raw::{RawGraph, RawNode};
use super::{Edge, Graph, GraphCheck, Node, NodeKind, Relation, Source};
use crate::Error;

fn bad(msg: String) -> Error {
    Error::Graph(msg)
}

fn node(raw: RawNode) -> Result<Node, Error> {
    let kind = NodeKind::parse(&raw.kind)?;
    let id = raw.id;
    let has_rule_parts = raw.field.is_some() || raw.times.is_some() || raw.sources.is_some();
    let shape_ok = match kind {
        NodeKind::RefTime => raw.desc.is_none() && !has_rule_parts && raw.meta.is_none(),
        NodeKind::Field => raw.desc.is_some() && !has_rule_parts && raw.meta.is_none(),
        k if k.is_rule() => {
            raw.desc.is_none()
                && raw.meta.is_none()
                && raw.field.is_some()
                && raw.times.is_some()
                && raw.sources.as_ref().is_some_and(|s| !s.is_empty())
        }
        _ => raw.desc.is_none() && !has_rule_parts && raw.meta.is_some(),
    };
    if !shape_ok {
        return Err(bad(format!("{id}: {} 노드의 칸 구성이 맞지 않음", kind.name())));
    }
    let sources = raw
        .sources
        .unwrap_or_default()
        .into_iter()
        .map(|s| Source { node: s.source, quote: s.quote, ok: s.ok })
        .collect();
    Ok(Node {
        id,
        kind,
        label: raw.label,
        desc: raw.desc,
        field: raw.field,
        times: raw.times.unwrap_or_default(),
        sources,
        meta: raw.meta,
    })
}

fn check_rule(n: &Node, nodes: &BTreeMap<String, Node>, fields: &BTreeSet<&str>) -> Result<(), Error> {
    if let Some(f) = &n.field
        && !fields.contains(f.as_str()) {
            return Err(bad(format!("{}: 분야 {f}가 없음", n.id)));
        }
    for t in &n.times {
        if nodes.get(t).map(|x| x.kind) != Some(NodeKind::RefTime) {
            return Err(bad(format!("{}: 기준시점 {t}가 없음", n.id)));
        }
    }
    for s in &n.sources {
        if !nodes.get(&s.node).is_some_and(|x| x.kind.is_source()) {
            return Err(bad(format!("{}: 근거 노드 {}가 없음", n.id, s.node)));
        }
    }
    Ok(())
}

fn check_edges(edges: &[Edge], nodes: &BTreeMap<String, Node>) -> Result<(), Error> {
    for e in edges {
        if !nodes.contains_key(&e.from) || !nodes.contains_key(&e.to) {
            return Err(bad(format!("간선 끝 노드가 없음: {} → {}", e.from, e.to)));
        }
        if (e.rel == Relation::BASIS) != e.quote.is_some() {
            return Err(bad(format!("근거 인용 칸이 맞지 않음: {} → {}", e.from, e.to)));
        }
    }
    for n in nodes.values().filter(|n| n.kind.is_rule()) {
        let mut by_edge: Vec<&str> = edges
            .iter()
            .filter(|e| e.from == n.id && e.rel == Relation::REF_TIME)
            .map(|e| e.to.as_str())
            .collect();
        let mut by_node: Vec<&str> = n.times.iter().map(String::as_str).collect();
        by_edge.sort_unstable();
        by_node.sort_unstable();
        if by_edge != by_node {
            return Err(bad(format!("{}: times 칸과 기준 시점 간선이 다름", n.id)));
        }
    }
    Ok(())
}

fn check_counts(raw: &RawGraph, nodes: &BTreeMap<String, Node>, edges: usize, total: usize) -> Result<(), Error> {
    if raw.check.nodes != total || nodes.len() != total || raw.check.edges != edges {
        return Err(bad("검사 칸의 노드·간선 수가 실제와 다름".to_string()));
    }
    for (name, count) in &raw.check.by_kind {
        let kind = NodeKind::parse(name)?;
        if nodes.values().filter(|n| n.kind == kind).count() != *count {
            return Err(bad(format!("검사 칸의 {name} 수가 실제와 다름")));
        }
    }
    if raw.check.error_count != raw.errors.len() || raw.check.unsupported != 0 {
        return Err(bad("검사 칸의 오류 수가 실제와 다름".to_string()));
    }
    for q in &raw.quote_checks {
        if !nodes.get(&q.principle).is_some_and(|n| n.kind.is_rule()) || q.basis.is_empty() {
            return Err(bad(format!("인용검사 칸의 원칙이 없음: {}", q.principle)));
        }
    }
    Ok(())
}

pub fn build(mut raw: RawGraph) -> Result<Graph, Error> {
    let mut nodes = BTreeMap::new();
    let total = raw.nodes.len();
    for r in std::mem::take(&mut raw.nodes) {
        let n = node(r)?;
        if nodes.contains_key(&n.id) {
            return Err(bad(format!("노드 id가 겹침: {}", n.id)));
        }
        nodes.insert(n.id.clone(), n);
    }
    let fields: BTreeSet<&str> = nodes
        .values()
        .filter(|n| n.kind == NodeKind::Field)
        .map(|n| n.label.as_str())
        .collect();
    for n in nodes.values().filter(|n| n.kind.is_rule()) {
        check_rule(n, &nodes, &fields)?;
    }
    let edges = std::mem::take(&mut raw.edges)
        .into_iter()
        .map(|e| {
            Ok(Edge { rel: Relation::parse(&e.rel)?, from: e.from, to: e.to, quote: e.quote })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    check_edges(&edges, &nodes)?;
    check_counts(&raw, &nodes, edges.len(), total)?;
    let check = GraphCheck {
        quote_match: raw.check.quote_match,
        cycles: raw.check.cycles,
        quote_checks: raw.quote_checks.len(),
        quote_checks_found: raw.quote_checks.iter().filter(|q| q.found).count(),
    };
    Ok(Graph {
        nodes,
        edges,
        laws: raw.laws,
        check,
        base_date: raw.base_date,
        source_sha256: raw.source.sha256,
    })
}
