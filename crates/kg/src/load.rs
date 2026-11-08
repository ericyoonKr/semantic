use std::io::{BufRead, BufReader, Read};

use rusqlite::Connection;
use serde_json::Value;

use crate::contract::Contract;
use crate::error::KgError;
use crate::fields::{self, Fields};
use crate::shapes::{self, Item};
use crate::store;
use crate::texts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Counts {
    pub nodes: u64,
    pub edges: u64,
    pub states: u64,
    pub dated: u64,
}

#[derive(Debug)]
pub(crate) struct Loaded {
    pub head: Value,
    pub contract: Contract,
    pub counts: Counts,
    pub shape: String,
}

pub(crate) fn load(
    reader: impl Read,
    conn: &mut Connection,
    consumer_schema: &str,
    manifest_shape: Option<&str>,
) -> Result<Loaded, KgError> {
    let mut lines = BufReader::with_capacity(256 * 1024, reader).lines();
    let first = lines.next().ok_or(KgError::HeadMissing)??;
    let mut head = parse_head(&first)?;
    if head.get("schema").and_then(Value::as_str) != Some(consumer_schema) {
        return Err(KgError::Schema);
    }
    let texts = take_texts(&mut head)?;
    let expected = head_counts(&head)?;
    let contract =
        Contract::from_value(head.get("contract").ok_or(KgError::HeadField("contract"))?)?;
    let shape = head
        .get("shape")
        .and_then(Value::as_str)
        .or(manifest_shape)
        .ok_or(KgError::HeadField("shape"))?
        .to_string();
    let shape_reader = shapes::reader(&shape).ok_or(KgError::Shape)?;
    let fields = fields::for_shape(&shape);

    let tx = conn.transaction()?;
    let mut counts = Counts::default();
    let mut line_no: u64 = 1;
    for line in lines {
        line_no += 1;
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let item: Value = serde_json::from_str(&line).map_err(|_| KgError::Json(line_no))?;
        let Value::Object(mut map) = item else {
            return Err(KgError::UnknownItem(line_no));
        };
        if map.len() != 1 {
            return Err(KgError::UnknownItem(line_no));
        }
        let (tag, mut body) = map
            .iter_mut()
            .next()
            .map(|(k, v)| (k.clone(), v.take()))
            .ok_or(KgError::UnknownItem(line_no))?;
        texts::resolve(&mut body, &texts).map_err(|_| KgError::TextRef(line_no))?;
        for item in shape_reader.read(&tag, body, line_no)? {
            match item {
                Item::Node(body) => {
                    let has_state = put_node(&tx, &body, line_no)?;
                    counts.nodes += 1;
                    counts.states += u64::from(has_state);
                    counts.dated += u64::from(dated(&body, fields));
                }
                Item::Edge(body) => {
                    put_edge(&tx, &body, &contract, line_no)?;
                    counts.edges += 1;
                    counts.dated += u64::from(dated(&body, fields));
                }
                Item::State(body) => {
                    store::insert_state(&tx, None, &body.to_string())?;
                    counts.states += 1;
                }
            }
        }
    }
    check_count("node", expected.nodes, counts.nodes)?;
    check_count("edge", expected.edges, counts.edges)?;
    check_count("state", expected.states, counts.states)?;
    tx.commit()?;
    Ok(Loaded {
        head,
        contract,
        counts,
        shape,
    })
}

fn dated(body: &Value, fields: &Fields) -> bool {
    fields::at(body, fields.period)
        .and_then(|p| p.get("state"))
        .and_then(Value::as_str)
        == Some("known")
}

fn parse_head(line: &str) -> Result<Value, KgError> {
    let value: Value = serde_json::from_str(line).map_err(|_| KgError::HeadMissing)?;
    let Value::Object(mut map) = value else {
        return Err(KgError::HeadMissing);
    };
    if let Some(inner) = map.remove("head") {
        return match inner {
            Value::Object(_) if map.is_empty() => Ok(inner),
            _ => Err(KgError::HeadMissing),
        };
    }
    if map.contains_key("schema") {
        Ok(Value::Object(map))
    } else {
        Err(KgError::HeadMissing)
    }
}

fn take_texts(head: &mut Value) -> Result<Vec<String>, KgError> {
    let Some(obj) = head.as_object_mut() else {
        return Err(KgError::HeadMissing);
    };
    match obj.remove("texts") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .into_iter()
            .map(|v| match v {
                Value::String(s) => Ok(s),
                _ => Err(KgError::HeadField("texts")),
            })
            .collect(),
        Some(_) => Err(KgError::HeadField("texts")),
    }
}

fn head_counts(head: &Value) -> Result<Counts, KgError> {
    let counts = head.get("counts").ok_or(KgError::HeadField("counts"))?;
    let get = |k: &str| {
        counts
            .get(k)
            .and_then(Value::as_u64)
            .ok_or(KgError::HeadField("counts"))
    };
    Ok(Counts {
        nodes: get("nodes")?,
        edges: get("edges")?,
        states: get("states")?,
        dated: 0,
    })
}

fn check_count(kind: &'static str, expected: u64, actual: u64) -> Result<(), KgError> {
    if expected == actual {
        Ok(())
    } else {
        Err(KgError::Count {
            kind,
            expected,
            actual,
        })
    }
}

fn put_node(conn: &Connection, body: &Value, line_no: u64) -> Result<bool, KgError> {
    let field = |k: &'static str| {
        body.get(k)
            .and_then(Value::as_str)
            .ok_or(KgError::ItemField(line_no, k))
    };
    let id = field("id")?;
    let kind = field("kind")?;
    let name = field("label")?;
    store::insert_node(conn, id, kind, name, &body.to_string())?;
    match body.get("item_state") {
        None | Some(Value::Null) => Ok(false),
        Some(state) => {
            store::insert_state(conn, Some(id), &state.to_string())?;
            Ok(true)
        }
    }
}

fn endpoint(body: &Value, key: &'static str, line_no: u64) -> Result<String, KgError> {
    match body.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Object(o)) => o
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(KgError::ItemField(line_no, key)),
        _ => Err(KgError::ItemField(line_no, key)),
    }
}

fn put_edge(
    conn: &Connection,
    body: &Value,
    contract: &Contract,
    line_no: u64,
) -> Result<(), KgError> {
    let src = endpoint(body, "source", line_no)?;
    let dst = endpoint(body, "target", line_no)?;
    let relation = body
        .get("relation")
        .and_then(Value::as_str)
        .ok_or(KgError::ItemField(line_no, "relation"))?;
    if !contract.allowed.contains(relation) {
        return Err(KgError::Relation(line_no));
    }
    store::insert_edge(conn, &src, &dst, relation, &body.to_string())
}
