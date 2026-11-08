use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use engine::ClaimGraph;
use kg::KgSet;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::RulesError;
use crate::rules::rows;
use crate::rules::set::RuleSet;
use crate::tasks;

pub const RULES_BUNDLE: &str = "semantic-rules";
const MANIFEST: &str = "manifest.json";

#[derive(Deserialize)]
struct Manifest {
    graphs: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(alias = "graph")]
    name: String,
    file: String,
    sha256: String,
}

fn plain_file(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn load_bundle(kg_dir: &Path, kg: &KgSet) -> Result<BTreeMap<String, ClaimGraph>, RulesError> {
    let dir = kg_dir.join("consumer").join(RULES_BUNDLE);
    let raw = match fs::read(dir.join(MANIFEST)) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e.into()),
    };
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(|_| RulesError::Manifest)?;
    let verified: HashMap<String, String> = kg
        .catalog()
        .into_iter()
        .filter(|e| e.bundle == RULES_BUNDLE)
        .map(|e| (e.graph, e.sha256))
        .collect();
    let mut trees = BTreeMap::new();
    for entry in manifest.graphs {
        if !plain_file(&entry.file) || trees.contains_key(&entry.name) {
            return Err(RulesError::Manifest);
        }
        let Some(checked) = verified.get(&kg::graph_name(RULES_BUNDLE, &entry.name)) else {
            return Err(RulesError::NotVerified(entry.name));
        };
        let body = fs::read(dir.join(&entry.file))?;
        let actual = sha256_hex(&body);
        if !actual.eq_ignore_ascii_case(checked) || !actual.eq_ignore_ascii_case(&entry.sha256) {
            return Err(RulesError::Sha256(entry.name));
        }
        let text = String::from_utf8(body).map_err(|_| RulesError::Line(entry.name.clone(), 0))?;
        let rows = rows::parse(&text).map_err(|line| RulesError::Line(entry.name.clone(), line))?;
        if rows.graph != entry.name {
            return Err(RulesError::Manifest);
        }
        let tree = ClaimGraph::from_rows(rows.nodes, rows.edges, &rows.expected)
            .map_err(|e| RulesError::Graph(entry.name.clone(), e))?;
        trees.insert(entry.name, tree);
    }
    Ok(trees)
}

pub async fn startup(kg_dir: PathBuf, kg: Arc<KgSet>, cpu: &rayon::ThreadPool) -> RuleSet {
    match tasks::cpu(cpu, move || load_bundle(&kg_dir, &kg)).await {
        Ok(Ok(trees)) => {
            tracing::info!(trees = trees.len(), "requirement trees ready");
            RuleSet::new(trees)
        }
        Ok(Err(e)) => {
            tracing::error!(error = %e, "requirement trees could not be loaded; judging is off");
            RuleSet::default()
        }
        Err(_) => {
            tracing::error!("requirement tree loading failed; judging is off");
            RuleSet::default()
        }
    }
}
