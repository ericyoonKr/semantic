use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use crate::error::KgError;

const TABLES: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE nodes (id TEXT PRIMARY KEY, kind TEXT, name TEXT, data TEXT);
CREATE TABLE edges (src TEXT, dst TEXT, type TEXT, data TEXT);
CREATE TABLE refs (article_id TEXT, node_id TEXT);
CREATE TABLE states (node_id TEXT, data TEXT);
CREATE VIRTUAL TABLE nodes_fts USING fts5(name, tokenize='trigram');
";

const INDEXES: &str = "
CREATE INDEX edges_src_type ON edges (src, type);
CREATE INDEX edges_dst_type ON edges (dst, type);
CREATE INDEX refs_article ON refs (article_id);
CREATE INDEX states_node ON states (node_id);
";

pub(crate) fn create(path: &Path) -> Result<Connection, KgError> {
    let conn = Connection::open(path)?;
    conn.pragma_update_and_check(None, "journal_mode", "OFF", |r| r.get::<_, String>(0))?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.execute_batch(TABLES)?;
    Ok(conn)
}

pub(crate) fn finish(conn: &Connection) -> Result<(), KgError> {
    conn.execute_batch(INDEXES)?;
    Ok(())
}

pub(crate) fn open_read_only(
    path: &Path,
    mmap_bytes: i64,
    cache_kib: i64,
) -> Result<Connection, KgError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "query_only", true)?;
    conn.pragma_update_and_check(None, "mmap_size", mmap_bytes, |r| r.get::<_, i64>(0))?;
    conn.pragma_update(None, "cache_size", -cache_kib)?;
    Ok(conn)
}

pub(crate) fn put_meta(conn: &Connection, key: &str, value: &str) -> Result<(), KgError> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

pub(crate) fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>, KgError> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub(crate) fn stored_meta(path: &Path, keys: [&str; 2]) -> Option<[String; 2]> {
    if !path.is_file() {
        return None;
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let a = get_meta(&conn, keys[0]).ok().flatten()?;
    let b = get_meta(&conn, keys[1]).ok().flatten()?;
    Some([a, b])
}

pub(crate) fn count_rows(conn: &Connection, table: &str) -> Result<u64, KgError> {
    let n: i64 = conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
    u64::try_from(n).map_err(|_| KgError::Meta)
}

pub(crate) fn insert_node(
    conn: &Connection,
    id: &str,
    kind: &str,
    name: &str,
    data: &str,
) -> Result<(), KgError> {
    conn.prepare_cached("INSERT INTO nodes (id, kind, name, data) VALUES (?1, ?2, ?3, ?4)")?
        .execute(params![id, kind, name, data])?;
    let rowid = conn.last_insert_rowid();
    conn.prepare_cached("INSERT INTO nodes_fts (rowid, name) VALUES (?1, ?2)")?
        .execute(params![rowid, name])?;
    Ok(())
}

pub(crate) fn insert_edge(
    conn: &Connection,
    src: &str,
    dst: &str,
    kind: &str,
    data: &str,
) -> Result<(), KgError> {
    conn.prepare_cached("INSERT INTO edges (src, dst, type, data) VALUES (?1, ?2, ?3, ?4)")?
        .execute(params![src, dst, kind, data])?;
    Ok(())
}

pub(crate) fn insert_state(
    conn: &Connection,
    node_id: Option<&str>,
    data: &str,
) -> Result<(), KgError> {
    conn.prepare_cached("INSERT INTO states (node_id, data) VALUES (?1, ?2)")?
        .execute(params![node_id, data])?;
    Ok(())
}
