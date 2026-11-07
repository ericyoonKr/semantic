use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::config::KgConfig;
use crate::contract;
use crate::error::KgError;
use crate::load;
use crate::manifest::{GraphEntry, Manifest, plain_name};
use crate::set::{DisabledBundle, Graph, KgSet};
use crate::store;
use crate::verify::{HashingReader, SignatureStatus, file_sha256, same_hash};

const MANIFEST: &str = "manifest.json";
const SIGNATURE: &str = "manifest.json.sig";

pub fn sync_all(kg_dir: &Path, cfg: &KgConfig) -> Result<KgSet, KgError> {
    let mut set = KgSet::default();
    let entries = match fs::read_dir(kg_dir.join("consumer")) {
        Ok(e) => e,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(set),
        Err(e) => return Err(e.into()),
    };
    let mut bundles: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        bundles.push((name, entry.path()));
    }
    bundles.sort();
    for (bundle, dir) in bundles {
        match sync_bundle(kg_dir, &bundle, &dir, cfg)
            .and_then(|ready| open_bundle(&set, &ready, cfg))
        {
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
    Ok(set)
}

fn open_bundle(
    set: &KgSet,
    ready: &[(String, PathBuf)],
    cfg: &KgConfig,
) -> Result<Vec<(String, Graph)>, KgError> {
    let mut out = Vec::with_capacity(ready.len());
    for (name, path) in ready {
        if set.contains(name) {
            return Err(KgError::DuplicateGraph);
        }
        out.push((
            name.clone(),
            Graph::open(path, cfg.mmap_bytes, cfg.cache_kib)?,
        ));
    }
    Ok(out)
}

fn sync_bundle(
    kg_dir: &Path,
    bundle: &str,
    dir: &Path,
    cfg: &KgConfig,
) -> Result<Vec<(String, PathBuf)>, KgError> {
    if !plain_name(bundle) {
        return Err(KgError::BadName);
    }
    let manifest_bytes = fs::read(dir.join(MANIFEST))?;
    let manifest = Manifest::parse(&manifest_bytes, &cfg.manifest_schema)?;
    let mut names = BTreeSet::new();
    if !manifest
        .graphs
        .iter()
        .all(|g| names.insert(g.name.as_str()))
    {
        return Err(KgError::DuplicateGraph);
    }
    let signature = match fs::read(dir.join(SIGNATURE)) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let status = cfg.signature.verify(&manifest_bytes, signature.as_deref());
    if status == SignatureStatus::Rejected {
        return Err(KgError::Signature);
    }
    let out_dir = kg_dir.join("sqlite").join(bundle);
    fs::create_dir_all(&out_dir)?;

    let ctx = BundleCtx {
        bundle,
        dir,
        out_dir: &out_dir,
        cfg,
        status,
    };
    let mut pending: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut ready = Vec::with_capacity(manifest.graphs.len());
    for entry in &manifest.graphs {
        let final_path = out_dir.join(format!("{}.sqlite", entry.name));
        match prepare_graph(&ctx, entry, &final_path) {
            Ok(Some(tmp)) => pending.push((tmp, final_path.clone())),
            Ok(None) => {}
            Err(e) => {
                discard(&pending);
                return Err(e);
            }
        }
        ready.push((entry.name.clone(), final_path));
    }
    for (i, (tmp, final_path)) in pending.iter().enumerate() {
        if let Err(e) = fs::rename(tmp, final_path) {
            discard(&pending[i..]);
            return Err(e.into());
        }
    }
    Ok(ready)
}

struct BundleCtx<'a> {
    bundle: &'a str,
    dir: &'a Path,
    out_dir: &'a Path,
    cfg: &'a KgConfig,
    status: SignatureStatus,
}

fn discard(pending: &[(PathBuf, PathBuf)]) {
    for (tmp, _) in pending {
        let _ = fs::remove_file(tmp);
    }
}

fn remove_if_exists(path: &Path) -> Result<(), KgError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn prepare_graph(
    ctx: &BundleCtx<'_>,
    entry: &GraphEntry,
    final_path: &Path,
) -> Result<Option<PathBuf>, KgError> {
    let jsonl = ctx.dir.join(&entry.file);
    let sha = file_sha256(&jsonl)?;
    if !same_hash(&sha, &entry.sha256) {
        return Err(KgError::Sha256);
    }
    if store::stored_sha256(final_path).is_some_and(|s| same_hash(&s, &sha)) {
        return Ok(None);
    }
    let tmp = ctx.out_dir.join(format!("{}.sqlite.tmp", entry.name));
    remove_if_exists(&tmp)?;
    match build(ctx, entry, &jsonl, &tmp) {
        Ok(()) => Ok(Some(tmp)),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn build(ctx: &BundleCtx<'_>, entry: &GraphEntry, jsonl: &Path, tmp: &Path) -> Result<(), KgError> {
    let mut conn = store::create(tmp)?;
    let mut reader = HashingReader::open(jsonl)?;
    let loaded = load::load(&mut reader, &mut conn, &ctx.cfg.consumer_schema)?;
    let streamed = reader.finish_hex();
    if !same_hash(&streamed, &entry.sha256) {
        return Err(KgError::Sha256);
    }
    for relation in &loaded.contract.acyclic {
        if contract::has_cycle(&conn, relation)? {
            return Err(KgError::Cycle);
        }
    }
    let acyclic = if loaded.contract.acyclic.is_empty() {
        json!("not_specified")
    } else {
        json!({ "checked": loaded.contract.acyclic })
    };
    let head = &loaded.head;
    let shape = head
        .get("shape")
        .and_then(Value::as_str)
        .or_else(|| entry.rest.get("shape").and_then(Value::as_str))
        .ok_or(KgError::HeadField("shape"))?;
    let counts = json!({
        "nodes": loaded.counts.nodes,
        "edges": loaded.counts.edges,
        "states": loaded.counts.states,
    });
    let field = |k: &str| head.get(k).cloned().unwrap_or(Value::Null).to_string();
    let rows: [(&str, String); 11] = [
        ("graph", entry.name.clone()),
        ("bundle", ctx.bundle.to_string()),
        ("sha256", streamed),
        ("schema", ctx.cfg.consumer_schema.clone()),
        ("shape", shape.to_string()),
        ("contract", field("contract")),
        ("measurement", field("measurement")),
        ("counts", counts.to_string()),
        ("signature", ctx.status.as_str().to_string()),
        ("acyclic", acyclic.to_string()),
        ("manifest_entry", entry.as_value().to_string()),
    ];
    for (key, value) in &rows {
        store::put_meta(&conn, key, value)?;
    }
    store::put_meta(&conn, "head", &head.to_string())?;
    store::finish(&conn)?;
    conn.close().map_err(|(_, e)| KgError::Sqlite(e))?;
    Ok(())
}
