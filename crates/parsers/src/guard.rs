mod package;

use std::time::{Duration, Instant};

use docmodel::{Block, Document};

use crate::error::ParseError;

pub(crate) use package::{Member, Package};

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct GuardLimits {
    pub max_file_bytes: u64,
    pub max_pages: u32,
    pub max_chars: u64,
    pub max_member_bytes: u64,
    pub max_total_bytes: u64,
    pub max_ratio: u64,
    pub time_limit: Option<Duration>,
}

impl Default for GuardLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 200 * MIB,
            max_pages: 500,
            max_chars: 5_000_000,
            max_member_bytes: 64 * MIB,
            max_total_bytes: 96 * MIB,
            max_ratio: 250,
            time_limit: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Deadline(Option<Instant>);

impl Deadline {
    pub(crate) fn start(limits: &GuardLimits) -> Self {
        Self(limits.time_limit.and_then(|d| Instant::now().checked_add(d)))
    }

    pub(crate) fn check(&self) -> Result<(), ParseError> {
        match self.0 {
            Some(end) if Instant::now() >= end => Err(ParseError::Timeout),
            _ => Ok(()),
        }
    }
}

pub(crate) fn check_size(bytes: &[u8], limits: &GuardLimits) -> Result<(), ParseError> {
    if bytes.len() as u64 > limits.max_file_bytes {
        return Err(ParseError::TooLarge);
    }
    Ok(())
}

pub(crate) fn check_document(doc: &Document, limits: &GuardLimits) -> Result<(), ParseError> {
    let mut chars: u64 = 0;
    let mut visible = false;
    let mut max_page: u32 = 0;
    for block in &doc.blocks {
        let (texts, pages): (Vec<&str>, Vec<Option<u32>>) = match block {
            Block::Heading { text, anchor, .. } | Block::Para { text, anchor } => {
                (vec![text.as_str()], vec![anchor.page])
            }
            Block::Table(t) => (
                t.cells.iter().map(|c| c.text.as_str()).collect(),
                t.cells.iter().map(|c| c.anchor.page).chain([t.anchor.page]).collect(),
            ),
        };
        for text in texts {
            chars += text.chars().count() as u64;
            visible |= text.chars().any(|c| !c.is_whitespace());
        }
        max_page = pages.into_iter().flatten().fold(max_page, u32::max);
    }
    if chars > limits.max_chars || max_page > limits.max_pages {
        return Err(ParseError::TooLarge);
    }
    if !visible {
        return Err(ParseError::EmptyText);
    }
    Ok(())
}
