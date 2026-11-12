use std::collections::BTreeMap;

use serde_json::Value;

use crate::call::{Outcome, Status, Target};
use crate::sample::{GoldCase, GoldLaw};

#[derive(Debug, Clone)]
pub enum Gold {
    Laws(Vec<GoldLaw>),
    Cases(Vec<GoldCase>),
}

#[derive(Debug, Clone)]
pub struct Scoring {
    pub group: String,
    pub gold: Gold,
}

#[derive(Debug, Default)]
pub struct Tally {
    pub n: usize,
    pub ok: usize,
    pub not_found: usize,
    pub failed: usize,
    pub reasons: BTreeMap<String, usize>,
    pub ranks: Vec<Option<usize>>,
    pub latencies: Vec<f64>,
    pub hits: usize,
    pub non_court_label: usize,
    pub label_missing: usize,
    pub non_court_role: usize,
    pub paragraph_asked: usize,
    pub paragraph_found: usize,
}

pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '·' | 'ㆍ' | '・' | '「' | '」' | '『' | '』'))
        .collect()
}

fn digits(s: &str) -> (String, &str) {
    let d: String = s.chars().take_while(char::is_ascii_digit).collect();
    let rest = s.get(d.len()..).unwrap_or("");
    (d, rest)
}

fn article(s: &str) -> Option<String> {
    let rest = s.strip_prefix('제')?;
    let (num, rest) = digits(rest);
    if num.is_empty() {
        return None;
    }
    let rest = rest.strip_prefix('조')?;
    let mut out = format!("제{num}조");
    if let Some(r) = rest.strip_prefix('의') {
        let (branch, _) = digits(r);
        if !branch.is_empty() {
            out.push('의');
            out.push_str(&branch);
        }
    }
    Some(out)
}

pub fn unit_key(raw: &str) -> String {
    let s = norm(raw);
    if let Some(rest) = s.strip_prefix("부칙") {
        return match article(rest) {
            Some(a) => format!("부칙{a}"),
            None => "부칙".to_string(),
        };
    }
    article(&s).unwrap_or(s)
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn law_match(hit: &Value, gold: &[GoldLaw]) -> bool {
    let name = norm(text(hit, "law_name"));
    let unit = unit_key(text(hit, "unit_key"));
    gold.iter()
        .any(|g| norm(&g.law) == name && unit_key(&g.unit) == unit)
}

pub fn paragraph_key(detail: &str) -> Option<String> {
    let s = norm(detail);
    let article = article(&s)?;
    let rest = s.strip_prefix(article.as_str())?.strip_prefix('제')?;
    let (num, rest) = digits(rest);
    if num.is_empty() || !rest.starts_with('항') {
        return None;
    }
    Some(format!("{article}제{num}항"))
}

fn within(path: &str, paragraph: &str) -> bool {
    path.strip_prefix(paragraph).is_some_and(|rest| rest.is_empty() || rest.starts_with('제'))
}

fn paragraph_match(hit: &Value, gold: &[GoldLaw]) -> bool {
    let name = norm(text(hit, "law_name"));
    let unit = unit_key(text(hit, "unit_key"));
    let paths: Vec<String> = hit
        .get("matched_units")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(norm).collect())
        .unwrap_or_default();
    gold.iter().any(|g| {
        norm(&g.law) == name
            && unit_key(&g.unit) == unit
            && paragraph_key(&g.detail).is_some_and(|p| paths.iter().any(|path| within(path, &p)))
    })
}

fn case_match(hit: &Value, gold: &[GoldCase]) -> bool {
    let no = norm(text(hit, "case_no"));
    !no.is_empty() && gold.iter().any(|g| norm(&g.case_no) == no)
}

pub fn visible(o: &Outcome, exclude: &[String], k: usize) -> Vec<Value> {
    o.hits
        .iter()
        .filter(|h| o.target != Target::Cases || !exclude.contains(&norm(text(h, "case_no"))))
        .take(k)
        .cloned()
        .collect()
}

