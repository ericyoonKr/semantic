use std::fs;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::config::KgConfig;
use crate::contract::{self, Contract};
use crate::error::KgError;
use crate::fields;
use crate::load;
use crate::naming;
use crate::set::{DisabledBundle, Graph, KgSet};
use crate::store;
use crate::sync::LOADER;
use crate::verify::{HashingReader, SignatureStatus, file_sha256, same_hash};

const BUILD: &str = "legal_build.yaml";
const BUILD_SIGNATURE: &str = "legal_build.yaml.sig";
const CONTRACT: &str = "legal_contract.yaml";
const TYPES: &str = "types.yaml";
const FILES: &str = "파일";
const TREE_GROUP: &str = "판정 관계";
const TREE_SHAPE: &str = "나무";
const NET_SHAPE: &str = "연결망";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Nodes,
    Edges,
    Other,
}

struct Bundle<'a> {
    name: &'a str,
    cfg: &'a KgConfig,
    dir: &'a Path,
    out_dir: PathBuf,
    files: serde_json::Map<String, Value>,
    contract: Value,
    types: Value,
    status: SignatureStatus,
}

struct Planned {
    graph: String,
    file: PathBuf,
    sha256: String,
    shape: String,
    contract: Value,
}

pub(crate) fn sync_sources(kg_dir: &Path, cfg: &KgConfig, set: &mut KgSet) -> Result<(), KgError> {
    if cfg.source_schema.is_none() {
        return Ok(());
    }
    let entries = match fs::read_dir(kg_dir.join("source")) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let mut bundles: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join(BUILD).is_file() {
            bundles.push((entry.file_name().to_string_lossy().into_owned(), entry.path()));
        }
    }
    bundles.sort();
    let superseded = naming::superseded(bundles.iter().map(|(name, _)| name.as_str()));
    for ((bundle, dir), old) in bundles.into_iter().zip(superseded) {
        let opened = if old {
            Err(KgError::Superseded)
        } else {
            sync_bundle(kg_dir, &bundle, &dir, cfg).and_then(|ready| open_all(set, &ready, cfg))
        };
        match opened {
            Ok(graphs) => {
                for (name, graph) in graphs {
                    set.graphs.insert(name, graph);
                }
            }
            Err(e) => set.disabled.push(DisabledBundle {
                bundle,
                reason: e.to_string(),
            }),
        }
    }
    Ok(())
}

fn open_all(
    set: &KgSet,
    ready: &[(String, PathBuf)],
    cfg: &KgConfig,
) -> Result<Vec<(String, Graph)>, KgError> {
    let mut out = Vec::with_capacity(ready.len());
    for (name, path) in ready {
        if set.contains(name) {
            return Err(KgError::DuplicateGraph);
        }
        out.push((name.clone(), Graph::open(name, path, cfg.mmap_bytes, cfg.cache_kib)?));
    }
    Ok(out)
}

fn read_yaml(path: &Path) -> Result<Value, KgError> {
    let bytes = fs::read(path)?;
    serde_yaml::from_slice(&bytes).map_err(|_| KgError::Manifest)
}

fn listed_sha<'a>(files: &'a serde_json::Map<String, Value>, rel: &str) -> Result<&'a str, KgError> {
    files.get(rel).and_then(Value::as_str).ok_or(KgError::Manifest)
}

fn checked_yaml(dir: &Path, files: &serde_json::Map<String, Value>, rel: &str) -> Result<Value, KgError> {
    let path = dir.join(rel);
    if !same_hash(&file_sha256(&path)?, listed_sha(files, rel)?) {
        return Err(KgError::Sha256);
    }
    read_yaml(&path)
}

fn sync_bundle(
    kg_dir: &Path,
    bundle: &str,
    dir: &Path,
    cfg: &KgConfig,
) -> Result<Vec<(String, PathBuf)>, KgError> {
    if !crate::manifest::plain_name(bundle) {
        return Err(KgError::BadName);
    }
    let build_bytes = fs::read(dir.join(BUILD))?;
    let signature = match fs::read(dir.join(BUILD_SIGNATURE)) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let status = cfg.signature.verify(&build_bytes, signature.as_deref());
    if status == SignatureStatus::Rejected {
        return Err(KgError::Signature);
    }
    let build: Value = serde_yaml::from_slice(&build_bytes).map_err(|_| KgError::Manifest)?;
    let files = build
        .get(FILES)
        .and_then(Value::as_object)
        .cloned()
        .ok_or(KgError::Manifest)?;
    let ctx = Bundle {
        name: bundle,
        cfg,
        dir,
        out_dir: kg_dir.join("sqlite").join(bundle),
        contract: checked_yaml(dir, &files, CONTRACT)?,
        types: checked_yaml(dir, &files, TYPES)?,
        files,
        status,
    };
    let planned = plan(&ctx)?;
    fs::create_dir_all(&ctx.out_dir)?;
    let mut ready = Vec::with_capacity(planned.len());
    for p in &planned {
        let final_path = ctx.out_dir.join(format!("{}.sqlite", p.graph));
        prepare(&ctx, p, &final_path)?;
        ready.push((naming::graph_name(bundle, &p.graph), final_path));
    }
    Ok(ready)
}

