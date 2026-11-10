pub const MAX_TERMS: usize = 8;
const MIN_TERM_CHARS: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terms(Vec<String>);

const EDGE_MARKS: &[char] = &[',', '.', '·', '‘', '’', '\'', '"', '“', '”', '(', ')', '「', '」', '『', '』', '[', ']', '?', '!', ':', ';'];
const ENDINGS: &[&str] = &[
    "에서는", "으로서", "으로써", "에게서", "이라는", "하는지", "인지를", "에서", "으로", "에게", "하는", "하여", "하고", "이나",
    "라는", "인지", "는지", "부터", "까지", "의", "가", "이", "을", "를", "은", "는", "에", "로", "와", "과", "도", "만",
];

pub fn stem(word: &str) -> String {
    let bare = word.trim_matches(|c: char| EDGE_MARKS.contains(&c));
    for end in ENDINGS {
        if let Some(head) = bare.strip_suffix(end)
            && head.chars().count() >= MIN_TERM_CHARS
        {
            return head.to_string();
        }
    }
    bare.to_string()
}

impl Terms {
    pub fn split(query: &str) -> Option<Self> {
        Self::split_with(query, false)
    }

    pub fn split_with(query: &str, stemmed: bool) -> Option<Self> {
        let mut out: Vec<String> = Vec::new();
        for raw in query.split_whitespace() {
            let owned = if stemmed { stem(raw) } else { raw.to_string() };
            let word = owned.as_str();
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

pub const MAX_GROUPS: usize = 28;

fn groups(n: usize, m: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut pick: Vec<usize> = (0..m).collect();
    if m == 0 || m > n {
        return out;
    }
    loop {
        out.push(pick.clone());
        let Some(i) = (0..m).rev().find(|&i| pick[i] != i + n - m) else {
            return out;
        };
        pick[i] += 1;
        for j in i + 1..m {
            pick[j] = pick[j - 1] + 1;
        }
    }
}

impl Terms {
    pub fn like_patterns(&self) -> Vec<String> {
        self.0.iter().map(|w| format!("%{}%", like_prefix(w))).collect()
    }

    pub fn contained(&self, column: &str, first: usize) -> String {
        let parts: Vec<String> = self
            .slots(first)
            .map(|i| format!("({column} LIKE ${i})::int"))
            .collect();
        format!("({})", parts.join(" + "))
    }

    pub fn at_least(&self, column: &str, first: usize, m: usize) -> Option<String> {
        let all = groups(self.0.len(), m);
        if all.is_empty() || all.len() > MAX_GROUPS {
            return None;
        }
        let parts: Vec<String> = all
            .iter()
            .map(|g| {
                let ands: Vec<String> = g.iter().map(|i| format!("{column} LIKE ${}", first + i)).collect();
                format!("({})", ands.join(" AND "))
            })
            .collect();
        Some(format!("({})", parts.join(" OR ")))
    }

    pub fn relaxations(&self) -> Vec<usize> {
        let n = self.0.len();
        let floor = n.div_ceil(2).max(1);
        (floor..=n)
            .rev()
            .take_while(|m| groups(n, *m).len() <= MAX_GROUPS)
            .collect()
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
    #[test]
    fn endings_are_cut_only_when_two_or_more_characters_remain() {
        use super::stem;
        assert_eq!(stem("행정청의"), "행정청");
        assert_eq!(stem("항고소송의"), "항고소송");
        assert_eq!(stem("대상이"), "대상");
        assert_eq!(stem("결정하는"), "결정");
        assert_eq!(stem("제1항,"), "제1항");
        assert_eq!(stem("‘분양대상자별"), "분양대상자별");
        assert_eq!(stem("대가"), "대가");
        assert_eq!(stem("방법"), "방법");
        let t = Terms::split_with("행정청의 행위가 항고소송의 대상이", true).expect("terms");
        assert_eq!(t.words(), ["행정청", "행위", "항고소송", "대상"]);
    }

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
    fn groups_list_every_combination_once() {
        assert_eq!(groups(3, 2), vec![vec![0, 1], vec![0, 2], vec![1, 2]]);
        assert_eq!(groups(2, 2), vec![vec![0, 1]]);
        assert_eq!(groups(8, 6).len(), 28);
        assert!(groups(2, 3).is_empty());
    }

    #[test]
    fn substring_fragments_and_relaxation_steps() {
        let t = Terms(vec!["가나".into(), "다_라".into(), "마바".into()]);
        assert_eq!(t.like_patterns(), vec!["%가나%", "%다\\_라%", "%마바%"]);
        assert_eq!(t.contained("b", 4), "((b LIKE $4)::int + (b LIKE $5)::int + (b LIKE $6)::int)");
        assert_eq!(
            t.at_least("b", 4, 2).as_deref(),
            Some("((b LIKE $4 AND b LIKE $5) OR (b LIKE $4 AND b LIKE $6) OR (b LIKE $5 AND b LIKE $6))")
        );
        assert_eq!(t.relaxations(), vec![3, 2]);
        let eight = Terms((0..8).map(|i| format!("낱말{i}")).collect());
        assert_eq!(eight.relaxations(), vec![8, 7, 6]);
        let one = Terms(vec!["가나".into()]);
        assert_eq!(one.relaxations(), vec![1]);
    }

    #[test]
    fn like_prefix_escapes_wildcards_and_compact_removes_spaces() {
        assert_eq!(like_prefix("a%b_c\\"), "a\\%b\\_c\\\\");
        assert_eq!(compact(" 별표 13의2 "), "별표13의2");
    }
}
