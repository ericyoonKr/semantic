use checks::clock::clock_times_in;
use checks::dates::{dates_in, parse_korean_date};
use checks::periods::{Start, StartKnown};
use chrono::NaiveDate;
use docmodel::{Anchor, FactSource};
use serde::Deserialize;

use crate::deps::FactProvider;
use crate::leaf::settle::{LOCATE_FAILED, locate_in};
use crate::period::DateSource;
use crate::schema::{FailureReason, Trit};
use crate::slices::FactSlice;

pub const DATE_UNREADABLE: &str = "모델이 돌려준 날짜를 읽지 못함";
pub const DATE_NOT_IN_QUOTE: &str = "인용문에 그 날짜가 연·월·일로 적혀 있지 않음";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DateItem {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StartItem {
    pub value: Trit,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DateAnswer {
    #[serde(default)]
    pub dates: Vec<DateItem>,
    #[serde(default)]
    pub start: Option<StartItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    Date {
        date: NaiveDate,
        quote: String,
        source: DateSource,
        anchor: Anchor,
    },
    Absent,
    Failed(FailureReason),
}

fn squash(name: &str) -> String {
    name.chars().filter(|c| *c != '_' && !c.is_whitespace()).collect()
}

impl DateAnswer {
    pub fn item(&self, name: &str) -> Option<&DateItem> {
        let want = squash(name);
        self.dates.iter().find(|d| squash(&d.name) == want)
    }
}

pub fn date_in_quote(date: NaiveDate, quote: &str) -> bool {
    dates_in(quote).contains(&date)
}

pub fn start_from_quote(value: Trit, quote: &str, from: NaiveDate) -> StartKnown {
    let dates = dates_in(quote);
    if !dates.is_empty() && !dates.contains(&from) {
        return StartKnown::Undetermined;
    }
    let times = clock_times_in(quote);
    if times.is_empty() {
        return StartKnown::Undetermined;
    }
    let midnight = times.iter().all(|t| *t == (0, 0));
    let other = times.iter().all(|t| *t != (0, 0));
    match value {
        Trit::True if midnight => StartKnown::Known(Start::AtMidnight),
        Trit::False if other => StartKnown::Known(Start::AfterEvent),
        _ => StartKnown::Undetermined,
    }
}

fn source_of(anchor: &Anchor, source: &str) -> DateSource {
    match (&anchor.source, source.strip_prefix("user:")) {
        (FactSource::UserMessage { .. }, Some(id)) => DateSource::User {
            fact_id: id.trim().to_string(),
        },
        _ => DateSource::Document { page: anchor.page },
    }
}

pub async fn resolve<F: FactProvider>(
    item: Option<&DateItem>,
    facts: &F,
    session: &str,
    slice: &FactSlice,
) -> Found {
    let Some(item) = item.filter(|i| !i.date.trim().is_empty()) else {
        return Found::Absent;
    };
    let Some(date) = parse_korean_date(&item.date) else {
        return Found::Failed(FailureReason::NoEvidence(DATE_UNREADABLE.to_string()));
    };
    let quote = item.quote.trim();
    if quote.is_empty() {
        return Found::Failed(FailureReason::QuoteNotFound);
    }
    let source = item.source.trim();
    match locate_in(facts, session, slice, source, quote).await {
        Ok(Some(anchor)) if date_in_quote(date, quote) => Found::Date {
            date,
            quote: quote.to_string(),
            source: source_of(&anchor, source),
            anchor,
        },
        Ok(Some(_)) => Found::Failed(FailureReason::NoEvidence(DATE_NOT_IN_QUOTE.to_string())),
        Ok(None) => Found::Failed(FailureReason::QuoteNotFound),
        Err(error) => {
            tracing::warn!(%error, "engine.arith.locate");
            Found::Failed(FailureReason::NoEvidence(LOCATE_FAILED.to_string()))
        }
    }
}

pub async fn resolve_start<F: FactProvider>(
    item: Option<&StartItem>,
    from: NaiveDate,
    facts: &F,
    session: &str,
    slice: &FactSlice,
) -> (StartKnown, Option<String>) {
    let Some(item) = item.filter(|i| i.value != Trit::Unknown) else {
        return (StartKnown::Undetermined, None);
    };
    let quote = item.quote.trim();
    if quote.is_empty() {
        return (StartKnown::Undetermined, None);
    }
    match locate_in(facts, session, slice, item.source.trim(), quote).await {
        Ok(Some(_)) => match start_from_quote(item.value, quote, from) {
            StartKnown::Undetermined => (StartKnown::Undetermined, None),
            known => (known, Some(quote.to_string())),
        },
        _ => (StartKnown::Undetermined, None),
    }
}