fn plan(ctx: &Bundle<'_>) -> Result<Vec<Planned>, KgError> {
    let locations = ctx
        .contract
        .get("graph_locations")
        .and_then(Value::as_object)
        .ok_or(KgError::Manifest)?;
    let mut out = Vec::new();
    for (graph, rel) in locations {
        let Some(rel) = rel.as_str() else { continue };
        let file = ctx.dir.join(rel);
        if !file.is_file() {
            continue;
        }
        if !crate::manifest::plain_name(graph) {
            return Err(KgError::BadName);
        }
        let Some(shape) = shape_of(&ctx.types, graph) else { continue };
        let contract = ctx
            .contract
            .get("graph_contracts")
            .and_then(|c| c.get(graph))
            .cloned()
            .ok_or(KgError::HeadField("contract"))?;
        out.push(Planned {
            graph: graph.clone(),
            file,
            sha256: listed_sha(&ctx.files, rel)?.to_string(),
            shape: shape.to_string(),
            contract,
        });
    }
    Ok(out)
}

fn names_in(v: Option<&Value>) -> Vec<&str> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn shape_of(types: &Value, graph: &str) -> Option<&'static str> {
    let kinds = types.get("graph_types")?.as_object()?;
    for kind in kinds.values() {
        let groups = kind.get("groups").and_then(Value::as_object);
        if groups
            .and_then(|g| g.get(TREE_GROUP))
            .is_some_and(|g| names_in(Some(g)).contains(&graph))
        {
            return Some(TREE_SHAPE);
        }
    }
    let concept = kinds.get("C")?;
    names_in(concept.get("families"))
        .contains(&graph)
        .then_some(NET_SHAPE)
}

