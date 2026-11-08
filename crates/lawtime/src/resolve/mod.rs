mod base;
mod checks;
mod engine;
mod trace;

use serde::Serialize;

pub use engine::Engine;
pub use trace::{Flag, Missing, MissingReason, Step, StepKind};

use crate::facts::DateSpec;
use crate::versions::Version;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NotJudged {
    NoPrinciple,
    NoRegisteredTime { principle: String, times: Vec<String> },
    SeveralTimes { principle: String, times: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Status {
    Decided,
    NotJudged(NotJudged),
    MissingFacts,
    Undetermined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolution {
    pub status: Status,
    pub versions: Vec<Version>,
    pub trace: Vec<Step>,
    pub flags: Vec<Flag>,
    pub missing: Vec<Missing>,
    pub needs_expert: bool,
}

impl Resolution {
    pub(crate) fn empty(status: Status) -> Self {
        Self {
            status,
            versions: Vec::new(),
            trace: Vec::new(),
            flags: Vec::new(),
            missing: Vec::new(),
            needs_expert: false,
        }
    }

    pub(crate) fn finish(mut self) -> Self {
        self.needs_expert = self.flags.iter().any(Flag::needs_expert);
        if self.status == Status::Decided {
            if !self.missing.is_empty() {
                self.status = Status::MissingFacts;
            } else if self.flags.iter().any(|f| {
                matches!(
                    f,
                    Flag::AddendumUndetermined { .. } | Flag::RefTimeUndetermined { .. } | Flag::DataHold { .. }
                )
            }) {
                self.status = Status::Undetermined;
            }
        }
        self
    }

    pub fn with_flags(mut self, flags: Vec<Flag>) -> Self {
        for f in flags {
            if !self.flags.contains(&f) {
                self.flags.push(f);
            }
        }
        self.finish()
    }

    pub fn ref_date(&self) -> Option<DateSpec> {
        self.trace.iter().find_map(|s| match s {
            Step::RefTime { date, .. } | Step::RefTimeAnswer { date, .. } => Some(*date),
            _ => None,
        })
    }

    pub fn has_review(&self) -> bool {
        self.flags.iter().any(Flag::is_review)
    }

    pub(crate) fn push_version(&mut self, v: Version) {
        if !self.versions.iter().any(|x| x.same_row(&v)) {
            self.versions.push(v);
        }
    }
}
