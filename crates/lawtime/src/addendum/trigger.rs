use chrono::NaiveDate;
use serde::Serialize;

use super::{Comparator, Trigger, TriggerEvent};
use crate::facts::{CaseFacts, DateSpec, EventKind, Lookup};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Unknown {
    Absent(EventKind),
    Several(EventKind),
    Straddles,
    NeedsSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Holds {
    Yes,
    No,
    Unknown(Unknown),
}

fn event_date(event: TriggerEvent, facts: &CaseFacts) -> Result<DateSpec, Unknown> {
    let kind = match event {
        TriggerEvent::Event(k) => k,
        TriggerEvent::TaxPeriodStart | TriggerEvent::TaxPeriodEnd => EventKind::TaxPeriod,
    };
    let fact = match facts.find(kind) {
        Lookup::Absent => return Err(Unknown::Absent(kind)),
        Lookup::Several => return Err(Unknown::Several(kind)),
        Lookup::One(f) => f,
    };
    match (event, fact.date) {
        (TriggerEvent::Event(_), d) => Ok(d),
        (TriggerEvent::TaxPeriodStart, DateSpec::Span { from, .. }) => Ok(DateSpec::Point(from)),
        (TriggerEvent::TaxPeriodEnd, DateSpec::Span { to: Some(t), .. }) => Ok(DateSpec::Point(t)),
        (_, DateSpec::Span { to: None, .. }) => Err(Unknown::Straddles),
        (_, d) => Ok(d),
    }
}

fn point(c: Comparator, x: NaiveDate, pivot: NaiveDate) -> bool {
    match c {
        Comparator::Before => x < pivot,
        Comparator::AtOrBefore => x <= pivot,
        Comparator::AtOrAfter => x >= pivot,
        Comparator::After => x > pivot,
        Comparator::PendingAt => false,
    }
}

pub fn holds(trigger: &Trigger, facts: &CaseFacts, pivot: NaiveDate) -> Holds {
    let date = match event_date(trigger.event, facts) {
        Ok(d) => d,
        Err(u) => return Holds::Unknown(u),
    };
    let c = trigger.comparator;
    if c == Comparator::PendingAt {
        return match date {
            DateSpec::Span { from, to } => {
                if from < pivot && to.is_none_or(|t| t >= pivot) {
                    Holds::Yes
                } else {
                    Holds::No
                }
            }
            _ => Holds::Unknown(Unknown::NeedsSpan),
        };
    }
    let lo = point(c, date.lower(), pivot);
    let hi = match date.upper() {
        Some(u) => point(c, u, pivot),
        None => matches!(c, Comparator::AtOrAfter | Comparator::After),
    };
    match (lo, hi) {
        (true, true) => Holds::Yes,
        (false, false) => Holds::No,
        _ => Holds::Unknown(Unknown::Straddles),
    }
}
