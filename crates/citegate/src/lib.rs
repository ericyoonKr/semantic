use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

pub const MAX_SENTENCE: usize = 400;
pub const NOTICE: &str = "확인하지 못한 근거를 든 문장을 뺐습니다.";

pub const UNIT_NOT_READ: &str = "unit_not_read";
pub const CASE_NOT_READ: &str = "case_not_read";
pub const PARAGRAPH_NOT_READ: &str = "paragraph_not_read";

const SAME_LAW: [&str; 5] = ["동법", "같은법", "이법", "법", "같은 법"];
const CASE_CODES: [&str; 45] = [
    "헌마", "헌바", "헌가", "헌나", "헌라", "헌사", "헌아", "구합", "구단", "가합", "가단", "가소", "고합", "고단", "고정", "고약", "카합", "카단",
    "드합", "드단", "느합", "느단", "구", "누", "두", "다", "나", "가", "도", "노", "므", "르", "스", "브", "마", "라", "카", "재", "추", "후",
    "허", "그", "아", "모", "오",
];
static NAME: &str = r"(?:「([^」\n]{1,60})」|([가-힣]*(?:법률|법)))(?:\s*(시행령|시행규칙))?(?:상|의)?\s*";
static ARTICLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"{NAME}제\s*(\d+)\s*조(?:\s*의\s*(\d+))?(?:\s*제\s*(\d+)\s*항)?")).expect("article pattern"));
static BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"제\s*(\d+)\s*조(?:\s*의\s*(\d+))?(?:\s*제\s*(\d+)\s*항)?").expect("bare article pattern"));
static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"「([^」\n]{0,58}(?:법|법률|령|규칙|조례|규정))」(?:\s*(시행령|시행규칙))?").expect("quoted law pattern"));
const CIRCLED_FIRST: u32 = 0x2460;
const CIRCLED_COUNT: u32 = 20;
static ANNEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"(?:{NAME})?(별표|서식)\s*(\d+)(?:\s*의\s*(\d+))?")).expect("annex pattern")
});
static CASE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"((?:\d{{4}}|\d{{2}})(?:{})\d{{1,7}})", CASE_CODES.join("|"))).expect("case pattern"));

static PAGE_MARK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-{3}p\d+-{3}").expect("page mark pattern"));
static FOOTNOTE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([가-힣])\d{1,3}\)([\s」]|$)").expect("footnote pattern"));

pub fn clean(text: &str) -> String {
    let unpaged = PAGE_MARK.replace_all(text, "");
    FOOTNOTE.replace_all(&unpaged, "$1$2").into_owned()
}

pub fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

const SUBORDINATE: [&str; 2] = ["시행령", "시행규칙"];

pub fn canonical(name: &str) -> String {
    let short = compact(name);
    if let Some(entry) = lawnames::lookup(&short) {
        return compact(&entry.name);
    }
    for sub in SUBORDINATE {
        if let Some(entry) = short.strip_suffix(sub).and_then(lawnames::lookup) {
            return format!("{}{sub}", compact(&entry.name));
        }
    }
    short
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Mention {
    Unit { law: Option<String>, unit: String, para: Option<u32> },
    Case(String),
}

fn law_of(caps: &regex::Captures<'_>) -> Option<String> {
    let base = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str())?;
    if SAME_LAW.contains(&compact(base).as_str()) || SAME_LAW.contains(&base) {
        return None;
    }
    Some(compact(&format!("{base}{}", caps.get(3).map(|m| m.as_str()).unwrap_or(""))))
}

