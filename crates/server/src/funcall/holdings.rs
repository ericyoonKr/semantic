use std::collections::HashMap;

use retrieval::Corpus;

pub const ORDINANCE_KINDS: [&str; 3] = ["ordin", "ordinance", "자치법규"];
pub const NO_ORDINANCE_NOTE: &str = "조례(자치법규)는 이 서비스의 자료에 없어 확인하지 못했습니다.";
pub const SCOPE_LINE: &str = "자료 범위: 자치법규(조례·규칙)는 자료에 없음. 상위 법령(법률·시행령·시행규칙·별표)과 판례에서 찾는다.";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Holdings {
    kinds: HashMap<String, i64>,
}

impl Holdings {
    pub fn from_kinds(kinds: impl IntoIterator<Item = (String, i64)>) -> Self {
        Self { kinds: kinds.into_iter().collect() }
    }

    pub async fn load(corpus: &Corpus) -> Option<Self> {
        match retrieval::query::laws::law_kinds(corpus.pool()).await {
            Ok(kinds) => {
                let found = Self::from_kinds(kinds);
                tracing::info!(kinds = ?found.kinds, ordinances = found.has_ordinances(), "holdings.loaded");
                Some(found)
            }
            Err(error) => {
                tracing::warn!(%error, "holdings.load_failed");
                None
            }
        }
    }

    pub fn has_ordinances(&self) -> bool {
        ORDINANCE_KINDS.iter().any(|k| self.kinds.get(*k).is_some_and(|n| *n > 0))
    }
}

pub fn asks_ordinance(question: &str) -> bool {
    question.contains("조례") || question.contains("자치법규")
}

pub fn notes_for(question: &str, holdings: Option<&Holdings>) -> Vec<String> {
    match holdings {
        Some(h) if asks_ordinance(question) && !h.has_ordinances() => vec![NO_ORDINANCE_NOTE.to_string()],
        _ => Vec::new(),
    }
}

pub fn scope_for(notes: &[String]) -> Vec<String> {
    if notes.iter().any(|n| n == NO_ORDINANCE_NOTE) { vec![SCOPE_LINE.to_string()] } else { Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::{Holdings, NO_ORDINANCE_NOTE, SCOPE_LINE, asks_ordinance, notes_for, scope_for};

    #[test]
    fn asking_for_an_ordinance_gets_a_note_only_when_the_corpus_has_none() {
        let without = Holdings::from_kinds([("law".to_string(), 6058), ("admrul".to_string(), 25923)]);
        let with = Holdings::from_kinds([("law".to_string(), 6058), ("ordin".to_string(), 12)]);
        assert!(asks_ordinance("강남구 무슨 조례?") && asks_ordinance("자치법규 있어?") && !asks_ordinance("무슨 법?"));
        assert_eq!(notes_for("무슨 조례?", Some(&without)), vec![NO_ORDINANCE_NOTE.to_string()]);
        assert!(notes_for("무슨 조례?", Some(&with)).is_empty());
        assert!(notes_for("무슨 법?", Some(&without)).is_empty());
        assert!(notes_for("무슨 조례?", None).is_empty());
        assert_eq!(scope_for(&[NO_ORDINANCE_NOTE.to_string()]), vec![SCOPE_LINE.to_string()]);
        assert!(scope_for(&[]).is_empty());
    }
}
