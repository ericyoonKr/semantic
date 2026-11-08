use engine::{
    ArithSpec, Burden, ClaimEdge, ClaimNode, ClaimType, Evidence, ExpectedCounts, GateType,
    NodeId, PeriodRule, Relation,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug)]
pub struct TreeRows {
    pub graph: String,
    pub expected: ExpectedCounts,
    pub nodes: Vec<ClaimNode>,
    pub edges: Vec<ClaimEdge>,
}

#[derive(Deserialize)]
struct Head {
    graph: String,
    counts: HeadCounts,
}

#[derive(Deserialize)]
struct HeadCounts {
    nodes: usize,
    edges: usize,
    leaves: usize,
}

#[derive(Deserialize)]
struct NodeRow {
    id: NodeId,
    label: String,
    gate: GateType,
    burden: Option<Burden>,
    proposition: Option<String>,
    ask: Option<String>,
    negation_of: Option<NodeId>,
    arithmetic: Option<ArithSpec>,
    period_rule: Option<PeriodRule>,
    #[serde(default)]
    evidence: Vec<Evidence>,
}

#[derive(Deserialize)]
struct EdgeRow {
    source: Value,
    target: Value,
    relation: Relation,
}

fn endpoint(v: &Value) -> Option<NodeId> {
    match v {
        Value::String(s) => Some(NodeId::new(s.clone())),
        Value::Object(map) => map.get("id").and_then(Value::as_str).map(NodeId::new),
        _ => None,
    }
}

fn tagged(line: &str) -> Option<(String, Value)> {
    let Value::Object(map) = serde_json::from_str::<Value>(line).ok()? else {
        return None;
    };
    if map.len() != 1 {
        return None;
    }
    map.into_iter().next()
}

pub fn parse(text: &str) -> Result<TreeRows, usize> {
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .filter(|(_, l)| !l.trim().is_empty());
    let (first, head_line) = lines.next().ok_or(1usize)?;
    let head: Head = match tagged(head_line) {
        Some((tag, body)) if tag == "head" => serde_json::from_value(body).map_err(|_| first)?,
        _ => return Err(first),
    };
    let claim_type = ClaimType(head.graph.clone());
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for (no, line) in lines {
        let (tag, body) = tagged(line).ok_or(no)?;
        match tag.as_str() {
            "node" => {
                let row: NodeRow = serde_json::from_value(body).map_err(|_| no)?;
                nodes.push(ClaimNode {
                    id: row.id,
                    claim_type: claim_type.clone(),
                    label: row.label,
                    gate: row.gate,
                    burden: row.burden.ok_or(no)?,
                    proposition: row.proposition,
                    ask: row.ask,
                    negation_of: row.negation_of,
                    arithmetic: row.arithmetic,
                    period_rule: row.period_rule,
                    evidence: row.evidence,
                });
            }
            "edge" => {
                let row: EdgeRow = serde_json::from_value(body).map_err(|_| no)?;
                edges.push(ClaimEdge {
                    parent: endpoint(&row.source).ok_or(no)?,
                    child: endpoint(&row.target).ok_or(no)?,
                    relation: row.relation,
                });
            }
            _ => return Err(no),
        }
    }
    Ok(TreeRows {
        graph: head.graph,
        expected: ExpectedCounts {
            nodes: head.counts.nodes,
            edges: head.counts.edges,
            leaves: head.counts.leaves,
        },
        nodes,
        edges,
    })
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn emitted_lines_become_engine_rows() {
        let text = concat!(
            "{\"head\":{\"graph\":\"T\",\"counts\":{\"nodes\":2,\"edges\":1,\"states\":0,\"leaves\":1}}}\n",
            "{\"node\":{\"id\":\"R\",\"gate\":\"AND\",\"label\":\"r\",\"kind\":\"rule\",\"burden\":\"pending\"}}\n",
            "{\"node\":{\"id\":\"L\",\"gate\":\"LEAF\",\"label\":\"l\",\"proposition\":\"p\",\"ask\":\"a\",",
            "\"kind\":\"rule\",\"burden\":\"pending\",\"evidence\":[{\"law\":\"X\",\"unit\":\"Y\"}]}}\n",
            "{\"edge\":{\"source\":\"R\",\"target\":{\"id\":\"L\"},\"relation\":\"element\",\"evidence\":[]}}\n",
        );
        let rows = parse(text);
        assert!(rows.is_ok(), "{rows:?}");
        let Ok(rows) = rows else { return };
        assert_eq!(rows.graph, "T");
        assert_eq!(rows.nodes.len(), 2);
        assert_eq!(rows.nodes[1].claim_type.0, "T");
        assert_eq!(rows.nodes[1].evidence[0].law, "X");
        assert_eq!(rows.edges[0].child.as_str(), "L");
        assert_eq!(rows.expected.leaves, 1);
    }

    #[test]
    fn bad_lines_report_their_number() {
        let head = "{\"head\":{\"graph\":\"T\",\"counts\":{\"nodes\":1,\"edges\":0,\"leaves\":1}}}\n";
        assert_eq!(parse("").err(), Some(1));
        assert_eq!(parse("{\"node\":{}}\n").err(), Some(1));
        let missing_burden = format!(
            "{head}{}",
            "{\"node\":{\"id\":\"L\",\"gate\":\"LEAF\",\"label\":\"l\"}}\n"
        );
        assert_eq!(parse(&missing_burden).err(), Some(2));
        assert_eq!(parse(&format!("{head}\n{{\"state\":{{}}}}\n")).err(), Some(3));
    }
}