pub fn rank(hits: &[Value], gold: &Gold) -> Option<usize> {
    hits.iter()
        .position(|h| match gold {
            Gold::Laws(g) => law_match(h, g),
            Gold::Cases(g) => case_match(h, g),
        })
        .map(|i| i + 1)
}

fn label_non_court(h: &Value) -> Option<bool> {
    let keys = ["speaker", "opinion", "accepted"];
    if !keys.iter().any(|k| h.get(*k).is_some()) {
        return None;
    }
    let speaker = text(h, "speaker");
    let opinion = text(h, "opinion");
    let accepted = text(h, "accepted");
    Some(
        (!speaker.is_empty() && !matches!(speaker, "court" | "법원"))
            || matches!(opinion, "dissent" | "separate" | "반대" | "별개")
            || matches!(accepted, "rejected" | "배척"),
    )
}

pub fn add(t: &mut Tally, o: &Outcome, hits: &[Value], gold: &Gold, roles: &[String]) {
    t.n += 1;
    match o.status {
        Status::Ok => t.ok += 1,
        Status::NotFound => t.not_found += 1,
        Status::Failed => t.failed += 1,
    }
    if o.status != Status::Ok {
        let r = o.reason.clone().unwrap_or_else(|| "사유 없음".to_string());
        *t.reasons.entry(r).or_default() += 1;
    }
    if o.status != Status::Failed && o.latency_ms.is_finite() {
        t.latencies.push(o.latency_ms);
    }
    t.ranks.push(rank(hits, gold));
    if let Gold::Laws(g) = gold
        && g.iter().any(|x| paragraph_key(&x.detail).is_some())
    {
        t.paragraph_asked += 1;
        if hits.iter().any(|h| paragraph_match(h, g)) {
            t.paragraph_found += 1;
        }
    }
    if o.target != Target::Cases {
        return;
    }
    for h in hits {
        t.hits += 1;
        match label_non_court(h) {
            Some(true) => t.non_court_label += 1,
            Some(false) => {}
            None => t.label_missing += 1,
        }
        if roles.iter().any(|r| r == text(h, "section_role")) {
            t.non_court_role += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{paragraph_key, paragraph_match};
    use crate::sample::GoldLaw;
    use serde_json::json;

    fn gold(detail: &str) -> Vec<GoldLaw> {
        vec![GoldLaw { law: "정보공개법".into(), unit: "제9조".into(), detail: detail.into(), named_in_query: false }]
    }

    #[test]
    fn paragraph_key_reads_article_and_paragraph_only() {
        assert_eq!(paragraph_key("제9조 제1항 제6호").as_deref(), Some("제9조제1항"));
        assert_eq!(paragraph_key("제12조의2 제3항").as_deref(), Some("제12조의2제3항"));
        assert_eq!(paragraph_key("제9조"), None);
        assert_eq!(paragraph_key("제9조 제6호"), None);
    }

    #[test]
    fn a_hit_counts_only_when_its_matched_units_reach_the_gold_paragraph() {
        let hit = json!({"law_name": "정보공개법", "unit_key": "제9조", "matched_units": ["제9조제1항제6호", "제9조제2항"]});
        assert!(paragraph_match(&hit, &gold("제9조 제1항 제6호")));
        assert!(!paragraph_match(&hit, &gold("제9조 제3항")));
        let bare = json!({"law_name": "정보공개법", "unit_key": "제9조"});
        assert!(!paragraph_match(&bare, &gold("제9조 제1항")));
        assert!(!paragraph_match(&hit, &gold("제9조 제11항")));
        let eleventh = json!({"law_name": "정보공개법", "unit_key": "제9조", "matched_units": ["제9조제11항"]});
        assert!(!paragraph_match(&eleventh, &gold("제9조 제1항")));
    }
}
