use citegate::Verdict;
use serde::{Deserialize, Serialize};

use super::readlog::ReadLog;

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

pub fn check_sentences(reads: &ReadLog, req: &CheckRequest) -> CheckReply {
    let set = reads.snapshot(req.thread_id.trim());
    CheckReply { results: req.sentences.iter().map(|s| citegate::check(s, &set)).collect() }
}
