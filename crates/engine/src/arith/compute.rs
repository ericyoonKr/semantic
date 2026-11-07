use checks::periods::{Bounds, Holidays, PeriodUnit, Start, StartKnown, bounds};
use chrono::NaiveDate;

use super::extract::Found;
use crate::period::{DateSource, DateUse, PeriodCase, PeriodLength, PeriodReport, StartRule};
use crate::schema::{FailureReason, LeafResult};

pub const FROM_MISSING: &str = "기산일이 사실 자료에 없음";
pub const TO_BEFORE_FROM: &str = "제소일이 기산일보다 앞섬";
pub const NOT_COMPUTED: &str = "기간의 말일을 계산하지 못함";
pub const START_OPEN: &str = "사건이 오전 0시에 시작했는지 사실 자료로 정하지 못해 말일이 하루 달라지고, 제소일이 그 사이에 있음";
pub const HOLIDAYS_OPEN: &str = "공휴일 자료가 없는 해가 있어 말일이 공휴일로 미뤄지는지 확인하지 못함";

pub struct Dates {
    pub from: Found,
    pub to: Found,
    pub start: StartKnown,
    pub start_quote: Option<String>,
    pub today: NaiveDate,
}

fn unknown(failure: FailureReason) -> LeafResult {
    LeafResult::Unknown { failure }
}

fn record(use_: &mut DateUse, date: NaiveDate, quote: Option<String>, source: DateSource) {
    use_.date = Some(date);
    use_.quote = quote;
    use_.source = Some(source);
}

fn cases(b: &Bounds) -> Vec<PeriodCase> {
    b.cases
        .iter()
        .map(|(how, r)| PeriodCase {
            first_day_included: *how == Start::AtMidnight,
            end: r.end,
            last: r.last,
            skipped: r.skipped.clone(),
            uncovered_years: r.uncovered.iter().copied().collect(),
        })
        .collect()
}

fn open_reason(b: &Bounds, to: NaiveDate) -> String {
    let lasts: Vec<NaiveDate> = b.cases.iter().map(|(_, r)| r.last).collect();
    let by_start = lasts.len() > 1 && lasts.iter().any(|l| to <= *l) && lasts.iter().any(|l| to > *l);
    let mut why = Vec::new();
    if by_start {
        why.push(START_OPEN);
    }
    if b.hi.is_none() {
        why.push(HOLIDAYS_OPEN);
    }
    if why.is_empty() {
        why.push(NOT_COMPUTED);
    }
    why.join("; ")
}

pub fn finish(mut report: PeriodReport, dates: Dates, holidays: &Holidays) -> (LeafResult, PeriodReport) {
    let (from, from_quote, from_anchor) = match dates.from {
        Found::Date { date, quote, source, anchor } => {
            record(&mut report.from, date, Some(quote.clone()), source);
            (date, quote, anchor)
        }
        Found::Absent => return (unknown(FailureReason::NoEvidence(FROM_MISSING.to_string())), report),
        Found::Failed(f) => return (unknown(f), report),
    };
    let to = match dates.to {
        Found::Date { date, quote, source, .. } => {
            record(&mut report.to, date, Some(quote), source);
            date
        }
        Found::Absent => {
            record(&mut report.to, dates.today, None, DateSource::Today);
            dates.today
        }
        Found::Failed(f) => return (unknown(f), report),
    };
    report.start = match dates.start {
        StartKnown::Known(Start::AfterEvent) => StartRule::FirstDayExcluded,
        StartKnown::Known(Start::AtMidnight) => StartRule::FirstDayIncluded,
        StartKnown::Undetermined => StartRule::BothChecked,
    };
    report.start_quote = dates.start_quote;
    if to < from {
        return (unknown(FailureReason::NoEvidence(TO_BEFORE_FROM.to_string())), report);
    }
    let (count, unit) = match report.length {
        PeriodLength::Days(n) => (n, PeriodUnit::Days),
        PeriodLength::Years(n) => (n, PeriodUnit::Years),
    };
    let Some(b) = bounds(from, count, unit, dates.start, holidays) else {
        return (unknown(FailureReason::NoEvidence(NOT_COMPUTED.to_string())), report);
    };
    report.cases = cases(&b);
    report.lo = Some(b.lo);
    report.hi = b.hi;
    report.value = b.within(to);
    let result = match report.value {
        Some(value) => LeafResult::Judged {
            value,
            anchor: from_anchor,
            quote: from_quote,
        },
        None => unknown(FailureReason::NoEvidence(open_reason(&b, to))),
    };
    (result, report)
}
