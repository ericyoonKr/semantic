use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use citegate::Mention;
use serde::{Deserialize, Serialize};

use super::known::names_a_law;

pub const WHOLE_READ_CHARS: u64 = 20_000;
pub const WHOLE_READ_LINES: u32 = 2_000;
pub const EXCERPT_CHARS: usize = 1_500;
pub const ISSUE_HEAD: &str = "쟁점:";
pub const PURPOSE_HEAD: &str = "목적:";
pub const FIRST_PURPOSE: &str = "조회";
pub const EXCERPT_HEAD: &str = "올린 문서에서 법령이 적힌 줄:";
pub const QUESTION_LAWS_HEAD: &str = "질문에 나온 법령(먼저 쟁점 조문을 찾아 읽는다):";
pub const HISTORY_HEAD: &str = "앞선 질문:";
pub const LAST_ANSWER_HEAD: &str = "직전 답 요지:";
pub const READ_HEAD: &str = "이 대화에서 이미 읽은 근거:";
pub const HISTORY_MAX: usize = 5;
pub const HISTORY_CHARS: usize = 200;
pub const LAST_ANSWER_CHARS: usize = 1500;
pub const READ_MAX: usize = 12;
const LAW_WORDS: [&str; 12] = ["법", "법령", "법률", "조례", "자치법규", "판례", "조문", "조항", "근거", "규정", "시행령", "시행규칙"];
const LAW_NAME_MIN: usize = 3;
pub const RESEARCHER: &str = "researcher";
const KEYWORD_MIN: usize = 2;
const KEEP: Duration = Duration::from_secs(24 * 3600);
const MAX_THREADS: usize = 10_000;
const STRIP: &[char] = &['?', '!', '.', ',', '·', '「', '」', '『', '』', '(', ')', '[', ']', '"', '\''];
const PARTICLES: [&str; 17] = ["이란", "인가", "인지", "에서", "으로", "란", "은", "는", "이", "가", "을", "를", "에", "의", "로", "도", "만"];

fn names_law(text: &str) -> bool {
    let quoted = text.find('「').is_some_and(|open| text[open..].contains('」'));
    quoted || citegate::mentions(text).iter().any(|m| matches!(m, Mention::Unit { law: Some(_), .. }))
}

#[derive(Debug, Clone)]
pub struct Doc {
    pub file_id: String,
    pub filename: String,
    pub chars: u64,
    pub uploaded_at: u64,
    pub processing: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "tool", content = "args", rename_all = "snake_case")]
pub enum Step {
    Model,
    ReadFile { file_path: String, limit: u32 },
    SearchDocument { query: String },
    Task { description: String, subagent_type: String },
}

#[derive(Debug, Serialize)]
pub struct StepReply {
    #[serde(flatten)]
    pub step: Step,
    pub notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct StepRequest {
    pub request_id: String,
    pub question: String,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub history: Vec<String>,
    #[serde(default)]
    pub last_answer: Option<String>,
}

pub const MUST_READ_HEAD: &str = "먼저 읽을 조문:";

fn one_line(text: &str, max: usize) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect()
}

pub fn read_labels(set: &citegate::ReadSet) -> Vec<String> {
    let parts = set.parts();
    parts.units.iter().map(|(law, unit, _)| format!("{law} {unit}")).chain(parts.cases.iter().cloned()).take(READ_MAX).collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    pub history: Vec<String>,
    pub last_answer: Option<String>,
    pub read: Vec<String>,
    pub scope: Vec<String>,
}

impl Context {
    pub fn new(req: &StepRequest, reads: &citegate::ReadSet) -> Self {
        let skip = req.history.len().saturating_sub(HISTORY_MAX);
        let history = req.history.iter().skip(skip).map(|q| one_line(q, HISTORY_CHARS)).filter(|q| !q.is_empty()).collect();
        let last_answer = req.last_answer.as_deref().map(|a| one_line(a, LAST_ANSWER_CHARS)).filter(|a| !a.is_empty());
        Context { history, last_answer, read: read_labels(reads), scope: Vec::new() }
    }

