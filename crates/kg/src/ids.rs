use std::collections::HashMap;

use serde::{Deserialize, Serialize};

const PREFIX: char = 'L';

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShortTarget {
    pub graph: String,
    pub id: String,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ShortIdsRepr {
    entries: Vec<ShortTarget>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ShortIdsRepr", into = "ShortIdsRepr")]
pub struct ShortIds {
    entries: Vec<ShortTarget>,
    index: HashMap<(String, String), usize>,
}

impl From<ShortIdsRepr> for ShortIds {
    fn from(repr: ShortIdsRepr) -> Self {
        let mut out = ShortIds::default();
        for target in repr.entries {
            out.push(target);
        }
        out
    }
}

impl From<ShortIds> for ShortIdsRepr {
    fn from(ids: ShortIds) -> Self {
        ShortIdsRepr {
            entries: ids.entries,
        }
    }
}

impl ShortIds {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, target: ShortTarget) -> usize {
        let key = (target.graph.clone(), target.id.clone());
        if let Some(&i) = self.index.get(&key) {
            if let Some(entry) = self.entries.get_mut(i) {
                entry.sha256 = target.sha256;
            }
            return i;
        }
        let i = self.entries.len();
        self.index.insert(key, i);
        self.entries.push(target);
        i
    }

    pub fn assign(&mut self, graph: &str, id: &str, sha256: &str) -> String {
        let i = self.push(ShortTarget {
            graph: graph.to_string(),
            id: id.to_string(),
            sha256: sha256.to_string(),
        });
        format!("{PREFIX}{}", i + 1)
    }

    pub fn resolve(&self, short: &str) -> Option<&ShortTarget> {
        let n: usize = short.trim().strip_prefix(PREFIX)?.parse().ok()?;
        self.entries.get(n.checked_sub(1)?)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::ShortIds;

    #[test]
    fn assigns_stable_numbers_and_round_trips() -> Result<(), serde_json::Error> {
        let mut ids = ShortIds::new();
        assert_eq!(ids.assign("g", "a", "s1"), "L1");
        assert_eq!(ids.assign("g", "b", "s1"), "L2");
        assert_eq!(ids.assign("g", "a", "s2"), "L1");
        assert_eq!(ids.assign("h", "a", "s1"), "L3");
        assert_eq!(ids.resolve("L1").map(|t| t.sha256.as_str()), Some("s2"));
        let old: ShortIds = serde_json::from_str(r#"{"entries":[{"graph":"g","id":"a"}]}"#)?;
        assert_eq!(old.resolve("L1").map(|t| t.sha256.as_str()), Some(""));
        let back: ShortIds = serde_json::from_str(&serde_json::to_string(&ids)?)?;
        assert_eq!(back, ids);
        assert_eq!(back.resolve("L2").map(|t| t.id.as_str()), Some("b"));
        assert_eq!(back.resolve("L0"), None);
        assert_eq!(back.resolve("L9"), None);
        assert_eq!(back.resolve("X1"), None);
        Ok(())
    }
}
