mod grammar;
mod parse;
mod resolve;

use chrono::NaiveDate;
use serde::Serialize;

pub use parse::{parse_bracket, parse_citations, parse_one};
pub use resolve::{CiteFlag, CiteStatus, CitationResolution, MistakeReason, resolve_citation};

use crate::versions::PromulNo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromulRef {
    pub date: NaiveDate,
    pub no: PromulNo,
    pub via: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Bound {
    UpperWithEffective { promul: PromulRef, effective: NaiveDate, full: bool },
    Upper { promul: PromulRef, repeal: bool },
    FullRevisionUpper { promul: PromulRef },
    Both { lower: PromulRef, upper: PromulRef, lower_full: bool, upper_full: bool },
    Lower { promul: PromulRef, full: bool, effective: Option<NaiveDate> },
    AsPromulgated { promul: PromulRef },
    Prose { promul: PromulRef, effective: NaiveDate },
    Unmarked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum CiteType {
    UpperWithEffective,
    UpperOnly,
    BothSides,
    FullRevisionUpper,
    AsPromulgated,
    Prose,
    Subordinate,
    Unmarked,
    LowerOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Citation {
    pub law_name: String,
    pub old: bool,
    pub bound: Bound,
    pub article: Option<String>,
    pub same_below: bool,
    pub inherited: bool,
    pub start: usize,
    pub end: usize,
}

impl Bound {
    pub fn promuls(&self) -> Vec<&PromulRef> {
        match self {
            Self::UpperWithEffective { promul, .. }
            | Self::Upper { promul, .. }
            | Self::FullRevisionUpper { promul }
            | Self::Lower { promul, .. }
            | Self::AsPromulgated { promul }
            | Self::Prose { promul, .. } => vec![promul],
            Self::Both { lower, upper, .. } => vec![lower, upper],
            Self::Unmarked => Vec::new(),
        }
    }
}

impl Citation {
    pub fn cite_type(&self) -> CiteType {
        match &self.bound {
            Bound::UpperWithEffective { .. } => CiteType::UpperWithEffective,
            Bound::Upper { promul, .. } if !promul.no.is_statute() => CiteType::Subordinate,
            Bound::Upper { .. } => CiteType::UpperOnly,
            Bound::Both { .. } => CiteType::BothSides,
            Bound::FullRevisionUpper { .. } => CiteType::FullRevisionUpper,
            Bound::AsPromulgated { .. } => CiteType::AsPromulgated,
            Bound::Prose { .. } => CiteType::Prose,
            Bound::Lower { .. } => CiteType::LowerOnly,
            Bound::Unmarked => CiteType::Unmarked,
        }
    }
}
