use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::contract::Contract;
use crate::error::KgError;
use crate::store;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisabledBundle {
    pub bundle: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogEntry {
    pub graph: String,
    pub bundle: String,
    pub shape: String,
    pub nodes: u64,
    pub edges: u64,
    pub states: u64,
    pub measurement: Value,
    pub signature: String,
    pub acyclic: Value,
    pub sha256: String,
}

pub(crate) struct Graph {
    pub entry: CatalogEntry,
    pub allowed: BTreeSet<String>,
    path: PathBuf,
    mmap_bytes: i64,
    cache_kib: i64,
    pool: Mutex<Vec<Connection>>,
}

#[derive(Default)]
pub struct KgSet {
    pub(crate) graphs: BTreeMap<String, Graph>,
    pub(crate) disabled: Vec<DisabledBundle>,
}

fn meta_json(conn: &Connection, key: &str) -> Result<Value, KgError> {
    let raw = store::get_meta(conn, key)?.ok_or(KgError::Meta)?;
    serde_json::from_str(&raw).map_err(|_| KgError::Meta)
}

fn meta_text(conn: &Connection, key: &str) -> Result<String, KgError> {
    store::get_meta(conn, key)?.ok_or(KgError::Meta)
}

impl Graph {
    pub(crate) fn open(path: &Path, mmap_bytes: i64, cache_kib: i64) -> Result<Self, KgError> {
        let conn = store::open_read_only(path, mmap_bytes, cache_kib)?;
        let counts = meta_json(&conn, "counts")?;
        let count = |k: &str| counts.get(k).and_then(Value::as_u64).ok_or(KgError::Meta);
        let entry = CatalogEntry {
            graph: meta_text(&conn, "graph")?,
            bundle: meta_text(&conn, "bundle")?,
            shape: meta_text(&conn, "shape")?,
            nodes: count("nodes")?,
            edges: count("edges")?,
            states: count("states")?,
            measurement: meta_json(&conn, "measurement")?,
            signature: meta_text(&conn, "signature")?,
            acyclic: meta_json(&conn, "acyclic")?,
            sha256: meta_text(&conn, "sha256")?,
        };
        let allowed = Contract::from_value(&meta_json(&conn, "contract")?)?.allowed;
        Ok(Self {
            entry,
            allowed,
            path: path.to_path_buf(),
            mmap_bytes,
            cache_kib,
            pool: Mutex::new(vec![conn]),
        })
    }

    pub(crate) fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, KgError>,
    ) -> Result<T, KgError> {
        let taken = self.pool.lock().map_err(|_| KgError::Pool)?.pop();
        let conn = match taken {
            Some(c) => c,
            None => store::open_read_only(&self.path, self.mmap_bytes, self.cache_kib)?,
        };
        let out = f(&conn);
        if let Ok(mut pool) = self.pool.lock() {
            pool.push(conn);
        }
        out
    }
}

impl KgSet {
    pub fn catalog(&self) -> Vec<CatalogEntry> {
        self.graphs.values().map(|g| g.entry.clone()).collect()
    }

    pub fn disabled(&self) -> &[DisabledBundle] {
        &self.disabled
    }

    pub fn contains(&self, graph: &str) -> bool {
        self.graphs.contains_key(graph)
    }

    pub(crate) fn graph(&self, name: &str) -> Result<&Graph, KgError> {
        self.graphs.get(name).ok_or(KgError::UnknownGraph)
    }
}
