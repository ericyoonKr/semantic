use std::collections::{BTreeSet, HashMap};

use rusqlite::Connection;
use serde_json::Value;

use crate::error::KgError;

#[derive(Debug, Clone, Default)]
pub(crate) struct Contract {
    pub allowed: BTreeSet<String>,
    pub acyclic: Vec<String>,
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
        match obj.get("optional_relations") {
            None | Some(Value::Null) => {}
            Some(Value::Array(items)) => {
                for item in items {
                    let name = item.as_str().ok_or(KgError::HeadField("contract"))?;
                    out.allowed.insert(name.to_string());
                }
            }
            Some(_) => return Err(KgError::HeadField("contract")),
        }
        Ok(out)
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
    use super::cyclic;

    #[test]
    fn detects_cycles_only_when_present() {
        assert!(!cyclic(&[vec![1], vec![2], vec![]]));
        assert!(!cyclic(&[vec![1, 2], vec![2], vec![]]));
        assert!(cyclic(&[vec![1], vec![2], vec![0]]));
        assert!(cyclic(&[vec![0]]));
    }
}
