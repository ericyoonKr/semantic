use std::collections::HashMap;

use kg::{Direction, Evidence, Expansion, NodeHit, ShortIds};
use serde::Serialize;
use serde_json::Value;

const GLOSS_FIELDS: [&str; 4] = ["gloss", "definition", "description", "summary"];
const GLOSS_CHARS: usize = 160;
const NO_EVIDENCE: &str = "근거 없음";
const HIDDEN_FIELDS: [&str; 3] = ["id", "source", "target"];

#[derive(Debug, Clone, Serialize)]
pub struct NodeView {
    pub node: String,
    pub name: String,
    pub kind: String,
    pub gloss: String,
    pub status: Option<Value>,
    pub content: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeView {
    pub from: String,
    pub to: String,
    pub src: String,
    pub dst: String,
    pub relation: String,
    pub depth: u8,
    pub certainty: Option<Value>,
    pub evidence: Value,
    pub neighbor: Option<NodeView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandView {
    pub origin: String,
    pub direction: Direction,
    pub edges: Vec<EdgeView>,
}

fn strip(data: &Value) -> Value {
    match data {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !HIDDEN_FIELDS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn gloss(data: &Value) -> String {
    let text = GLOSS_FIELDS
        .iter()
        .find_map(|k| data.get(*k).and_then(Value::as_str))
        .unwrap_or_default();
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    line.chars().take(GLOSS_CHARS).collect()
}

pub fn node_view(graph: &str, hit: &NodeHit, ids: &mut ShortIds) -> NodeView {
    NodeView {
        node: ids.assign(graph, &hit.id),
        name: hit.name.clone(),
        kind: hit.kind.clone(),
        gloss: gloss(&hit.data),
        status: hit.state.clone(),
        content: strip(&hit.data),
    }
}

pub fn expand_view(
    graph: &str,
    ex: &Expansion,
    direction: Direction,
    ids: &mut ShortIds,
) -> ExpandView {
    let nodes: HashMap<&str, &NodeHit> = ex.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let origin = ids.assign(graph, &ex.origin);
    let edges = ex
        .edges
        .iter()
        .map(|e| {
            let src = ids.assign(graph, &e.src);
            let dst = ids.assign(graph, &e.dst);
            let (from, to, far) = match direction {
                Direction::Out => (src.clone(), dst.clone(), &e.dst),
                Direction::In => (dst.clone(), src.clone(), &e.src),
            };
            let evidence = match &e.evidence {
                Evidence::Present(items) => Value::Array(items.clone()),
                Evidence::NoEvidence => Value::String(NO_EVIDENCE.to_string()),
            };
            EdgeView {
                from,
                to,
                src,
                dst,
                relation: e.relation.clone(),
                depth: e.depth,
                certainty: e.certainty.clone(),
                evidence,
                neighbor: nodes.get(far.as_str()).map(|n| node_view(graph, n, ids)),
            }
        })
        .collect();
    ExpandView {
        origin,
        direction,
        edges,
    }
}
