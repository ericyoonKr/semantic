use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use citegate::Mention;
use serde::{Deserialize, Serialize};

pub const WHOLE_READ_CHARS: u64 = 20_000;
pub const WHOLE_READ_LINES: u32 = 2_000;
pub const EXCERPT_CHARS: usize = 1_500;
pub const ISSUE_HEAD: &str = "쟁점:";
pub const PURPOSE_HEAD: &str = "목적:";
pub const FIRST_PURPOSE: &str = "조회";
pub const EXCERPT_HEAD: &str = "올린 문서에서 법령이 적힌 줄:";
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
    pub chars: u64,
    pub uploaded_at: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "tool", content = "args", rename_all = "snake_case")]
pub enum Step {
    Model,
    ReadFile { file_path: String, limit: u32 },
    Grep { pattern: String, path: String, output_mode: String },
    Task { description: String, subagent_type: String },
}

#[derive(Debug, Deserialize)]
pub struct StepRequest {
    pub request_id: String,
    pub question: String,
}

#[derive(Debug, Default)]
struct Progress {
    at: Option<Instant>,
    docs: HashSet<String>,
    turn: String,
    done: bool,
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

pub fn brief(question: &str, excerpt: &str) -> String {
    let issue = question.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = format!("{ISSUE_HEAD} {issue}\n{PURPOSE_HEAD} {FIRST_PURPOSE}");
    if !excerpt.is_empty() {
        out.push_str(&format!("\n{EXCERPT_HEAD}\n{excerpt}"));
    }
    out
}

fn doc_step(doc: &Doc, word: &str) -> Step {
    let path = upload_path(&doc.file_id);
    if doc.chars <= WHOLE_READ_CHARS {
        return Step::ReadFile { file_path: path, limit: WHOLE_READ_LINES };
    }
    Step::Grep { pattern: word.to_string(), path, output_mode: "content".into() }
}

impl StepLog {
    pub fn new_turn(&self, thread: &str, request_id: &str) -> bool {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(thread).is_none_or(|p| p.turn != request_id)
    }

    pub fn next(&self, thread: &str, req: &StepRequest, legal: bool, docs: &[Doc]) -> Step {
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
        }
        let Some(word) = keyword(&req.question) else {
            progress.done = true;
            return Step::Model;
        };
        if progress.done || !legal {
            progress.done = true;
            return Step::Model;
        }
        let mut newest: Vec<&Doc> = docs.iter().collect();
        newest.sort_by(|a, b| b.uploaded_at.cmp(&a.uploaded_at).then_with(|| a.file_id.cmp(&b.file_id)));
        if let Some(doc) = newest.into_iter().find(|d| progress.docs.insert(d.file_id.clone())) {
            return doc_step(doc, &word);
        }
        progress.done = true;
        Step::Task { description: brief(&req.question, &law_lines(docs)), subagent_type: RESEARCHER.into() }
    }
}

#[cfg(test)]
mod tests;
