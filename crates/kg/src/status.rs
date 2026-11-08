use serde::Serialize;
use serde_json::Value;

use crate::error::KgError;
use crate::fields::{self, Fields};
use crate::query::{Direction, NodeHit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Found,
    NoneConfirmed,
    Unknown,
    OutOfScope,
    NotExamined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    UnknownGraph,
    RelationNotInContract,
    NoCompositions,
    NoArticleIndex,
    ArticleKeyUnresolved,
    NoMatch,
    RecordedNone,
    RecordedUnknown,
    NoRecord,
    ExcludedByAsOf,
    NodeNotInGraph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Period {
    InForce,
    NotInForce,
    NotChecked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AsOfSummary {
    pub date: String,
    pub in_force: u64,
    pub not_checked: u64,
    pub excluded: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Day(u32);

fn parse_day(text: &str) -> Option<Day> {
    let digits: String = text.chars().filter(|c| *c != '-').collect();
    if digits.len() != 8 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    let (month, day) = ((n / 100) % 100, n % 100);
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some(Day(n))
}

impl Day {
    pub(crate) fn parse(text: &str) -> Result<Self, KgError> {
        parse_day(text.trim()).ok_or(KgError::Argument("as_of"))
    }
}

pub(crate) fn period_at(period: Option<&Value>, day: Day) -> Period {
    let Some(p) = period else {
        return Period::NotChecked;
    };
    if p.get("state").and_then(Value::as_str) != Some("known") {
        return Period::NotChecked;
    }
    let bound = |k: &str| p.get(k).and_then(Value::as_str).map(parse_day);
    match (bound("start"), bound("end")) {
        (Some(Some(start)), end) => match end {
            None => in_force(start <= day),
            Some(Some(end)) => in_force(start <= day && day < end),
            Some(None) => Period::NotChecked,
        },
        _ => Period::NotChecked,
    }
}

fn in_force(yes: bool) -> Period {
    if yes {
        Period::InForce
    } else {
        Period::NotInForce
    }
}

pub(crate) struct AsOf {
    day: Day,
    summary: AsOfSummary,
}

impl AsOf {
    pub(crate) fn new(text: Option<&str>) -> Result<Option<Self>, KgError> {
        let Some(text) = text else {
            return Ok(None);
        };
        Ok(Some(Self {
            day: Day::parse(text)?,
            summary: AsOfSummary {
                date: text.trim().to_string(),
                in_force: 0,
                not_checked: 0,
                excluded: 0,
            },
        }))
    }

    pub(crate) fn judge(&mut self, period: Option<&Value>) -> Period {
        let p = period_at(period, self.day);
        match p {
            Period::InForce => self.summary.in_force += 1,
            Period::NotChecked => self.summary.not_checked += 1,
            Period::NotInForce => self.summary.excluded += 1,
        }
        p
    }

    pub(crate) fn excluded(&self) -> u64 {
        self.summary.excluded
    }

    pub(crate) fn finish(self) -> AsOfSummary {
        self.summary
    }
}

pub(crate) fn recorded(
    origin: &NodeHit,
    relations: &[String],
    dir: Direction,
    fields: &Fields,
) -> (Status, Reason, Vec<String>) {
    let records = match fields::at(&origin.data, fields.search_records) {
        Some(Value::Array(items)) => items.as_slice(),
        _ => &[],
    };
    let mut basis = Vec::new();
    let mut statuses = Vec::with_capacity(relations.len());
    for relation in relations {
        let mut status = Status::NotExamined;
        for r in records {
            let same = r.get("relation").and_then(Value::as_str) == Some(relation.as_str());
            let way = r.get("direction").and_then(Value::as_str);
            if !same || !(way == Some(dir.as_str()) || way == Some("both")) {
                continue;
            }
            let result = r.get("result").and_then(Value::as_str);
            status = match (status, result) {
                (Status::Unknown, _) | (_, Some("unknown")) => Status::Unknown,
                (_, Some("none_confirmed")) => Status::NoneConfirmed,
                (s, _) => s,
            };
            if let Some(b) = r.get("basis").and_then(Value::as_str) {
                basis.push(b.to_string());
            }
        }
        statuses.push(status);
    }
    if statuses.iter().all(|s| *s == Status::NoneConfirmed) && !statuses.is_empty() {
        (Status::NoneConfirmed, Reason::RecordedNone, basis)
    } else if statuses.contains(&Status::Unknown) {
        (Status::Unknown, Reason::RecordedUnknown, basis)
    } else {
        (Status::NotExamined, Reason::NoRecord, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::{Day, Period, period_at};
    use serde_json::json;

    #[test]
    fn known_periods_are_judged_and_others_are_not_checked() -> Result<(), crate::KgError> {
        let day = Day::parse("2024-05-01")?;
        let known = json!({"state": "known", "start": "2020-01-01", "end": "2024-05-01"});
        assert_eq!(period_at(Some(&known), day), Period::NotInForce);
        let open = json!({"state": "known", "start": "20200101", "end": null});
        assert_eq!(period_at(Some(&open), day), Period::InForce);
        let later = json!({"state": "known", "start": "2024-05-02"});
        assert_eq!(period_at(Some(&later), day), Period::NotInForce);
        let na = json!({"state": "not_applicable", "start": null, "end": null});
        assert_eq!(period_at(Some(&na), day), Period::NotChecked);
        assert_eq!(period_at(None, day), Period::NotChecked);
        let broken = json!({"state": "known", "start": "2020-13-01"});
        assert_eq!(period_at(Some(&broken), day), Period::NotChecked);
        assert!(Day::parse("2024-5-1").is_err());
        Ok(())
    }
}
