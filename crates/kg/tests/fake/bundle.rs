#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use kg::KgConfig;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const CONSUMER: &str = "test-consumer/1";
pub const MANIFEST: &str = "test-manifest/1";

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

pub fn cfg() -> KgConfig {
    KgConfig::new(CONSUMER, MANIFEST)
}

pub struct Spec<'a> {
    pub graph: &'a str,
    pub schema: &'a str,
    pub shape: &'a str,
    pub extra_edges: Vec<Value>,
    pub compositions: Value,
}

impl<'a> Spec<'a> {
    pub fn new(graph: &'a str) -> Self {
        Self {
            graph,
            schema: CONSUMER,
            shape: "연결망",
            extra_edges: Vec::new(),
            compositions: json!([["rel_x", "kind_a", "rel_x"]]),
        }
    }
}

pub fn edge(id: &str, src: &str, dst: &str, relation: &str, evidence: Value) -> Value {
    json!({"edge": {
        "id": id,
        "source": {"graph": "g", "id": src},
        "target": {"graph": "g", "id": dst},
        "relation": relation,
        "certainty": "uncertain",
        "evidence": evidence,
        "evidence_check": {"state": "unchecked", "reason_ref": 1}
    }})
}

pub fn lines(spec: &Spec<'_>) -> Vec<String> {
    let mut items = vec![
        json!({"node": {"id": "n1", "kind": "kind_a", "label": "alpha concept", "evidence": []}}),
        json!({"node": {"id": "n2", "kind": "kind_a", "label": "beta idea", "evidence": [],
            "item_state": {"judgment": "unknown", "reason_ref": 1}}}),
        json!({"node": {"id": "n3", "kind": "kind_b", "label": "gamma group",
            "evidence": [{"source": "s", "quote_ref": 2}], "record": {"gloss": "gamma gloss"}}}),
        json!({"node": {"id": "n4", "kind": "kind_a", "label": "xy pair", "evidence": [],
            "item_state": {"judgment": "not_judged", "search_records": [
                {"relation": "rel_x", "direction": "out", "result": "none_confirmed", "basis_ref": 1},
                {"relation": "rel_z", "direction": "both", "result": "unknown", "basis_ref": 1}]}}}),
        edge(
            "e1",
            "n1",
            "n2",
            "rel_x",
            json!([{"source": "s", "quote_ref": 0}]),
        ),
        {
            let mut e = edge("e2", "n2", "n3", "rel_x", json!([]));
            e["edge"]["book_evidence"] = json!([{"source": "book", "loc": {"book": [7]}}]);
            e
        },
        {
            let mut e = edge("e3", "n1", "n4", "rel_y", json!([]));
            e["edge"]["period"] =
                json!({"state": "known", "start": "2020-01-01", "end": "2021-01-01"});
            e
        },
        json!({"unattached_state": {"judgment": "not_judged", "reason_ref": 1}}),
    ];
    items.extend(spec.extra_edges.iter().cloned());
    let count = |k: &str| items.iter().filter(|v| v.get(k).is_some()).count();
    let states = count("unattached_state")
        + items
            .iter()
            .filter(|v| v.get("node").and_then(|n| n.get("item_state")).is_some())
            .count();
    let head = json!({"head": {
        "schema": spec.schema,
        "graph": spec.graph,
        "shape": spec.shape,
        "state": {"kind": "note", "values": ["yes", "no"]},
        "contract": {
            "node_kinds": ["kind_a"],
            "relations": {"rel_x": {"acyclic": true}},
            "optional_relations": ["rel_y", "rel_z"],
            "compositions": spec.compositions,
            "composition_mode": "chain_only"
        },
        "measurement": {"state": "unmeasured"},
        "counts": {"nodes": count("node"), "edges": count("edge"), "states": states},
        "texts": ["quoted line", "reason text", "node quote"]
    }});
    std::iter::once(head)
        .chain(items)
        .map(|v| v.to_string())
        .collect()
}

pub fn nodes(
    set: &kg::KgSet,
    graph: &str,
    by: kg::FindBy<'_>,
    kind: Option<&str>,
    limit: usize,
) -> Result<Vec<kg::NodeHit>, kg::KgError> {
    let q = kg::FindQuery {
        by,
        kind,
        limit,
        as_of: None,
    };
    Ok(set.find(graph, &q)?.nodes)
}

pub fn expand(
    set: &kg::KgSet,
    node: &str,
    relation: Option<&str>,
    direction: kg::Direction,
    depth: u8,
    limit: usize,
) -> Result<kg::Expansion, kg::KgError> {
    let q = kg::ExpandQuery {
        relation,
        direction,
        depth,
        limit,
        as_of: None,
    };
    set.expand("b1/g", node, &q)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn write_bundle(
    kg_dir: &Path,
    bundle: &str,
    graph: &str,
    lines: &[String],
    sha: Option<&str>,
) -> Result<PathBuf, std::io::Error> {
    let dir = kg_dir.join("consumer").join(bundle);
    fs::create_dir_all(&dir)?;
    let file = format!("{graph}.jsonl");
    let mut body = lines.join("\n");
    body.push('\n');
    fs::write(dir.join(&file), body.as_bytes())?;
    let digest = sha
        .map(str::to_string)
        .unwrap_or_else(|| sha256_hex(body.as_bytes()));
    let manifest = json!({
        "schema": MANIFEST,
        "source_bundle": bundle,
        "graphs": [{"name": graph, "file": file, "sha256": digest, "shape": "연결망"}],
        "version_mismatches": []
    });
    fs::write(dir.join("manifest.json"), manifest.to_string())?;
    Ok(dir)
}

pub fn sqlite_files(kg_dir: &Path, bundle: &str) -> Vec<String> {
    match fs::read_dir(kg_dir.join("sqlite").join(bundle)) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}
