use chrono::NaiveDate;
use serde::Serialize;

use crate::Error;
use crate::period::{prev_day, ymd};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CaseKind {
    CriminalSubstantive,
    CriminalProcedure,
    AdminSanction,
    AdminFine,
    AdminApplication,
    AdminGeneral,
    AdminLitigation,
    AdminOmission,
    CivilSubstantive,
    CivilProcedure,
    Tax,
    InternationalInheritance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EventKind {
    Act,
    ApplicationFiled,
    Disposition,
    ContractConcluded,
    Tort,
    TaxPeriod,
    TaxLiability,
    IncomeAccrued,
    ProceedingPending,
    PaymentDue,
    ProcedureAct,
    Completed,
    Death,
    Indictment,
    FirstInstanceClosed,
    FactualTrialClosed,
    Judgment,
    Finalized,
    UnconstitutionalDecision,
    PriorConstitutionalDecision,
    PrecedentChange,
    NonconformityDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DateSpec {
    Point(NaiveDate),
    Approx { from: NaiveDate, to: Option<NaiveDate> },
    Span { from: NaiveDate, to: Option<NaiveDate> },
}

impl DateSpec {
    pub fn month(year: i32, month: u32) -> Result<Self, Error> {
        let from = ymd(year, month, 1)?;
        let next = if month == 12 { ymd(year + 1, 1, 1)? } else { ymd(year, month + 1, 1)? };
        Ok(Self::Approx { from, to: Some(prev_day(next)?) })
    }

    pub fn lower(&self) -> NaiveDate {
        match *self {
            Self::Point(d) => d,
            Self::Approx { from, .. } | Self::Span { from, .. } => from,
        }
    }

    pub fn upper(&self) -> Option<NaiveDate> {
        match *self {
            Self::Point(d) => Some(d),
            Self::Approx { to, .. } | Self::Span { to, .. } => to,
        }
    }

    pub fn point(&self) -> Option<NaiveDate> {
        match *self {
            Self::Point(d) => Some(d),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceSpan {
    pub document: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fact {
    pub event: EventKind,
    pub date: DateSpec,
    pub source: Option<SourceSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProcState {
    Pending,
    Finalized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseFacts {
    pub kind: CaseKind,
    pub facts: Vec<Fact>,
    pub state: Option<ProcState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup<'a> {
    Absent,
    One(&'a Fact),
    Several,
}

impl CaseFacts {
    pub fn new(kind: CaseKind) -> Self {
        Self { kind, facts: Vec::new(), state: None }
    }

    pub fn with(mut self, event: EventKind, date: DateSpec) -> Self {
        self.facts.push(Fact { event, date, source: None });
        self
    }

    pub fn with_state(mut self, state: ProcState) -> Self {
        self.state = Some(state);
        self
    }

    pub fn find(&self, event: EventKind) -> Lookup<'_> {
        let mut hits = self.facts.iter().filter(|f| f.event == event);
        match (hits.next(), hits.next()) {
            (None, _) => Lookup::Absent,
            (Some(f), None) => Lookup::One(f),
            _ => Lookup::Several,
        }
    }
}
