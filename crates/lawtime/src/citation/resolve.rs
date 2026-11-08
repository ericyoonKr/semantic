use chrono::NaiveDate;
use serde::Serialize;

use super::{Bound, Citation};
use crate::Error;
use crate::period::prev_day;
use crate::versions::{PromulNo, UnitKey, Version, VersionStore};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum MistakeReason {
    UnknownPromulgation(PromulNo),
    PromulDateMismatch { no: PromulNo, cited: NaiveDate, stored: NaiveDate },
    NoEffectiveDate(PromulNo),
    InvertedBounds { lower: NaiveDate, upper: NaiveDate },
    NoVersionForPromul(PromulNo),
    NoVersionAt(NaiveDate),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CiteStatus {
    Resolved,
    MistakeCandidate(MistakeReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CiteFlag {
    SeveralVersionsInSpan(usize),
    AssumedCurrent,
    Remapped { from: UnitKey, to: UnitKey },
    ListNotInterpreted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CitationResolution {
    pub status: CiteStatus,
    pub version: Option<Version>,
    pub base_date: Option<NaiveDate>,
    pub unit: UnitKey,
    pub flags: Vec<CiteFlag>,
}

impl CitationResolution {
    fn mistake(unit: UnitKey, flags: Vec<CiteFlag>, base_date: Option<NaiveDate>, why: MistakeReason) -> Self {
        Self { status: CiteStatus::MistakeCandidate(why), version: None, base_date, unit, flags }
    }
}

enum Step {
    Date(NaiveDate),
    Done(Option<Version>),
    Mistake(MistakeReason),
}

fn effective(store: &dyn VersionStore, unit: &UnitKey, no: &PromulNo) -> Result<Result<NaiveDate, MistakeReason>, Error> {
    Ok(store.effective_date(unit, no)?.ok_or_else(|| MistakeReason::NoEffectiveDate(no.clone())))
}

fn base_step(
    store: &dyn VersionStore,
    cite: &Citation,
    unit: &UnitKey,
    judgment_date: NaiveDate,
    flags: &mut Vec<CiteFlag>,
) -> Result<Step, Error> {
    let step = match &cite.bound {
        Bound::UpperWithEffective { effective: e, .. } => Step::Date(prev_day(*e)?),
        Bound::Upper { promul, .. } | Bound::FullRevisionUpper { promul } => {
            match effective(store, unit, &promul.no)? {
                Ok(e) => Step::Date(prev_day(e)?),
                Err(m) => Step::Mistake(m),
            }
        }
        Bound::Lower { promul, .. } | Bound::Prose { promul, .. } => match effective(store, unit, &promul.no)? {
            Ok(e) => Step::Date(e),
            Err(m) => Step::Mistake(m),
        },
        Bound::Both { lower, upper, .. } => {
            let lo = match effective(store, unit, &lower.no)? {
                Ok(d) => d,
                Err(m) => return Ok(Step::Mistake(m)),
            };
            let hi = match effective(store, unit, &upper.no)? {
                Ok(d) => prev_day(d)?,
                Err(m) => return Ok(Step::Mistake(m)),
            };
            if hi < lo {
                return Ok(Step::Mistake(MistakeReason::InvertedBounds { lower: lo, upper: hi }));
            }
            let span = store.between(unit, lo, Some(hi))?;
            if span.len() > 1 {
                flags.push(CiteFlag::SeveralVersionsInSpan(span.len()));
            }
            Step::Date(lo)
        }
        Bound::AsPromulgated { promul } => match store.by_promul(unit, &promul.no)? {
            Some(v) => Step::Done(Some(v)),
            None => Step::Mistake(MistakeReason::NoVersionForPromul(promul.no.clone())),
        },
        Bound::Unmarked => {
            flags.push(CiteFlag::AssumedCurrent);
            Step::Date(judgment_date)
        }
    };
    Ok(step)
}

pub fn resolve_citation(
    store: &dyn VersionStore,
    cite: &Citation,
    unit: &UnitKey,
    judgment_date: NaiveDate,
) -> Result<CitationResolution, Error> {
    let mut flags = Vec::new();
    if cite.list_unresolved {
        flags.push(CiteFlag::ListNotInterpreted);
    }
    let unit = match store.current_key(unit)? {
        Some(k) if &k != unit => {
            flags.push(CiteFlag::Remapped { from: unit.clone(), to: k.clone() });
            k
        }
        _ => unit.clone(),
    };
    for p in cite.bound.promuls() {
        let why = match store.promul_date(&unit.law_id, &p.no)? {
            None => Some(MistakeReason::UnknownPromulgation(p.no.clone())),
            Some(d) if d != p.date => {
                Some(MistakeReason::PromulDateMismatch { no: p.no.clone(), cited: p.date, stored: d })
            }
            Some(_) => None,
        };
        if let Some(why) = why {
            return Ok(CitationResolution::mistake(unit, flags, None, why));
        }
    }
    let t = match base_step(store, cite, &unit, judgment_date, &mut flags)? {
        Step::Mistake(m) => return Ok(CitationResolution::mistake(unit, flags, None, m)),
        Step::Done(v) => {
            return Ok(CitationResolution { status: CiteStatus::Resolved, version: v, base_date: None, unit, flags });
        }
        Step::Date(t) => t,
    };
    match store.at(&unit, t)? {
        Some(v) => Ok(CitationResolution { status: CiteStatus::Resolved, version: Some(v), base_date: Some(t), unit, flags }),
        None => Ok(CitationResolution::mistake(unit, flags, Some(t), MistakeReason::NoVersionAt(t))),
    }
}
