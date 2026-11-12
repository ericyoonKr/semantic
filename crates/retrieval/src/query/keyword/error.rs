use std::fmt::Display;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KeywordError {
    #[error("keyword index could not be opened at {dir}: {reason}")]
    Open { dir: PathBuf, reason: String },
    #[error("keyword index at {dir} was built with tokenizer {found:?}, this build reads {expected:?}; rebuild the index")]
    Mismatch { dir: PathBuf, found: String, expected: String },
    #[error("keyword search failed: {0}")]
    Search(String),
    #[error("keyword hit could not be read: {0}")]
    Read(String),
    #[error("keyword index could not be written: {0}")]
    Write(String),
}

pub(crate) fn open(dir: &Path) -> impl FnOnce(tantivy::TantivyError) -> KeywordError + '_ {
    move |e| KeywordError::Open { dir: dir.to_path_buf(), reason: e.to_string() }
}

pub(crate) fn mismatch(dir: &Path, found: &str, expected: &str) -> KeywordError {
    KeywordError::Mismatch { dir: dir.to_path_buf(), found: found.to_string(), expected: expected.to_string() }
}

pub(crate) fn search(e: impl Display) -> KeywordError {
    KeywordError::Search(e.to_string())
}

pub(crate) fn read(e: impl Display) -> KeywordError {
    KeywordError::Read(e.to_string())
}

pub fn write(e: impl Display) -> KeywordError {
    KeywordError::Write(e.to_string())
}

pub fn log_open_failure(error: &KeywordError) {
    tracing::warn!(%error, "keyword index is unavailable; keyword branch is skipped");
}

pub fn log_search_failure(error: &KeywordError) {
    tracing::warn!(%error, "keyword search failed; results come from the other branches");
}
