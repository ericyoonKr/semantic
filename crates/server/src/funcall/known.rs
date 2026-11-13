use std::collections::HashMap;
use std::sync::Mutex;

use retrieval::Corpus;

const MAX_NAMES: usize = 20_000;

#[derive(Default)]
pub struct KnownLaws(Mutex<HashMap<String, bool>>);

impl KnownLaws {
    pub async fn in_corpus(&self, corpus: Option<&Corpus>, law: &str) -> bool {
        let key = citegate::canonical(law);
        if let Some(hit) = self.0.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return *hit;
        }
        let Some(corpus) = corpus else { return true };
        let found = match retrieval::query::laws::resolve_law(corpus.pool(), &key).await {
            Ok(row) => row.is_some(),
            Err(error) => {
                tracing::warn!(%error, "known_laws.lookup_failed");
                return true;
            }
        };
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= MAX_NAMES {
            map.clear();
        }
        map.insert(key, found);
        found
    }
}

pub fn law_of_label(label: &str) -> Option<&str> {
    let (law, unit) = label.rsplit_once(' ')?;
    (unit.starts_with('제') || unit.starts_with("별표") || unit.starts_with("서식")).then_some(law)
}
