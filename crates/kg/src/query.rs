use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::KgError;
use crate::fields::{self, Fields};
use crate::index;
use crate::set::KgSet;
use crate::status::{AsOf, AsOfSummary, Period, Reason, Status};

pub const MAX_LIMIT: usize = 20;
pub const MAX_DEPTH: u8 = 2;
const LIKE_MAX_CHARS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindBy<'a> {
    Text(&'a str),
    LawKey(&'a str),
    Article { law: &'a str, unit: &'a str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Out,
    In,
}

impl Direction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Direction::Out => "out",
            Direction::In => "in",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvidenceItem {
    pub field: String,
    pub item: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", content = "items", rename_all = "snake_case")]
pub enum Evidence {
    NoEvidence,
    Present(Vec<EvidenceItem>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeHit {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub gloss: Option<String>,
    pub evidence: Evidence,
    pub state: Option<Value>,
    pub period: Option<Period>,
    pub data: Value,
}

#[derive(Debug, Clone, Copy)]
pub struct FindQuery<'a> {
    pub by: FindBy<'a>,
    pub kind: Option<&'a str>,
    pub limit: usize,
    pub as_of: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Found {
    pub status: Status,
    pub reason: Option<Reason>,
    pub as_of: Option<AsOfSummary>,
    pub nodes: Vec<NodeHit>,
}

impl Found {
    pub(crate) fn empty(status: Status, reason: Reason) -> Self {
        Self {
            status,
            reason: Some(reason),
            as_of: None,
            nodes: Vec::new(),
        }
    }
}

pub(crate) fn check_limit(limit: usize) -> Result<(), KgError> {
    if limit == 0 || limit > MAX_LIMIT {
        return Err(KgError::Argument("limit"));
    }
    Ok(())
}

pub(crate) fn evidence_of(data: &Value, fields: &Fields) -> Evidence {
    let items: Vec<EvidenceItem> = fields
        .evidence
        .iter()
        .flat_map(|path| {
            let list = match fields::at(data, path) {
                Some(Value::Array(items)) => items.clone(),
                _ => Vec::new(),
            };
            list.into_iter().map(move |item| EvidenceItem {
                field: (*path).to_string(),
                item,
            })
        })
        .collect();
    if items.is_empty() {
        Evidence::NoEvidence
    } else {
        Evidence::Present(items)
    }
}

pub(crate) struct RawNode {
    id: String,
    kind: String,
    name: String,
    data: String,
}

pub(crate) fn raw_node(row: &Row<'_>) -> rusqlite::Result<RawNode> {
    Ok(RawNode {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        data: row.get(3)?,
    })
}

impl RawNode {
    pub(crate) fn into_hit(self, fields: &Fields) -> NodeHit {
        let data: Value = serde_json::from_str(&self.data).unwrap_or(Value::Null);
        let state = data.get("item_state").filter(|v| !v.is_null()).cloned();
        let gloss = fields
            .gloss
            .iter()
            .find_map(|p| fields::at(&data, p).and_then(Value::as_str))
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string);
        NodeHit {
            id: self.id,
            kind: self.kind,
            name: self.name,
            gloss,
            evidence: evidence_of(&data, fields),
            state,
            period: None,
            data,
        }
    }
}

pub(crate) fn node_by_id(
    conn: &Connection,
    id: &str,
    fields: &Fields,
) -> Result<Option<NodeHit>, KgError> {
    Ok(conn
        .prepare_cached("SELECT id, kind, name, data FROM nodes WHERE id = ?1")?
        .query_row([id], raw_node)
        .optional()?
        .map(|r| r.into_hit(fields)))
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

pub(crate) fn select_nodes(
    conn: &Connection,
    sql: &str,
    key: &str,
    q: &FindQuery<'_>,
    fields: &Fields,
) -> Result<Vec<NodeHit>, KgError> {
    let limit = match q.as_of {
        Some(_) => -1,
        None => i64::try_from(q.limit).map_err(|_| KgError::Argument("limit"))?,
    };
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map(rusqlite::params![key, q.kind, limit], raw_node)?;
    Ok(rows
        .map(|r| r.map(|raw| raw.into_hit(fields)))
        .collect::<Result<Vec<_>, _>>()?)
}

fn text_hits(
    conn: &Connection,
    text: &str,
    q: &FindQuery<'_>,
    fields: &Fields,
) -> Result<Vec<NodeHit>, KgError> {
    if text.chars().count() <= LIKE_MAX_CHARS {
        select_nodes(
            conn,
            "SELECT id, kind, name, data FROM nodes WHERE name LIKE ?1 ESCAPE '\\' \
             AND (?2 IS NULL OR kind = ?2) ORDER BY length(name), id LIMIT ?3",
            &escape_like(text),
            q,
            fields,
        )
    } else {
        select_nodes(
            conn,
            "SELECT n.id, n.kind, n.name, n.data FROM nodes_fts f JOIN nodes n ON n.rowid = f.rowid \
             WHERE nodes_fts MATCH ?1 AND (?2 IS NULL OR n.kind = ?2) ORDER BY f.rank, n.id LIMIT ?3",
            &fts_phrase(text),
            q,
            fields,
        )
    }
}

pub(crate) fn keep(
    hits: Vec<NodeHit>,
    q: &FindQuery<'_>,
    fields: &Fields,
) -> Result<Found, KgError> {
    let mut as_of = AsOf::new(q.as_of)?;
    let mut nodes = Vec::new();
    for mut hit in hits {
        if nodes.len() == q.limit {
            break;
        }
        if let Some(a) = as_of.as_mut() {
            let p = a.judge(fields::at(&hit.data, fields.period));
            if p == Period::NotInForce {
                continue;
            }
            hit.period = Some(p);
        }
        nodes.push(hit);
    }
    let excluded = as_of.as_ref().map_or(0, AsOf::excluded);
    let (status, reason) = match (nodes.is_empty(), excluded) {
        (false, _) => (Status::Found, None),
        (true, 0) => (Status::Unknown, Some(Reason::NoMatch)),
        (true, _) => (Status::Unknown, Some(Reason::ExcludedByAsOf)),
    };
    Ok(Found {
        status,
        reason,
        as_of: as_of.map(AsOf::finish),
        nodes,
    })
}

impl KgSet {
    pub fn find(&self, graph: &str, q: &FindQuery<'_>) -> Result<Found, KgError> {
        check_limit(q.limit)?;
        let by = match q.by {
            FindBy::Text(t) => FindBy::Text(t.trim()),
            FindBy::LawKey(t) => FindBy::LawKey(t.trim()),
            FindBy::Article { law, unit } => FindBy::Article {
                law: law.trim(),
                unit: unit.trim(),
            },
        };
        let empty = match by {
            FindBy::Text(t) | FindBy::LawKey(t) => t.is_empty(),
            FindBy::Article { law, unit } => law.is_empty() || unit.is_empty(),
        };
        if empty {
            return Err(KgError::Argument("text"));
        }
        let Ok(g) = self.graph(graph) else {
            return Ok(Found::empty(Status::OutOfScope, Reason::UnknownGraph));
        };
        let fields = g.fields;
        g.with_conn(|c| match by {
            FindBy::Text(text) => keep(text_hits(c, text, q, fields)?, q, fields),
            FindBy::LawKey(key) => index::by_law_key(c, key, q, fields),
            FindBy::Article { .. } => index::by_article(c),
        })
    }

    pub fn node(&self, graph: &str, id: &str) -> Result<Option<NodeHit>, KgError> {
        let g = self.graph(graph)?;
        g.with_conn(|c| node_by_id(c, id, g.fields))
    }
}
