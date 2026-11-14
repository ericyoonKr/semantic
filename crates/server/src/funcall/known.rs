use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use norms::Kind;
use retrieval::Corpus;
use tokio::sync::OnceCell;

const MAX_NAMES: usize = 20_000;

#[derive(Default)]
pub struct KnownLaws {
    names: OnceCell<HashSet<String>>,
    kinds: OnceCell<HashMap<String, Kind>>,
    cache: Mutex<HashMap<String, bool>>,
}

impl KnownLaws {
    pub fn preset(names: &[&str]) -> Self {
        let set: HashSet<String> = names.iter().map(|n| citegate::canonical(n)).collect();
        Self { names: OnceCell::new_with(Some(set)), kinds: OnceCell::new(), cache: Mutex::default() }
    }

    pub async fn load(&self, corpus: &Corpus) {
        match retrieval::query::laws::law_name_kinds(corpus.pool()).await {
            Ok(rows) => {
                let set: HashSet<String> = rows.iter().map(|(n, _)| citegate::canonical(n)).collect();
                let kinds: HashMap<String, Kind> =
                    rows.iter().filter_map(|(n, k)| Kind::from_db(k).map(|kind| (citegate::canonical(n), kind))).collect();
                tracing::info!(names = set.len(), "known_laws.loaded");
                let _ = self.names.set(set);
                let _ = self.kinds.set(kinds);
            }
            Err(error) => tracing::warn!(%error, "known_laws.load_failed"),
        }
    }

    pub fn kind_of(&self, law: &str) -> Option<Kind> {
        self.kinds.get().and_then(|m| m.get(&citegate::canonical(law)).copied())
    }

    async fn lookup(&self, corpus: Option<&Corpus>, law: &str) -> Option<bool> {
        let key = citegate::canonical(law);
        if let Some(set) = self.names.get() {
            return Some(set.contains(&key));
        }
        if let Some(hit) = self.cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return Some(*hit);
        }
        let corpus = corpus?;
        let found = match retrieval::query::laws::resolve_law(corpus.pool(), &key).await {
            Ok(row) => row.is_some(),
            Err(error) => {
                tracing::warn!(%error, "known_laws.lookup_failed");
                return None;
            }
        };
        let mut map = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= MAX_NAMES {
            map.clear();
        }
        map.insert(key, found);
        Some(found)
    }

    pub async fn in_corpus(&self, corpus: Option<&Corpus>, law: &str) -> bool {
        self.lookup(corpus, law).await.unwrap_or(true)
    }

    pub async fn surely_in_corpus(&self, corpus: Option<&Corpus>, law: &str) -> bool {
        self.lookup(corpus, law).await.unwrap_or(false)
    }
}

pub fn names_a_law(word: &str) -> bool {
    norms::names_a_norm(word)
}

pub fn law_of_label(label: &str) -> Option<&str> {
    match label.rsplit_once(' ') {
        Some((law, unit)) => (unit.starts_with('제') || unit.starts_with("별표") || unit.starts_with("서식")).then_some(law),
        None => names_a_law(label).then_some(label),
    }
}

#[cfg(test)]
mod tests {
    use super::law_of_label;

    #[test]
    fn labels_give_their_law_names() {
        assert_eq!(law_of_label("지방공무원법 제48조"), Some("지방공무원법"));
        assert_eq!(law_of_label("식품위생법시행규칙 별표23"), Some("식품위생법시행규칙"));
        assert_eq!(law_of_label("지방공무원법"), Some("지방공무원법"));
        assert_eq!(law_of_label("2024두35989"), None);
    }
}