    fn lines(&self) -> String {
        let mut out = String::new();
        if !self.history.is_empty() {
            out.push_str(&format!("\n{HISTORY_HEAD}\n{}", self.history.iter().map(|q| format!("- {q}")).collect::<Vec<_>>().join("\n")));
        }
        if let Some(answer) = &self.last_answer {
            out.push_str(&format!("\n{LAST_ANSWER_HEAD} {answer}"));
        }
        if !self.read.is_empty() {
            out.push_str(&format!("\n{READ_HEAD} {}", self.read.join(", ")));
        }
        for line in &self.scope {
            out.push_str(&format!("\n{line}"));
        }
        out
    }
}

const MODIFIER_TAILS: [&str; 5] = ["는", "은", "할", "을", "던"];
const FORMAT_WORDS: [&str; 10] = ["표로", "영어로", "요약", "짧게", "길게", "다시 말해", "존댓말", "번역", "한 문장으로", "말투"];

pub fn asks_for_law(question: &str) -> bool {
    let raw: Vec<&str> = question.split_whitespace().map(|w| w.trim_matches(STRIP)).collect();
    let named = raw.iter().enumerate().any(|(i, w)| match strip_particle(w) {
        "법" => i == 0 || !MODIFIER_TAILS.iter().any(|t| raw[i - 1].ends_with(t)),
        word => LAW_WORDS.contains(&word),
    });
    named || !question_laws(question).is_empty()
}

pub fn asks_for_format(question: &str) -> bool {
    let flat = question.split_whitespace().collect::<Vec<_>>().join(" ");
    FORMAT_WORDS.iter().any(|w| flat.contains(w))
}

#[derive(Debug, Default)]
struct Progress {
    at: Option<Instant>,
    docs: HashSet<String>,
    turn: String,
    done: bool,
    reread: bool,
    legal: bool,
    cites: u32,
    flagged: Vec<String>,
}

pub const UNCITED_HEAD: &str = "근거 없이 쓴 문장(조문·판례를 찾아 붙이거나 뺀다):";

