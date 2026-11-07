use engine::{Burden, GateType, Relation};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::error::LoaderError;
use crate::source::RuleFile;

pub const SHAPE: &str = "나무";
pub const SOURCE_TYPE: &str = "R";
pub const NODE_KIND: &str = "rule";

pub struct Emitted {
    pub name: String,
    pub file: String,
    pub body: Vec<u8>,
    pub sha256: String,
}

pub fn graph(file: &RuleFile, consumer_schema: &str) -> Result<Emitted, LoaderError> {
    let name = file.claim_type.clone();
    if !plain_name(&name) {
        return Err(LoaderError::BadClaimName(name));
    }
    let relations: Map<String, Value> = Relation::ALL
        .iter()
        .map(|r| (r.as_str().to_string(), json!({"acyclic": true})))
        .collect();
    let leaves = file.nodes.iter().filter(|n| n.gate == GateType::Leaf).count();
    let head = json!({"head": {
        "schema": consumer_schema,
        "graph": name,
        "shape": SHAPE,
        "source_type": SOURCE_TYPE,
        "contract": {"node_kinds": [NODE_KIND], "relations": relations},
        "counts": {
            "nodes": file.nodes.len(),
            "edges": file.edges.len(),
            "states": 0,
            "leaves": leaves,
        },
    }});
    let mut lines = vec![head.to_string()];
    for node in &file.nodes {
        let mut body = serde_json::to_value(node)?;
        if let Value::Object(map) = &mut body {
            map.insert("kind".into(), Value::String(NODE_KIND.into()));
            let burden = node.burden.or(file.burden_pending.then_some(Burden::Pending));
            if let Some(b) = burden {
                map.insert("burden".into(), serde_json::to_value(b)?);
            }
        }
        lines.push(json!({ "node": body }).to_string());
    }
    for edge in &file.edges {
        let item = json!({"edge": {
            "source": edge.parent,
            "target": edge.child,
            "relation": edge.relation,
            "evidence": edge.evidence,
        }});
        lines.push(item.to_string());
    }
    let mut text = lines.join("\n");
    text.push('\n');
    let body = text.into_bytes();
    Ok(Emitted {
        file: format!("{name}.jsonl"),
        sha256: sha256_hex(&body),
        name,
        body,
    })
}

pub fn manifest(graphs: &[Emitted], manifest_schema: &str) -> Result<Vec<u8>, LoaderError> {
    let entries: Vec<Value> = graphs
        .iter()
        .map(|g| json!({"name": g.name, "file": g.file, "sha256": g.sha256, "shape": SHAPE}))
        .collect();
    let value = json!({"schema": manifest_schema, "graphs": entries});
    let mut text = serde_json::to_string_pretty(&value)?;
    text.push('\n');
    Ok(text.into_bytes())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{graph, manifest, sha256_hex};
    use crate::source::parse;

    const TREE: &str = "claim_type: T1
burden_pending: true
expected: { nodes: 3, edges: 2, leaves: 2 }
nodes:
  - { id: ROOT, gate: AND, label: root }
  - { id: L1, gate: LEAF, label: one, proposition: p1, ask: a1 }
  - { id: L2, gate: LEAF, label: two, proposition: p2, ask: a2, needs_split: true }
edges:
  - { parent: ROOT, child: L1, relation: element }
  - { parent: ROOT, child: L2, relation: defense }
";

    #[test]
    fn head_counts_and_hash_match_body() -> Result<(), Box<dyn std::error::Error>> {
        let file = parse(TREE)?;
        let out = graph(&file, "s/1")?;
        assert_eq!(out.sha256, sha256_hex(&out.body));
        let text = String::from_utf8(out.body.clone())?;
        let lines: Vec<Value> = text
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        assert_eq!(lines.len(), 6);
        let head = &lines[0]["head"];
        assert_eq!(head["schema"], "s/1");
        assert_eq!(head["shape"], "나무");
        assert_eq!(head["source_type"], "R");
        assert_eq!(head["counts"]["nodes"], 3);
        assert_eq!(head["counts"]["edges"], 2);
        assert_eq!(head["counts"]["leaves"], 2);
        let rels = head["contract"]["relations"].as_object().map(|m| m.len());
        assert_eq!(rels, Some(3));
        assert_eq!(lines[1]["node"]["kind"], "rule");
        assert_eq!(lines[1]["node"]["burden"], "pending");
        assert_eq!(lines[3]["node"]["needs_split"], true);
        assert_eq!(lines[5]["edge"]["relation"], "defense");
        assert_eq!(lines[5]["edge"]["source"], "ROOT");
        let m: Value = serde_json::from_slice(&manifest(&[out], "m/1")?)?;
        assert_eq!(m["schema"], "m/1");
        assert_eq!(m["graphs"][0]["file"], "T1.jsonl");
        Ok(())
    }

    #[test]
    fn kg_loader_reads_emitted_bundle() -> Result<(), Box<dyn std::error::Error>> {
        let file = parse(TREE)?;
        let out = graph(&file, "s/1")?;
        let manifest = manifest(std::slice::from_ref(&out), "m/1")?;
        let kg_dir = tempfile::tempdir()?;
        let bundle = kg_dir.path().join("consumer").join("rules");
        crate::write::replace_dir(
            &bundle,
            &[(out.file.clone(), out.body.clone()), ("manifest.json".into(), manifest)],
        )?;
        let set = kg::sync_all(kg_dir.path(), &kg::KgConfig::new("s/1", "m/1"))?;
        assert!(set.disabled().is_empty());
        let catalog = set.catalog();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].graph, "T1");
        assert_eq!(catalog[0].shape, "나무");
        assert_eq!((catalog[0].nodes, catalog[0].edges), (3, 2));
        Ok(())
    }
}
