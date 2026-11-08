mod admin;
mod criminal;
mod table;
mod tax;

use chrono::NaiveDate;
use serde::Serialize;

pub use table::{AdjustRule, AdjustTable, Doctrine, EXTRA_REFS};

use crate::Error;
use crate::facts::{CaseFacts, DateSpec, EventKind, Lookup};
use crate::principles::Graph;
use crate::resolve::{Flag, Missing, MissingReason, Step, StepKind};
use crate::versions::{UnitKey, Version, VersionStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Family {
    CriminalLighter,
    SanctionLighter,
    FineLighter,
    ApplicationChange,
    TaxPeriodChange,
}

pub struct Input<'a> {
    pub facts: &'a CaseFacts,
    pub unit: &'a UnitKey,
    pub base: Option<&'a Version>,
    pub base_date: DateSpec,
    pub as_of: NaiveDate,
    pub store: &'a dyn VersionStore,
    pub graph: &'a Graph,
}

#[derive(Debug, Clone, Default)]
pub struct AdjustResult {
    pub versions: Option<Vec<Version>>,
    pub steps: Vec<Step>,
    pub flags: Vec<Flag>,
    pub missing: Vec<Missing>,
}

impl AdjustResult {
    pub(crate) fn note(&mut self, rule: &AdjustRule, graph: &Graph, note: String, extra: &[&str]) -> Result<(), Error> {
        let refs = rule
            .refs
            .iter()
            .chain(extra.iter())
            .map(|id| graph.reference(id))
            .collect::<Result<Vec<_>, Error>>()?;
        self.steps.push(Step::Adjust {
            rule_id: rule.id.to_string(),
            label: rule.label.to_string(),
            note,
            refs,
        });
        Ok(())
    }

    pub(crate) fn review(&mut self, rule: &AdjustRule, note: String) {
        self.flags.push(Flag::Review { rule_id: rule.id.to_string(), note });
    }

    pub(crate) fn point(&mut self, facts: &CaseFacts, event: EventKind, required: bool) -> Option<NaiveDate> {
        let miss = |reason| Missing { event, reason, step: StepKind::Adjust };
        match facts.find(event) {
            Lookup::One(f) => match f.date {
                DateSpec::Point(d) => Some(d),
                _ => {
                    self.missing.push(miss(MissingReason::Imprecise));
                    None
                }
            },
            Lookup::Several => {
                self.missing.push(miss(MissingReason::Several));
                None
            }
            Lookup::Absent => {
                if required {
                    self.missing.push(miss(MissingReason::Absent));
                }
                None
            }
        }
    }
}

pub(crate) fn changes(
    store: &dyn VersionStore,
    unit: &UnitKey,
    after: NaiveDate,
    until: NaiveDate,
) -> Result<Vec<Version>, Error> {
    if until <= after {
        return Ok(Vec::new());
    }
    let next = crate::period::next_day(after)?;
    Ok(store
        .between(unit, next, Some(until))?
        .into_iter()
        .filter(|v| v.valid_period.from > after && v.valid_period.from <= until)
        .collect())
}

pub(crate) fn differs(base: Option<&Version>, other: Option<&Version>) -> bool {
    match (base, other) {
        (Some(a), Some(b)) => !a.same_row(b),
        (None, None) => false,
        _ => true,
    }
}

pub fn run(family: Family, rule: &AdjustRule, input: &Input<'_>) -> Result<AdjustResult, Error> {
    match family {
        Family::CriminalLighter => criminal::run(rule, input),
        Family::SanctionLighter => admin::sanction(rule, input),
        Family::FineLighter => admin::fine(rule, input),
        Family::ApplicationChange => admin::application(rule, input),
        Family::TaxPeriodChange => tax::run(rule, input),
    }
}
