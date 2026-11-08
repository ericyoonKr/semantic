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
