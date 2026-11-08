use std::collections::{BTreeSet, HashMap};

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::error::KgError;
use crate::query::Direction;

#[derive(Debug, Clone, Default)]
pub(crate) struct Contract {
    pub allowed: BTreeSet<String>,
    pub acyclic: Vec<String>,
    pub node_kinds: Vec<String>,
    pub compositions: BTreeSet<(String, String, String)>,
    pub composition_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateSpec {
    pub kind: String,
    pub values: Vec<String>,
}

fn names(obj: &serde_json::Map<String, Value>, key: &str) -> Result<Vec<String>, KgError> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or(KgError::HeadField("contract"))
            })
            .collect(),
        Some(_) => Err(KgError::HeadField("contract")),
    }
}

fn triple(item: &Value) -> Result<(String, String, String), KgError> {
    let parts = item.as_array().ok_or(KgError::HeadField("contract"))?;
    let text = |i: usize| {
        parts
            .get(i)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(KgError::HeadField("contract"))
    };
    if parts.len() != 3 {
        return Err(KgError::HeadField("contract"));
    }
    Ok((text(0)?, text(1)?, text(2)?))
}

impl Contract {
    pub(crate) fn from_value(contract: &Value) -> Result<Self, KgError> {
        let obj = contract.as_object().ok_or(KgError::HeadField("contract"))?;
        let mut out = Contract::default();
        match obj.get("relations") {
            None | Some(Value::Null) => {}
            Some(Value::Object(rels)) => {
                for (name, spec) in rels {
                    out.allowed.insert(name.clone());
                    if spec.get("acyclic").and_then(Value::as_bool) == Some(true) {
                        out.acyclic.push(name.clone());
                    }
                }
            }
            Some(_) => return Err(KgError::HeadField("contract")),
        }
        out.allowed.extend(names(obj, "optional_relations")?);
        out.node_kinds = names(obj, "node_kinds")?;
        out.node_kinds.extend(names(obj, "optional_node_kinds")?);
        match obj.get("compositions") {
            None | Some(Value::Null) => {}
            Some(Value::Array(items)) => {
                for item in items {
                    let t = triple(item)?;
                    if out.allowed.contains(&t.0) && out.allowed.contains(&t.2) {
                        out.compositions.insert(t);
                    }
                }
            }
            Some(_) => return Err(KgError::HeadField("contract")),
        }
        out.composition_mode = obj
            .get("composition_mode")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(out)
    }

    pub(crate) fn composes(&self, first: &str, mid_kind: &str, next: &str, dir: Direction) -> bool {
        let key = match dir {
            Direction::Out => (first, mid_kind, next),
            Direction::In => (next, mid_kind, first),
        };
        self.compositions
            .iter()
            .any(|(a, k, b)| a == key.0 && k == key.1 && b == key.2)
    }
}

impl StateSpec {
    pub(crate) fn from_head(head: &Value) -> Option<Self> {
        let state = head.get("state")?;
        let kind = state.get("kind")?.as_str()?.to_string();
        let values = state
            .get("values")?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        Some(Self { kind, values })
    }
}

pub(crate) fn has_cycle(conn: &Connection, relation: &str) -> Result<bool, KgError> {
    let mut stmt = conn.prepare("SELECT src, dst FROM edges WHERE type = ?1")?;
    let rows = stmt.query_map([relation], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut adj: Vec<Vec<usize>> = Vec::new();
    for row in rows {
        let (src, dst) = row?;
        let s = intern(&mut index, &mut adj, src);
        let d = intern(&mut index, &mut adj, dst);
        adj[s].push(d);
    }
    Ok(cyclic(&adj))
}

fn intern(index: &mut HashMap<String, usize>, adj: &mut Vec<Vec<usize>>, key: String) -> usize {
    if let Some(i) = index.get(&key) {
        return *i;
    }
    let i = adj.len();
    index.insert(key, i);
    adj.push(Vec::new());
    i
}

fn cyclic(adj: &[Vec<usize>]) -> bool {
    let mut color = vec![0u8; adj.len()];
    for start in 0..adj.len() {
        if color[start] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        color[start] = 1;
        while let Some(top) = stack.last_mut() {
            let (node, next) = *top;
            if let Some(&child) = adj[node].get(next) {
                top.1 += 1;
                match color[child] {
                    0 => {
                        color[child] = 1;
                        stack.push((child, 0));
                    }
                    1 => return true,
                    _ => {}
                }
            } else {
                color[node] = 2;
                stack.pop();
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{Contract, cyclic};
    use crate::query::Direction;
    use serde_json::json;

    #[test]
    fn detects_cycles_only_when_present() {
        assert!(!cyclic(&[vec![1], vec![2], vec![]]));
        assert!(!cyclic(&[vec![1, 2], vec![2], vec![]]));
        assert!(cyclic(&[vec![1], vec![2], vec![0]]));
        assert!(cyclic(&[vec![0]]));
    }

    #[test]
    fn keeps_compositions_of_declared_relations_and_reverses_inbound() -> Result<(), crate::KgError>
    {
        let c = Contract::from_value(&json!({
            "relations": {"a": {}},
            "optional_relations": ["b"],
            "compositions": [["a", "k", "b"], ["a", "k", "zz"]],
            "composition_mode": "chain_only"
        }))?;
        assert_eq!(c.compositions.len(), 1);
        assert!(c.composes("a", "k", "b", Direction::Out));
        assert!(!c.composes("b", "k", "a", Direction::Out));
        assert!(c.composes("b", "k", "a", Direction::In));
        assert!(!c.composes("a", "j", "b", Direction::Out));
        assert_eq!(c.composition_mode.as_deref(), Some("chain_only"));
        assert!(Contract::from_value(&json!({"compositions": [["a", "k"]]})).is_err());
        Ok(())
    }
}
