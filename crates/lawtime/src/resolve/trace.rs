use chrono::NaiveDate;
use serde::Serialize;

use crate::addendum::Outcome;
use crate::adjust::Family;
use crate::facts::{CaseKind, DateSpec, EventKind};
use crate::principles::GraphRef;
use crate::versions::{PromulNo, UnitKey};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Step {
    RefTime {
        kind: CaseKind,
        field: String,
        principle: GraphRef,
        time_id: String,
        date: DateSpec,
    },
    RefTimeAnswer {
        kind: CaseKind,
        field: String,
        source: String,
        basis: String,
        event: EventKind,
        date: DateSpec,
    },
    Addendum {
        rule_id: String,
        source: String,
        outcome: Outcome,
        reviewed: bool,
        common: GraphRef,
    },
    AddendumSkipped {
        rule_id: String,
        by: PromulNo,
        effective: NaiveDate,
    },
    Adjust {
        rule_id: String,
        label: String,
        note: String,
        refs: Vec<GraphRef>,
    },
    AdjustNone {
        family: Family,
        as_of: NaiveDate,
        note: String,
    },
    AdjustYielded {
        family: Family,
        adjust_rule_id: Option<String>,
        addendum_rule_id: String,
        note: String,
    },
    Check {
        id: String,
        note: String,
        refs: Vec<GraphRef>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Flag {
    Boundary { date: NaiveDate, note: String },
    UnreviewedRule { rule_id: String },
    FullRevisionRetired { rule_id: String, by: PromulNo, effective: NaiveDate },
    FullRevisionKept { rule_id: String, by: PromulNo, effective: NaiveDate },
    UnconstitutionalScope { unit: UnitKey, ended: Option<NaiveDate> },
    Review { rule_id: String, note: String },
    RefTimeUndetermined { note: String },
    AddendumUndetermined { rule_id: String, note: String },
    NoVersion { note: String },
    BasisUnverified { source: String, basis: String },
}

impl Flag {
    pub fn needs_expert(&self) -> bool {
        matches!(
            self,
            Self::UnreviewedRule { .. }
                | Self::FullRevisionRetired { .. }
                | Self::FullRevisionKept { .. }
                | Self::UnconstitutionalScope { .. }
                | Self::BasisUnverified { .. }
        )
    }

    pub fn is_review(&self) -> bool {
        matches!(self, Self::Review { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MissingReason {
    Absent,
    Several,
    Imprecise,
    Conflicting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StepKind {
    RefTime,
    Addendum,
    Adjust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Missing {
    pub event: EventKind,
    pub reason: MissingReason,
    pub step: StepKind,
}
