use citegate::{LAW_NOT_READ, UNIT_NOT_READ, Verdict};
use serde::{Deserialize, Serialize};

use super::docs::session_docs;
use super::known::law_of_label;
use crate::state::AppState;

pub const MAX_SENTENCES: usize = 200;

#[derive(Debug, Deserialize)]
pub struct CheckRequest {
    pub thread_id: String,
    pub sentences: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckReply {
    pub results: Vec<Verdict>,
}

#[derive(Debug, Deserialize)]
pub struct UnreadRequest {
    pub thread_id: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct UnreadReply {
    pub unread: Vec<String>,
}

pub fn split(text: &str) -> Vec<String> {
    let mut rest = text.to_string();
    let mut out = Vec::new();
    while let Some(at) = citegate::cut(&rest) {
        out.push(rest[..at].to_string());
        rest.drain(..at);
    }
    if !rest.trim().is_empty() {
        out.push(rest);
    }
    out
}

async fn doc_text(state: &AppState, thread: &str) -> String {
    let db = state.db.clone();
    let session = thread.to_string();
    let docs = tokio::task::spawn_blocking(move || session_docs(&db, &session)).await.unwrap_or_default();
    citegate::compact(&citegate::clean(&docs.iter().map(|d| d.text.as_str()).collect::<Vec<_>>().join("\n")))
}

async fn outside_corpus(state: &AppState, label: &str) -> bool {
    match law_of_label(label) {
        Some(law) => !state.known.in_corpus(state.corpus.as_ref(), law).await,
        None => false,
    }
}

async fn settle(state: &AppState, docs: &str, mut verdict: Verdict) -> Verdict {
    if verdict.ok || !verdict.reasons.iter().all(|r| *r == UNIT_NOT_READ || *r == LAW_NOT_READ) {
        return verdict;
    }
    let mut kept = Vec::new();
    for label in verdict.unread {
        let from_docs = law_of_label(&label).is_some_and(|law| docs.contains(law));
        if !(from_docs && outside_corpus(state, &label).await) {
            kept.push(label);
        }
    }
    verdict.ok = kept.is_empty();
    if verdict.ok {
        verdict.reasons.clear();
    }
    verdict.unread = kept;
    verdict
}

pub async fn check_sentences(state: &AppState, req: &CheckRequest) -> CheckReply {
    let thread = req.thread_id.trim();
    let set = state.reads.snapshot(thread);
    let verdicts: Vec<Verdict> = req.sentences.iter().map(|s| citegate::check(s, &set)).collect();
    if verdicts.iter().all(|v| v.ok) {
        return CheckReply { results: verdicts };
    }
    let docs = doc_text(state, thread).await;
    let mut results = Vec::with_capacity(verdicts.len());
    for v in verdicts {
        results.push(settle(state, &docs, v).await);
    }
    CheckReply { results }
}

pub async fn unread_in_corpus(state: &AppState, req: &UnreadRequest) -> UnreadReply {
    let thread = req.thread_id.trim();
    let set = state.reads.snapshot(thread);
    let mut unread: Vec<String> = Vec::new();
    for sentence in split(&req.text) {
        for label in citegate::check(&sentence, &set).unread {
            if unread.contains(&label) || outside_corpus(state, &label).await {
                continue;
            }
            unread.push(label);
        }
    }
    UnreadReply { unread }
}
