use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::schema::{ArithSpec, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeriodLength {
    Days(u32),
    Years(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateSource {
    Document { page: Option<u32> },
    User { fact_id: String },
    Today,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateUse {
    pub name: String,
    pub date: Option<NaiveDate>,
    pub quote: Option<String>,
    pub source: Option<DateSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartRule {
    FirstDayExcluded,
    FirstDayIncluded,
    BothChecked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodCase {
    pub first_day_included: bool,
    pub end: NaiveDate,
    pub last: NaiveDate,
    pub skipped: Vec<NaiveDate>,
    pub uncovered_years: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HolidayNote {
    Applied,
    NotSet,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodReport {
    pub node_id: NodeId,
    pub length: PeriodLength,
    pub from: DateUse,
    pub to: DateUse,
    pub start: StartRule,
    pub start_quote: Option<String>,
    pub cases: Vec<PeriodCase>,
    pub lo: Option<NaiveDate>,
    pub hi: Option<NaiveDate>,
    pub holidays: HolidayNote,
    pub value: Option<bool>,
}

impl DateUse {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            date: None,
            quote: None,
            source: None,
        }
    }
}

impl PeriodReport {
    pub fn blank(node_id: NodeId, spec: &ArithSpec, holidays: HolidayNote) -> Self {
        let (from, to, length) = match spec {
            ArithSpec::WithinDays { from, to, days } => (from, to, PeriodLength::Days(*days)),
            ArithSpec::WithinYears { from, to, years } => (from, to, PeriodLength::Years(*years)),
        };
        Self {
            node_id,
            length,
            from: DateUse::named(from),
            to: DateUse::named(to),
            start: StartRule::BothChecked,
            start_quote: None,
            cases: Vec::new(),
            lo: None,
            hi: None,
            holidays,
            value: None,
        }
    }
}
