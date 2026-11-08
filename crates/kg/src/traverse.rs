use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};
use serde::Serialize;
use serde_json::Value;

use crate::error::KgError;
use crate::fields;
use crate::query::{Direction, Evidence, MAX_DEPTH, NodeHit, check_limit, evidence_of, node_by_id};
use crate::set::{Graph, KgSet};
use crate::status::{AsOf, AsOfSummary, Period, Reason, Status, recorded};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExpandEdge {
    pub src: String,
    pub dst: String,
    pub relation: String,
    pub depth: u8,
    pub certainty: Option<Value>,
    pub evidence: Evidence,
    pub checks: serde_json::Map<String, Value>,
    pub period: Option<Period>,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Expansion {
    pub origin: String,
    pub status: Status,
    pub reason: Option<Reason>,
    pub basis: Vec<String>,
    pub composition_mode: Option<String>,
    pub refused_paths: u64,
    pub as_of: Option<AsOfSummary>,
    pub edges: Vec<ExpandEdge>,
    pub nodes: Vec<NodeHit>,
}

#[derive(Debug, Clone, Copy)]
pub struct ExpandQuery<'a> {
    pub relation: Option<&'a str>,
    pub direction: Direction,
    pub depth: u8,
    pub limit: usize,
    pub as_of: Option<&'a str>,
}

impl Expansion {
    fn empty(origin: &str, status: Status, reason: Reason) -> Self {
        Self {
            origin: origin.to_string(),
            status,
            reason: Some(reason),
            basis: Vec::new(),
            composition_mode: None,
            refused_paths: 0,
            as_of: None,
            edges: Vec::new(),
            nodes: Vec::new(),
        }
    }
}

struct Row {
    src: String,
    dst: String,
    relation: String,
    raw: String,
}

