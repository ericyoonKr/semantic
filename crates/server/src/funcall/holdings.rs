use std::collections::HashMap;

use retrieval::Corpus;

pub use norms::{NO_ORDINANCE_NOTE, ORDINANCE_SCOPE_LINE as SCOPE_LINE};

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
                tracing::info!(kinds = ?found.kinds, ordinances = found.has_kind("ordin"), "holdings.loaded");
                Some(found)
            }
            Err(error) => {
                tracing::warn!(%error, "holdings.load_failed");
                None
            }
        }
    }

    pub fn has_kind(&self, kind: &str) -> bool {
        self.kinds.get(kind).is_some_and(|n| *n > 0)
    }

    pub fn has_ordinances(&self) -> bool {
        self.has_kind("ordin")
    }
}

pub fn asks_ordinance(question: &str) -> bool {
    norms::kinds_in_question(question).iter().any(|k| k.kind == "ordin")
}

pub fn notes_for(question: &str, holdings: Option<&Holdings>) -> Vec<String> {
    let Some(h) = holdings else { return Vec::new() };
    norms::missing_kinds(question, |k| h.has_kind(k)).into_iter().filter_map(|k| k.missing_note.map(str::to_string)).collect()
}

pub fn scope_for(notes: &[String]) -> Vec<String> {
    norms::KINDS
        .iter()
        .filter(|k| k.missing_note.is_some_and(|n| notes.iter().any(|x| x == n)))
        .filter_map(|k| k.scope_line.map(str::to_string))
        .collect()
}

pub fn guidance_for<'a>(question: &str, kinds: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut names: Vec<&str> = norms::kinds_in_question(question).into_iter().map(|k| k.kind).collect();
    names.extend(kinds);
    norms::writing_notes(names).into_iter().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::{Holdings, NO_ORDINANCE_NOTE, SCOPE_LINE, asks_ordinance, guidance_for, notes_for, scope_for};

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

    #[test]
    fn writing_guidance_follows_the_kinds_in_the_question_and_the_laws_involved() {
        let from_question = guidance_for("학칙에 휴학 규정 있어?", []);
        assert_eq!(from_question, vec![norms::RULE_WRITING_NOTE.to_string()]);
        let from_laws = guidance_for("휴학은 어떻게 돼?", ["rule", "law"]);
        assert_eq!(from_laws, vec![norms::RULE_WRITING_NOTE.to_string()]);
        assert!(guidance_for("과태료 얼마야?", ["law", "admrul"]).is_empty());
        assert_eq!(guidance_for("강남구 조례는?", ["ordin"]), vec![norms::ORDINANCE_WRITING_NOTE.to_string()]);
    }
}