pub fn reread_brief(question: &str, unread: &[String], uncited: &[String], ctx: &Context) -> String {
    let mut out = brief(question, &[], "", ctx);
    if !unread.is_empty() {
        out.push_str(&format!("\n{MUST_READ_HEAD} {}", unread.join(", ")));
    }
    if !uncited.is_empty() {
        out.push_str(&format!("\n{UNCITED_HEAD}\n{}", uncited.iter().map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n")));
    }
    out
}

#[derive(Default)]
pub struct StepLog(Mutex<HashMap<String, Progress>>);

pub fn upload_path(file_id: &str) -> String {
    format!("/uploads/{file_id}.md")
}

fn strip_particle(word: &str) -> &str {
    PARTICLES.iter().find_map(|p| word.strip_suffix(p).filter(|rest| !rest.is_empty())).unwrap_or(word)
}

pub fn keyword(question: &str) -> Option<String> {
    question
        .split_whitespace()
        .map(|w| strip_particle(w.trim_matches(STRIP)).to_string())
        .filter(|w| w.chars().count() >= KEYWORD_MIN)
        .max_by_key(|w| w.chars().count())
}

fn sentences(text: &str) -> Vec<String> {
    let mut rest = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = Vec::new();
    while let Some(at) = citegate::cut(&rest) {
        out.push(rest[..at].trim().to_string());
        rest.drain(..at);
    }
    if !rest.trim().is_empty() {
        out.push(rest.trim().to_string());
    }
    out
}

pub fn law_lines(docs: &[Doc]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut size = 0;
    for doc in docs.iter().rev() {
        for text in sentences(&doc.text) {
            if !names_law(&text) || out.contains(&text) {
                continue;
            }
            if size + text.chars().count() > EXCERPT_CHARS {
                return out.join("\n");
            }
            size += text.chars().count();
            out.push(text);
        }
    }
    out.join("\n")
}

pub fn question_laws(question: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |name: String| {
        if !out.contains(&name) {
            out.push(name);
        }
    };
    for m in citegate::mentions(question) {
        if let Mention::Law(law) | Mention::Unit { law: Some(law), .. } = m {
            push(law);
        }
    }
    let mut outside = String::new();
    let mut depth = 0usize;
    for c in question.chars() {
        match c {
            '「' => depth += 1,
            '」' => depth = depth.saturating_sub(1),
            _ if depth == 0 => outside.push(c),
            _ => {}
        }
    }
    for word in outside.split_whitespace() {
        let word = strip_particle(word.trim_matches(STRIP));
        let word = word.strip_suffix("상").unwrap_or(word);
        if word.chars().count() >= LAW_NAME_MIN && names_a_law(word) {
            push(citegate::compact(word));
        }
    }
    out
}

pub fn brief(question: &str, laws: &[String], excerpt: &str, ctx: &Context) -> String {
    let issue = question.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = format!("{ISSUE_HEAD} {issue}\n{PURPOSE_HEAD} {FIRST_PURPOSE}{}", ctx.lines());
    if !laws.is_empty() {
        out.push_str(&format!("\n{QUESTION_LAWS_HEAD} {}", laws.join(", ")));
    }
    if !excerpt.is_empty() {
        out.push_str(&format!("\n{EXCERPT_HEAD}\n{excerpt}"));
    }
    out
}

fn readable_whole(doc: &Doc) -> bool {
    !doc.processing && doc.chars <= WHOLE_READ_CHARS
}

impl StepLog {
    pub fn after_answer(&self, thread: &str, req: &StepRequest, unread: &[String], uncited: &[String], cites: u32, ctx: &Context) -> Step {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let progress = map.entry(thread.to_string()).or_default();
        if progress.turn != req.request_id {
            progress.turn = req.request_id.clone();
            progress.done = true;
            progress.reread = false;
            progress.flagged.clear();
        }
        let _ = cites;
        if (unread.is_empty() && uncited.is_empty()) || progress.reread {
            return Step::Model;
        }
        progress.reread = true;
        progress.flagged = uncited.iter().map(|s| citegate::compact(s)).collect();
        Step::Task { description: reread_brief(&req.question, unread, uncited, ctx), subagent_type: RESEARCHER.into() }
    }

    pub fn flagged(&self, thread: &str) -> Vec<String> {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(thread).map(|p| p.flagged.clone()).unwrap_or_default()
    }

    pub fn new_turn(&self, thread: &str, request_id: &str) -> bool {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(thread).is_none_or(|p| p.turn != request_id)
    }

    pub fn is_legal(&self, thread: &str) -> bool {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(thread).is_none_or(|p| p.legal)
    }

    pub fn count_cites(&self, thread: &str, found: u32) -> u32 {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let progress = map.entry(thread.to_string()).or_default();
        progress.cites += found;
        progress.cites
    }

    pub fn next(&self, thread: &str, req: &StepRequest, legal: bool, docs: &[Doc], laws: &[String], ctx: &Context) -> Step {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if map.len() >= MAX_THREADS {
            map.retain(|_, p| p.at.is_some_and(|at| now.duration_since(at) < KEEP));
        }
        let progress = map.entry(thread.to_string()).or_default();
        progress.at = Some(now);
        if progress.turn != req.request_id {
            progress.turn = req.request_id.clone();
            progress.done = false;
            progress.reread = false;
            progress.legal = legal;
            progress.cites = 0;
            progress.flagged.clear();
        }
        if keyword(&req.question).is_none() || progress.done || !legal {
            progress.done = true;
            return Step::Model;
        }
        let mut newest: Vec<&Doc> = docs.iter().collect();
        newest.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at).then_with(|| a.file_id.cmp(&b.file_id)));
        if let Some(doc) = newest.iter().find(|d| readable_whole(d) && progress.docs.insert(d.file_id.clone())) {
            return Step::ReadFile { file_path: upload_path(&doc.file_id), limit: WHOLE_READ_LINES };
        }
        let large: Vec<String> = newest.iter().filter(|d| !readable_whole(d) && !progress.docs.contains(&d.file_id)).map(|d| d.file_id.clone()).collect();
        if !large.is_empty() {
            progress.docs.extend(large);
            return Step::SearchDocument { query: req.question.split_whitespace().collect::<Vec<_>>().join(" ") };
        }
        progress.done = true;
        Step::Task { description: brief(&req.question, laws, &law_lines(docs), ctx), subagent_type: RESEARCHER.into() }
    }
}

#[cfg(test)]
mod tests;