fn edges_from(
    conn: &Connection,
    node: &str,
    relations: &[String],
    dir: Direction,
) -> Result<Vec<Row>, KgError> {
    let (near, far) = match dir {
        Direction::Out => ("src", "dst"),
        Direction::In => ("dst", "src"),
    };
    let placeholders = vec!["?"; relations.len()].join(", ");
    let sql = format!(
        "SELECT src, dst, type, data FROM edges WHERE {near} = ? AND type IN ({placeholders}) ORDER BY {far}, type"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut bind: Vec<SqlValue> = Vec::with_capacity(relations.len() + 1);
    bind.push(SqlValue::Text(node.to_string()));
    bind.extend(relations.iter().cloned().map(SqlValue::Text));
    let rows = stmt.query_map(params_from_iter(bind), |r| {
        Ok(Row {
            src: r.get(0)?,
            dst: r.get(1)?,
            relation: r.get(2)?,
            raw: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

struct Walk<'a> {
    conn: &'a Connection,
    g: &'a Graph,
    q: &'a ExpandQuery<'a>,
    relations: &'a [String],
    as_of: Option<AsOf>,
    visited: HashSet<String>,
    refused: u64,
    out: Vec<ExpandEdge>,
    nodes: Vec<NodeHit>,
}

impl Walk<'_> {
    fn take(&mut self, row: Row, depth: u8) -> Result<Option<String>, KgError> {
        let data: Value = serde_json::from_str(&row.raw).unwrap_or(Value::Null);
        let fields = self.g.fields;
        let period = self
            .as_of
            .as_mut()
            .map(|a| a.judge(fields::at(&data, fields.period)));
        if period == Some(Period::NotInForce) {
            return Ok(None);
        }
        let far = match self.q.direction {
            Direction::Out => row.dst.clone(),
            Direction::In => row.src.clone(),
        };
        let checks = fields
            .edge_checks
            .iter()
            .filter_map(|k| fields::at(&data, k).map(|v| ((*k).to_string(), v.clone())))
            .collect();
        self.out.push(ExpandEdge {
            src: row.src,
            dst: row.dst,
            relation: row.relation,
            depth,
            certainty: data.get("certainty").filter(|v| !v.is_null()).cloned(),
            evidence: evidence_of(&data, fields),
            checks,
            period,
            data,
        });
        if self.visited.insert(far.clone())
            && let Some(hit) = node_by_id(self.conn, &far, fields)?
        {
            self.nodes.push(hit);
        }
        Ok(Some(far))
    }

    fn full(&self) -> bool {
        self.out.len() >= self.q.limit
    }

    fn first_step(&mut self, origin: &str) -> Result<Vec<(String, BTreeSet<String>)>, KgError> {
        let mut reached: Vec<(String, BTreeSet<String>)> = Vec::new();
        let mut at: HashMap<String, usize> = HashMap::new();
        for row in edges_from(self.conn, origin, self.relations, self.q.direction)? {
            if self.full() {
                break;
            }
            let relation = row.relation.clone();
            if let Some(far) = self.take(row, 1)? {
                let i = *at.entry(far.clone()).or_insert_with(|| {
                    reached.push((far, BTreeSet::new()));
                    reached.len() - 1
                });
                if let Some((_, set)) = reached.get_mut(i) {
                    set.insert(relation);
                }
            }
        }
        Ok(reached)
    }

    fn second_step(&mut self, mid: &str, firsts: &BTreeSet<String>) -> Result<(), KgError> {
        let Some(kind) = node_by_id(self.conn, mid, self.g.fields)?.map(|n| n.kind) else {
            return Ok(());
        };
        for row in edges_from(self.conn, mid, self.relations, self.q.direction)? {
            let allowed = firsts.iter().any(|a| {
                self.g
                    .contract
                    .composes(a, &kind, &row.relation, self.q.direction)
            });
            if !allowed {
                self.refused += 1;
                continue;
            }
            if !self.full() {
                self.take(row, 2)?;
            }
        }
        Ok(())
    }
}

fn expand_in(
    conn: &Connection,
    g: &Graph,
    origin: &str,
    q: &ExpandQuery<'_>,
    relations: &[String],
) -> Result<Expansion, KgError> {
    let Some(start) = node_by_id(conn, origin, g.fields)? else {
        return Ok(Expansion::empty(
            origin,
            Status::Unknown,
            Reason::NodeNotInGraph,
        ));
    };
    let mut walk = Walk {
        conn,
        g,
        q,
        relations,
        as_of: AsOf::new(q.as_of)?,
        visited: HashSet::from([origin.to_string()]),
        refused: 0,
        out: Vec::new(),
        nodes: Vec::new(),
    };
    let reached = walk.first_step(origin)?;
    if q.depth == 2 {
        for (mid, firsts) in &reached {
            walk.second_step(mid, firsts)?;
        }
    }
    let excluded = walk.as_of.as_ref().map_or(0, AsOf::excluded);
    let (status, reason, basis) = if !walk.out.is_empty() {
        (Status::Found, None, Vec::new())
    } else if excluded > 0 {
        (Status::Unknown, Some(Reason::ExcludedByAsOf), Vec::new())
    } else {
        let (s, r, b) = recorded(&start, relations, q.direction, g.fields);
        (s, Some(r), b)
    };
    Ok(Expansion {
        origin: origin.to_string(),
        status,
        reason,
        basis,
        composition_mode: g.contract.composition_mode.clone(),
        refused_paths: walk.refused,
        as_of: walk.as_of.map(AsOf::finish),
        edges: walk.out,
        nodes: walk.nodes,
    })
}

impl KgSet {
    pub fn expand(
        &self,
        graph: &str,
        node_id: &str,
        q: &ExpandQuery<'_>,
    ) -> Result<Expansion, KgError> {
        check_limit(q.limit)?;
        if q.depth == 0 || q.depth > MAX_DEPTH {
            return Err(KgError::Argument("depth"));
        }
        AsOf::new(q.as_of)?;
        let Ok(g) = self.graph(graph) else {
            return Ok(Expansion::empty(
                node_id,
                Status::OutOfScope,
                Reason::UnknownGraph,
            ));
        };
        let relations: Vec<String> = match q.relation {
            Some(r) if g.contract.allowed.contains(r) => vec![r.to_string()],
            Some(_) => {
                return Ok(Expansion::empty(
                    node_id,
                    Status::OutOfScope,
                    Reason::RelationNotInContract,
                ));
            }
            None => g.contract.allowed.iter().cloned().collect(),
        };
        if relations.is_empty() {
            return Ok(Expansion::empty(
                node_id,
                Status::OutOfScope,
                Reason::RelationNotInContract,
            ));
        }
        if q.depth == 2 && g.contract.compositions.is_empty() {
            return Ok(Expansion::empty(
                node_id,
                Status::OutOfScope,
                Reason::NoCompositions,
            ));
        }
        g.with_conn(|c| expand_in(c, g, node_id, q, &relations))
    }
}
