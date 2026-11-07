use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Row, params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::KgError;
use crate::set::KgSet;

pub const MAX_LIMIT: usize = 20;
pub const MAX_DEPTH: u8 = 2;
const LIKE_MAX_CHARS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindBy<'a> {
    Text(&'a str),
    LawKey(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Out,
    In,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeHit {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub state: Option<Value>,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", content = "items", rename_all = "snake_case")]
pub enum Evidence {
    NoEvidence,
    Present(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExpandEdge {
    pub src: String,
    pub dst: String,
    pub relation: String,
    pub depth: u8,
    pub certainty: Option<Value>,
    pub evidence: Evidence,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Expansion {
    pub origin: String,
    pub edges: Vec<ExpandEdge>,
    pub nodes: Vec<NodeHit>,
}

fn check_limit(limit: usize) -> Result<(), KgError> {
    if limit == 0 || limit > MAX_LIMIT {
        return Err(KgError::Argument("limit"));
    }
    Ok(())
}

fn node_from_row(row: &Row<'_>) -> rusqlite::Result<NodeHit> {
    let raw: String = row.get(3)?;
    let data: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let state = data.get("item_state").filter(|v| !v.is_null()).cloned();
    Ok(NodeHit {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        state,
        data,
    })
}

fn escape_like(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('%');
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

fn fts_phrase(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

fn find_in(
    conn: &Connection,
    by: FindBy<'_>,
    kind: Option<&str>,
    limit: usize,
) -> Result<Vec<NodeHit>, KgError> {
    let limit = i64::try_from(limit).map_err(|_| KgError::Argument("limit"))?;
    let (sql, key) = match by {
        FindBy::Text(text) if text.chars().count() <= LIKE_MAX_CHARS => (
            "SELECT id, kind, name, data FROM nodes WHERE name LIKE ?1 ESCAPE '\\' \
             AND (?2 IS NULL OR kind = ?2) ORDER BY length(name), id LIMIT ?3",
            escape_like(text),
        ),
        FindBy::Text(text) => (
            "SELECT n.id, n.kind, n.name, n.data FROM nodes_fts f JOIN nodes n ON n.rowid = f.rowid \
             WHERE nodes_fts MATCH ?1 AND (?2 IS NULL OR n.kind = ?2) ORDER BY f.rank, n.id LIMIT ?3",
            fts_phrase(text),
        ),
        FindBy::LawKey(law) => (
            "SELECT DISTINCT n.id, n.kind, n.name, n.data FROM refs r JOIN nodes n ON n.id = r.node_id \
             WHERE r.article_id = ?1 AND (?2 IS NULL OR n.kind = ?2) ORDER BY n.id LIMIT ?3",
            law.to_string(),
        ),
    };
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map(rusqlite::params![key, kind, limit], node_from_row)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn node_by_id(conn: &Connection, id: &str) -> Result<Option<NodeHit>, KgError> {
    Ok(conn
        .prepare_cached("SELECT id, kind, name, data FROM nodes WHERE id = ?1")?
        .query_row([id], node_from_row)
        .optional()?)
}

fn edge_from_parts(src: String, dst: String, relation: String, raw: &str, depth: u8) -> ExpandEdge {
    let data: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    let evidence = match data.get("evidence") {
        Some(Value::Array(items)) if !items.is_empty() => Evidence::Present(items.clone()),
        _ => Evidence::NoEvidence,
    };
    let certainty = data.get("certainty").filter(|v| !v.is_null()).cloned();
    ExpandEdge {
        src,
        dst,
        relation,
        depth,
        certainty,
        evidence,
        data,
    }
}

struct ExpandArgs<'a> {
    relations: &'a [String],
    direction: Direction,
    depth: u8,
    limit: usize,
}

fn expand_in(conn: &Connection, origin: &str, args: &ExpandArgs<'_>) -> Result<Expansion, KgError> {
    if node_by_id(conn, origin)?.is_none() {
        return Err(KgError::UnknownNode);
    }
    let placeholders = vec!["?"; args.relations.len()].join(", ");
    let (near, far) = match args.direction {
        Direction::Out => ("src", "dst"),
        Direction::In => ("dst", "src"),
    };
    let sql = format!(
        "SELECT src, dst, type, data FROM edges WHERE {near} = ? AND type IN ({placeholders}) ORDER BY {far}, type LIMIT ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut visited: HashSet<String> = HashSet::from([origin.to_string()]);
    let mut frontier = vec![origin.to_string()];
    let mut out = Expansion {
        origin: origin.to_string(),
        edges: Vec::new(),
        nodes: Vec::new(),
    };
    for level in 1..=args.depth {
        let mut next = Vec::new();
        for node in &frontier {
            let room = args.limit.saturating_sub(out.edges.len());
            if room == 0 {
                return Ok(out);
            }
            let mut bind: Vec<SqlValue> = Vec::with_capacity(args.relations.len() + 2);
            bind.push(SqlValue::Text(node.clone()));
            bind.extend(args.relations.iter().cloned().map(SqlValue::Text));
            bind.push(SqlValue::Integer(
                i64::try_from(room).map_err(|_| KgError::Argument("limit"))?,
            ));
            let rows = stmt.query_map(params_from_iter(bind), |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?;
            for row in rows {
                let (src, dst, relation, raw) = row?;
                let neighbour = match args.direction {
                    Direction::Out => dst.clone(),
                    Direction::In => src.clone(),
                };
                out.edges
                    .push(edge_from_parts(src, dst, relation, &raw, level));
                if visited.insert(neighbour.clone()) {
                    if let Some(hit) = node_by_id(conn, &neighbour)? {
                        out.nodes.push(hit);
                    }
                    next.push(neighbour);
                }
            }
        }
        frontier = next;
    }
    Ok(out)
}

impl KgSet {
    pub fn find(
        &self,
        graph: &str,
        by: FindBy<'_>,
        kind: Option<&str>,
        limit: usize,
    ) -> Result<Vec<NodeHit>, KgError> {
        check_limit(limit)?;
        let key = match by {
            FindBy::Text(t) | FindBy::LawKey(t) => t.trim(),
        };
        if key.is_empty() {
            return Err(KgError::Argument("text"));
        }
        let by = match by {
            FindBy::Text(_) => FindBy::Text(key),
            FindBy::LawKey(_) => FindBy::LawKey(key),
        };
        self.graph(graph)?
            .with_conn(|c| find_in(c, by, kind, limit))
    }

    pub fn expand(
        &self,
        graph: &str,
        node_id: &str,
        relation: Option<&str>,
        direction: Direction,
        depth: u8,
        limit: usize,
    ) -> Result<Expansion, KgError> {
        check_limit(limit)?;
        if depth == 0 || depth > MAX_DEPTH {
            return Err(KgError::Argument("depth"));
        }
        let g = self.graph(graph)?;
        let relations: Vec<String> = match relation {
            Some(r) if g.allowed.contains(r) => vec![r.to_string()],
            Some(_) => return Err(KgError::RelationNotAllowed),
            None => g.allowed.iter().cloned().collect(),
        };
        if relations.is_empty() {
            return Err(KgError::RelationNotAllowed);
        }
        let args = ExpandArgs {
            relations: &relations,
            direction,
            depth,
            limit,
        };
        g.with_conn(|c| expand_in(c, node_id, &args))
    }
}