fn prepare(ctx: &Bundle<'_>, p: &Planned, final_path: &Path) -> Result<(), KgError> {
    if store::stored_meta(final_path, ["sha256", "loader"])
        .is_some_and(|[s, loader]| same_hash(&s, &p.sha256) && loader == LOADER)
    {
        return Ok(());
    }
    let tmp = ctx.out_dir.join(format!("{}.sqlite.tmp", p.graph));
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    match build(ctx, p, &tmp) {
        Ok(()) => {
            fs::rename(&tmp, final_path)?;
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn build(ctx: &Bundle<'_>, p: &Planned, tmp: &Path) -> Result<(), KgError> {
    let contract = Contract::from_value(&p.contract)?;
    let source_schema = ctx.cfg.source_schema.as_deref().ok_or(KgError::Schema)?;
    let fields = fields::for_shape(&p.shape);
    let mut conn = store::create(tmp)?;
    let mut reader = HashingReader::open(&p.file)?;
    let (mut nodes, mut edges, mut dated) = (0u64, 0u64, 0u64);
    let mut schema_ok = false;
    {
        let tx = conn.transaction()?;
        stream(&mut reader, |item| match item {
            Item::Schema(s) => {
                schema_ok = s == source_schema;
                Ok(())
            }
            Item::Node(body, line) => {
                load::put_node(&tx, &body, line)?;
                nodes += 1;
                dated += u64::from(load::dated(&body, fields));
                Ok(())
            }
            Item::Edge(body, line) => {
                load::put_edge(&tx, &body, &contract, line)?;
                edges += 1;
                dated += u64::from(load::dated(&body, fields));
                Ok(())
            }
        })?;
        tx.commit()?;
    }
    if !schema_ok {
        return Err(KgError::Schema);
    }
    let streamed = reader.finish_hex();
    if !same_hash(&streamed, &p.sha256) {
        return Err(KgError::Sha256);
    }
    for relation in &contract.acyclic {
        if contract::has_cycle(&conn, relation)? {
            return Err(KgError::Cycle);
        }
    }
    let state = ctx
        .types
        .get("query_shapes")
        .and_then(|q| q.get(&p.shape))
        .and_then(|s| s.get("state"))
        .cloned()
        .unwrap_or(Value::Null);
    let head = json!({
        "schema": source_schema,
        "graph": p.graph,
        "shape": p.shape,
        "state": state,
        "contract": p.contract,
    });
    let acyclic = if contract.acyclic.is_empty() {
        json!("not_specified")
    } else {
        json!({ "checked": contract.acyclic })
    };
    let counts = json!({ "nodes": nodes, "edges": edges, "states": 0, "dated": dated });
    let entry = json!({ "name": p.graph, "file": p.file.file_name().map(|f| f.to_string_lossy().into_owned()), "sha256": p.sha256, "shape": p.shape });
    let rows: [(&str, String); 13] = [
        ("graph", p.graph.clone()),
        ("bundle", ctx.name.to_string()),
        ("sha256", streamed),
        ("schema", source_schema.to_string()),
        ("shape", p.shape.clone()),
        ("contract", p.contract.to_string()),
        ("measurement", Value::Null.to_string()),
        ("counts", counts.to_string()),
        ("signature", ctx.status.as_str().to_string()),
        ("acyclic", acyclic.to_string()),
        ("manifest_entry", entry.to_string()),
        ("loader", LOADER.to_string()),
        ("head", head.to_string()),
    ];
    for (key, value) in &rows {
        store::put_meta(&conn, key, value)?;
    }
    store::finish(&conn)?;
    conn.close().map_err(|(_, e)| KgError::Sqlite(e))?;
    Ok(())
}

enum Item {
    Schema(String),
    Node(Value, u64),
    Edge(Value, u64),
}

fn top_key(line: &str) -> Option<&str> {
    if line.starts_with([' ', '-', '#']) || line.trim().is_empty() {
        return None;
    }
    line.split_once(':').map(|(k, _)| k.trim())
}

fn flush(
    section: Section,
    buf: &mut String,
    start: u64,
    on_item: &mut impl FnMut(Item) -> Result<(), KgError>,
) -> Result<(), KgError> {
    if buf.is_empty() {
        return Ok(());
    }
    let parsed: Vec<Value> = serde_yaml::from_str(buf).map_err(|_| KgError::Json(start))?;
    buf.clear();
    let [body] = <[Value; 1]>::try_from(parsed).map_err(|_| KgError::UnknownItem(start))?;
    match section {
        Section::Nodes => on_item(Item::Node(body, start)),
        Section::Edges => on_item(Item::Edge(body, start)),
        Section::Other => Ok(()),
    }
}

fn stream(
    reader: impl std::io::Read,
    mut on_item: impl FnMut(Item) -> Result<(), KgError>,
) -> Result<(), KgError> {
    let mut section = Section::Other;
    let mut buf = String::new();
    let mut start: u64 = 0;
    let mut line_no: u64 = 0;
    let mut schema: Option<String> = None;
    for line in BufReader::with_capacity(1024 * 1024, reader).lines() {
        let line = line?;
        line_no += 1;
        if line.starts_with('-') && section != Section::Other {
            flush(section, &mut buf, start, &mut on_item)?;
            start = line_no;
            buf.push_str(&line);
            buf.push('\n');
            continue;
        }
        if let Some(key) = top_key(&line) {
            flush(section, &mut buf, start, &mut on_item)?;
            if key == "schema_version" {
                schema = line.split_once(':').map(|(_, v)| v.trim().to_string());
            }
            section = match key {
                "nodes" => Section::Nodes,
                "edges" => Section::Edges,
                _ => Section::Other,
            };
            continue;
        }
        if !buf.is_empty() && !line.starts_with('#') {
            buf.push_str(&line);
            buf.push('\n');
        }
    }
    flush(section, &mut buf, start, &mut on_item)?;
    on_item(Item::Schema(schema.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::{Item, stream};

    const GRAPH: &str = "# kgio-stream/1
schema_version: test-source/1
id: t
roots:
- R
# kgio-stream-list: nodes
nodes:
- id: R
  label: 뿌리
  kind: rule
  gate: AND
- id: L
  label: 잎
  kind: rule
  gate: LEAF
  record:
    aliases:
    - 다른 잎
    gloss: |
      첫 줄

      셋째 줄
# kgio-stream-list: edges
edges:
- id: e1
  source:
    graph: t
    id: R
  target:
    graph: t
    id: L
  relation: element
notes: []
";

    #[test]
    fn streams_nodes_edges_and_schema_with_blank_lines_in_block_text() -> Result<(), crate::KgError> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut schema = String::new();
        stream(GRAPH.as_bytes(), |item| {
            match item {
                Item::Node(v, _) => nodes.push(v),
                Item::Edge(v, _) => edges.push(v),
                Item::Schema(s) => schema = s,
            }
            Ok(())
        })?;
        assert_eq!(schema, "test-source/1");
        assert_eq!(nodes.len(), 2);
        assert_eq!(edges.len(), 1);
        assert_eq!(nodes[1]["record"]["gloss"], "첫 줄\n\n셋째 줄\n");
        assert_eq!(nodes[1]["record"]["aliases"][0], "다른 잎");
        assert_eq!(edges[0]["target"]["id"], "L");
        Ok(())
    }
}
