pub const MAX_TERMS: usize = 8;
const MIN_TERM_CHARS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terms(Vec<String>);

impl Terms {
    pub fn split(query: &str) -> Option<Self> {
        let mut out: Vec<String> = Vec::new();
        for word in query.split_whitespace() {
            if word.chars().count() < MIN_TERM_CHARS || out.iter().any(|w| w == word) {
                continue;
            }
            out.push(word.to_string());
            if out.len() == MAX_TERMS {
                break;
            }
        }
        if out.is_empty() {
            let whole = query.trim();
            if whole.is_empty() {
                return None;
            }
            out.push(whole.to_string());
        }
        Some(Self(out))
    }

    pub fn words(&self) -> &[String] {
        &self.0
    }

    fn slots(&self, first: usize) -> impl Iterator<Item = usize> {
        first..first + self.0.len()
    }

    pub fn matches(&self, column: &str, first: usize) -> String {
        let parts: Vec<String> = self
            .slots(first)
            .map(|i| format!("{column} %> ${i}"))
            .collect();
        format!("({})", parts.join(" OR "))
    }

    pub fn score(&self, column: &str, first: usize) -> String {
        let parts: Vec<String> = self
            .slots(first)
            .map(|i| format!("word_similarity(${i}, {column})"))
            .collect();
        format!("({})", parts.join(" + "))
    }

    pub fn first_position(&self, column: &str, first: usize) -> String {
        let parts: Vec<String> = self
            .slots(first)
            .map(|i| format!("NULLIF(strpos(lower({column}), lower(${i})), 0)"))
            .collect();
        format!("COALESCE(LEAST({}), 0)", parts.join(", "))
    }
}

pub fn like_prefix(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn compact(text: &str) -> String {
    text.split_whitespace().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_drops_short_and_repeated_words_and_caps_count() {
        let t = Terms::split("국가 배상 책임 의 국가 공무원 고의 과실 손해 배상금 직무 집행");
        let words = t.map(|t| t.words().to_vec()).unwrap_or_default();
        assert_eq!(words.len(), MAX_TERMS);
        assert_eq!(words[0], "국가");
        assert!(!words.contains(&"의".to_string()));
        assert_eq!(words.iter().filter(|w| *w == "국가").count(), 1);
    }

    #[test]
    fn split_keeps_a_short_query_whole_and_rejects_blank() {
        assert_eq!(Terms::split("법").map(|t| t.words().to_vec()), Some(vec!["법".to_string()]));
        assert_eq!(Terms::split("   "), None);
    }

    #[test]
    fn sql_fragments_number_the_placeholders_from_the_given_slot() {
        let t = Terms(vec!["가나".into(), "다라".into()]);
        assert_eq!(t.matches("s.body", 3), "(s.body %> $3 OR s.body %> $4)");
        assert_eq!(
            t.score("s.body", 3),
            "(word_similarity($3, s.body) + word_similarity($4, s.body))"
        );
        assert!(t.first_position("s.body", 3).contains("lower($4)"));
    }

    #[test]
    fn like_prefix_escapes_wildcards_and_compact_removes_spaces() {
        assert_eq!(like_prefix("a%b_c\\"), "a\\%b\\_c\\\\");
        assert_eq!(compact(" 별표 13의2 "), "별표13의2");
    }
}
