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

const LAW_ENDINGS: [&str; 6] = ["법", "법률", "령", "규칙", "조례", "규정"];

pub fn names_a_law(word: &str) -> bool {
    LAW_ENDINGS.iter().any(|e| word.ends_with(e))
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
