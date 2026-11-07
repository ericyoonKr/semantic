use std::collections::BTreeSet;

use chrono::{Datelike, Days, NaiveDate};

use super::{Holidays, Start, calendar_end, first_day};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodUnit {
    Days,
    Months,
    Years,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKnown {
    Known(Start),
    Undetermined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rolled {
    pub end: NaiveDate,
    pub last: NaiveDate,
    pub skipped: Vec<NaiveDate>,
    pub uncovered: BTreeSet<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bounds {
    pub cases: Vec<(Start, Rolled)>,
    pub lo: NaiveDate,
    pub hi: Option<NaiveDate>,
}

impl Bounds {
    pub fn within(&self, filed: NaiveDate) -> Option<bool> {
        if filed <= self.lo {
            Some(true)
        } else if self.hi.is_some_and(|hi| filed > hi) {
            Some(false)
        } else {
            None
        }
    }
}

pub fn period_end(start: NaiveDate, count: u32, unit: PeriodUnit, how: Start) -> Option<NaiveDate> {
    if count == 0 {
        return None;
    }
    let first = first_day(start, how)?;
    match unit {
        PeriodUnit::Days => first.checked_add_days(Days::new(u64::from(count) - 1)),
        PeriodUnit::Months => calendar_end(first, count),
        PeriodUnit::Years => calendar_end(first, count.checked_mul(12)?),
    }
}

pub fn roll(end: NaiveDate, holidays: &Holidays) -> Option<Rolled> {
    let mut d = end;
    let mut skipped = Vec::new();
    let mut uncovered = BTreeSet::new();
    loop {
        if !holidays.covers(d.year()) {
            uncovered.insert(d.year());
        }
        if !holidays.is_closed(d) {
            break;
        }
        skipped.push(d);
        d = d.checked_add_days(Days::new(1))?;
    }
    Some(Rolled { end, last: d, skipped, uncovered })
}

pub fn bounds(start: NaiveDate, count: u32, unit: PeriodUnit, known: StartKnown, holidays: &Holidays) -> Option<Bounds> {
    let modes = match known {
        StartKnown::Known(how) => vec![how],
        StartKnown::Undetermined => vec![Start::AfterEvent, Start::AtMidnight],
    };
    let cases = modes
        .into_iter()
        .map(|how| Some((how, roll(period_end(start, count, unit, how)?, holidays)?)))
        .collect::<Option<Vec<_>>>()?;
    let lo = cases.iter().map(|(_, r)| r.last).min()?;
    let hi = if cases.iter().all(|(_, r)| r.uncovered.is_empty()) {
        cases.iter().map(|(_, r)| r.last).max()
    } else {
        None
    };
    Some(Bounds { cases, lo, hi })
}

#[cfg(test)]
#[path = "bounds_tests.rs"]
mod tests;
