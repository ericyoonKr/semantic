use std::collections::HashMap;

use kg::{
    AsOfSummary, Direction, Evidence, Expansion, Found, NodeHit, Period, Reason, ShortIds, Status,
};
use serde::Serialize;
use serde_json::Value;

const GLOSS_CHARS: usize = 160;
const NO_EVIDENCE: &str = "근거 없음";
const HIDDEN_FIELDS: [&str; 3] = ["id", "source", "target"];

pub struct GraphRef<'a> {
    pub name: &'a str,
    pub sha256: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeView {
    pub node: String,
    pub name: String,
    pub kind: String,
    pub gloss: String,
    pub evidence: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<Period>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certainty: Option<Value>,
    pub evidence: Value,
    pub checks: serde_json::Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<Period>,
    pub neighbor: Option<NodeView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub basis: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<AsOfSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExpandView {
    #[serde(flatten)]
    pub outcome: Outcome,
    pub origin: String,
    pub direction: Direction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composition_mode: Option<String>,
    pub refused_paths: u64,
    pub edges: Vec<EdgeView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindView {
    #[serde(flatten)]
    pub outcome: Outcome,
    pub nodes: Vec<NodeView>,
}

pub fn reason_text(reason: Reason) -> &'static str {
    match reason {
        Reason::UnknownGraph => "카탈로그에 없는 그래프입니다",
        Reason::RelationNotInContract => "그래프 계약에 없는 관계입니다",
        Reason::NoCompositions => {
            "이 그래프 계약에는 허용된 두 단계 경로가 없습니다 — 깊이 1로 한 걸음씩 펼칩니다"
        }
        Reason::NoArticleIndex => "이 그래프에는 조문으로 찾는 색인이 없습니다",
        Reason::ArticleKeyUnresolved => {
            "법령과 조문을 이 그래프의 판본 키로 바꾸는 표가 없어 찾지 않았습니다"
        }
        Reason::NoMatch => "찾은 노드가 없습니다 — 그래프에 없다는 뜻은 아닙니다",
        Reason::RecordedNone => "생산 쪽 찾아본 기록에 이 관계가 없다고 적혀 있습니다",
        Reason::RecordedUnknown => {
            "생산 쪽 찾아본 기록에 이 관계를 확인하지 못했다고 적혀 있습니다"
        }
        Reason::NoRecord => "펼칠 간선이 없고, 생산 쪽이 이 관계를 찾아본 기록도 없습니다",
        Reason::ExcludedByAsOf => {
            "기준일에 효력이 있는 항목이 없습니다 — 기준일의 「없음」이 확인된 것은 아닙니다"
        }
        Reason::NodeNotInGraph => "이 그래프에 이 노드가 없습니다",
    }
}

pub fn stale_text() -> &'static str {
    "그래프가 바뀌어 이 번호의 노드를 지금 그래프에서 찾지 못했습니다 — kg_find로 다시 찾아 주세요"
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

fn gloss(text: Option<&str>) -> String {
    let line = text
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    line.chars().take(GLOSS_CHARS).collect()
}

fn evidence(e: &Evidence) -> Value {
    match e {
        Evidence::NoEvidence => Value::String(NO_EVIDENCE.to_string()),
        Evidence::Present(items) => Value::Array(
            items
                .iter()
                .map(|it| {
                    let mut map = serde_json::Map::new();
                    map.insert("kind".into(), Value::String(it.field.clone()));
                    match &it.item {
                        Value::Object(fields) => map.extend(fields.clone()),
                        other => {
                            map.insert("value".into(), other.clone());
                        }
                    }
                    Value::Object(map)
                })
                .collect(),
        ),
    }
}

pub fn outcome(
    status: Status,
    reason: Option<Reason>,
    basis: &[String],
    as_of: Option<&AsOfSummary>,
) -> Outcome {
    Outcome {
        status,
        reason: reason.map(|r| reason_text(r).to_string()),
        basis: basis.to_vec(),
        as_of: as_of.cloned(),
    }
}

pub fn node_view(graph: &GraphRef<'_>, hit: &NodeHit, ids: &mut ShortIds) -> NodeView {
    NodeView {
        node: ids.assign(graph.name, &hit.id, graph.sha256),
        name: hit.name.clone(),
        kind: hit.kind.clone(),
        gloss: gloss(hit.gloss.as_deref()),
        evidence: evidence(&hit.evidence),
        status: hit.state.clone(),
        period: hit.period,
        content: strip(&hit.data),
    }
}

pub fn find_view(graph: &GraphRef<'_>, found: &Found, ids: &mut ShortIds) -> FindView {
    FindView {
        outcome: outcome(found.status, found.reason, &[], found.as_of.as_ref()),
        nodes: found
            .nodes
            .iter()
            .map(|h| node_view(graph, h, ids))
            .collect(),
    }
}

pub fn expand_view(
    graph: &GraphRef<'_>,
    ex: &Expansion,
    direction: Direction,
    ids: &mut ShortIds,
) -> ExpandView {
    let nodes: HashMap<&str, &NodeHit> = ex.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let origin = ids.assign(graph.name, &ex.origin, graph.sha256);
    let edges = ex
        .edges
        .iter()
        .map(|e| {
            let src = ids.assign(graph.name, &e.src, graph.sha256);
            let dst = ids.assign(graph.name, &e.dst, graph.sha256);
            let (from, to, far) = match direction {
                Direction::Out => (src.clone(), dst.clone(), &e.dst),
                Direction::In => (dst.clone(), src.clone(), &e.src),
            };
            EdgeView {
                from,
                to,
                src,
                dst,
                relation: e.relation.clone(),
                depth: e.depth,
                certainty: e.certainty.clone(),
                evidence: evidence(&e.evidence),
                checks: e.checks.clone(),
                period: e.period,
                neighbor: nodes.get(far.as_str()).map(|n| node_view(graph, n, ids)),
            }
        })
        .collect();
    ExpandView {
        outcome: outcome(ex.status, ex.reason, &ex.basis, ex.as_of.as_ref()),
        origin,
        direction,
        composition_mode: ex.composition_mode.clone(),
        refused_paths: ex.refused_paths,
        edges,
    }
}