fn digit_at(text: &str, at: usize) -> bool {
    text[at..].chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn digit_before(text: &str, at: usize) -> bool {
    text[..at].chars().next_back().is_some_and(|c| c.is_ascii_digit())
}

fn article_key(no: &str, branch: Option<regex::Match<'_>>) -> String {
    match branch {
        Some(b) => format!("제{no}조의{}", b.as_str()),
        None => format!("제{no}조"),
    }
}

fn bare_articles(sentence: &str, covered: &[(usize, usize)], named: &[(usize, String)], out: &mut Vec<Mention>) {
    for caps in BARE.captures_iter(sentence) {
        let (Some(whole), Some(no)) = (caps.get(0), caps.get(1)) else { continue };
        if digit_at(sentence, whole.end()) || covered.iter().any(|&(s, e)| whole.start() < e && s < whole.end()) {
            continue;
        }
        let Some((_, law)) = named.iter().filter(|(at, _)| *at <= whole.start()).max_by_key(|(at, _)| *at) else {
            continue;
        };
        let para = caps.get(3).and_then(|p| p.as_str().parse().ok());
        out.push(Mention::Unit { law: Some(law.clone()), unit: article_key(no.as_str(), caps.get(2)), para });
    }
}

pub fn mentions(sentence: &str) -> Vec<Mention> {
    let cleaned = clean(sentence);
    let sentence = cleaned.as_str();
    let mut out: Vec<Mention> = Vec::new();
    let mut covered: Vec<(usize, usize)> = Vec::new();
    let mut named: Vec<(usize, String)> = QUOTED
        .captures_iter(sentence)
        .filter_map(|c| {
            let whole = c.get(0)?;
            let sub = c.get(2).map(|m| m.as_str()).unwrap_or("");
            Some((whole.end(), compact(&format!("{}{sub}", c.get(1)?.as_str()))))
        })
        .collect();
    for caps in ARTICLE.captures_iter(sentence) {
        let (Some(no), Some(whole)) = (caps.get(4), caps.get(0)) else { continue };
        if digit_at(sentence, whole.end()) {
            continue;
        }
        covered.push((whole.start(), whole.end()));
        let law = law_of(&caps);
        if let Some(l) = &law {
            named.push((whole.end(), l.clone()));
        }
        let para = caps.get(6).and_then(|p| p.as_str().parse().ok());
        out.push(Mention::Unit { law, unit: article_key(no.as_str(), caps.get(5)), para });
    }
    bare_articles(sentence, &covered, &named, &mut out);
    for caps in ANNEX.captures_iter(sentence) {
        let (Some(kind), Some(no), Some(whole)) = (caps.get(4), caps.get(5), caps.get(0)) else { continue };
        if digit_at(sentence, whole.end()) {
            continue;
        }
        let unit = match caps.get(6) {
            Some(b) => format!("{}{}의{}", kind.as_str(), no.as_str(), b.as_str()),
            None => format!("{}{}", kind.as_str(), no.as_str()),
        };
        out.push(Mention::Unit { law: law_of(&caps), unit, para: None });
    }
    for caps in CASE.captures_iter(sentence) {
        let Some(m) = caps.get(1) else { continue };
        if digit_before(sentence, m.start()) || digit_at(sentence, m.end()) {
            continue;
        }
        out.push(Mention::Case(m.as_str().to_string()));
    }
    let mut seen = HashSet::new();
    out.retain(|m| seen.insert(m.clone()));
    out
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadParts {
    pub units: Vec<(String, String, Vec<u32>)>,
    pub cases: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadSet {
    units: HashMap<(String, String), HashSet<u32>>,
    cases: HashSet<String>,
}

impl ReadSet {
    pub fn add_unit(&mut self, law: &str, unit: &str) {
        self.units.entry((canonical(law), compact(unit))).or_default();
    }

    pub fn add_unit_body(&mut self, law: &str, unit: &str, body: &str) {
        let paras = body
            .lines()
            .filter_map(|l| l.trim_start().chars().next())
            .map(|c| c as u32)
            .filter(|c| (CIRCLED_FIRST..CIRCLED_FIRST + CIRCLED_COUNT).contains(c))
            .map(|c| c - CIRCLED_FIRST + 1);
        self.units.entry((canonical(law), compact(unit))).or_default().extend(paras);
    }

    pub fn add_case(&mut self, case_no: &str) {
        self.cases.insert(compact(case_no));
    }

    pub fn add_mentions(&mut self, text: &str) {
        for m in mentions(text) {
            match m {
                Mention::Unit { law: Some(law), unit, para } => {
                    self.units.entry((canonical(&law), unit)).or_default().extend(para);
                }
                Mention::Case(no) => self.add_case(&no),
                Mention::Unit { law: None, .. } => {}
            }
        }
    }

    pub fn parts(&self) -> ReadParts {
        let mut units: Vec<(String, String, Vec<u32>)> = self
            .units
            .iter()
            .map(|((law, unit), paras)| {
                let mut p: Vec<u32> = paras.iter().copied().collect();
                p.sort_unstable();
                (law.clone(), unit.clone(), p)
            })
            .collect();
        units.sort();
        let mut cases: Vec<String> = self.cases.iter().cloned().collect();
        cases.sort();
        ReadParts { units, cases }
    }

    pub fn from_parts(parts: ReadParts) -> Self {
        let units = parts.units.into_iter().map(|(law, unit, paras)| ((law, unit), paras.into_iter().collect())).collect();
        ReadSet { units, cases: parts.cases.into_iter().collect() }
    }

    pub fn len(&self) -> usize {
        self.units.len() + self.cases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn matching(&self, law: Option<&str>, unit: &str) -> impl Iterator<Item = &HashSet<u32>> {
        let law = law.map(canonical);
        self.units
            .iter()
            .filter(move |((l, u), _)| u == unit && law.as_deref().is_none_or(|w| l.ends_with(w) || w.ends_with(l.as_str())))
            .map(|(_, p)| p)
    }

    fn has_case(&self, case_no: &str) -> bool {
        self.cases.contains(&compact(case_no))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub ok: bool,
    pub reasons: Vec<&'static str>,
    pub unread: Vec<String>,
}

fn label(m: &Mention) -> Option<String> {
    match m {
        Mention::Unit { law: Some(law), unit, .. } => Some(format!("{law} {unit}")),
        Mention::Unit { law: None, .. } => None,
        Mention::Case(no) => Some(no.clone()),
    }
}

pub fn check(sentence: &str, reads: &ReadSet) -> Verdict {
    let mut reasons: Vec<&'static str> = Vec::new();
    let mut unread: Vec<String> = Vec::new();
    for m in mentions(sentence) {
        let missing = match &m {
            Mention::Unit { law, unit, para } => {
                let mut found = reads.matching(law.as_deref(), unit).peekable();
                if found.peek().is_none() {
                    Some(UNIT_NOT_READ)
                } else {
                    para.filter(|p| !found.any(|ps| ps.contains(p))).map(|_| PARAGRAPH_NOT_READ)
                }
            }
            Mention::Case(no) => (!reads.has_case(no)).then_some(CASE_NOT_READ),
        };
        if missing == Some(UNIT_NOT_READ) || missing == Some(CASE_NOT_READ) {
            unread.extend(label(&m).filter(|l| !unread.contains(l)));
        }
        if let Some(r) = missing.filter(|r| !reasons.contains(r)) {
            reasons.push(r);
        }
    }
    Verdict { ok: reasons.is_empty(), reasons, unread }
}

pub fn cut(buf: &str) -> Option<usize> {
    let chars: Vec<(usize, char)> = buf.char_indices().collect();
    for (i, &(at, c)) in chars.iter().enumerate() {
        if c == '\n' {
            return Some(at + 1);
        }
        let after_digit = i > 0 && chars[i - 1].1.is_ascii_digit();
        if matches!(c, '.' | '?' | '!')
            && i > 0
            && !after_digit
            && let Some(&(next_at, next)) = chars.get(i + 1)
            && (next == ' ' || next == '\t')
        {
            return Some(next_at + 1);
        }
    }
    if buf.chars().count() > MAX_SENTENCE {
        let limit = buf.char_indices().nth(MAX_SENTENCE).map(|(b, _)| b).unwrap_or(buf.len());
        return Some(buf[..limit].rfind(' ').map(|s| s + 1).filter(|&s| s > 0).unwrap_or(limit));
    }
    None
}

#[cfg(test)]
mod tests;
