mod eval;
mod trigger;

use chrono::NaiveDate;
use serde::Serialize;

pub use eval::{AddendumResult, COMMON_PRINCIPLE, evaluate};
pub use trigger::{Holds, Unknown, holds};

use crate::Error;
use crate::facts::EventKind;
use crate::versions::{PromulNo, UnitKey, VersionStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RuleType {
    EffectiveDate,
    Sunset,
    Application,
    TransitionKeepOld,
    TransitionDeem,
    CrossAmend,
    Relation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TriggerEvent {
    Event(EventKind),
    TaxPeriodStart,
    TaxPeriodEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Comparator {
    Before,
    AtOrBefore,
    AtOrAfter,
    After,
    PendingAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Pivot {
    EffectiveDate,
    Date(NaiveDate),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Trigger {
    pub event: TriggerEvent,
    pub comparator: Comparator,
    pub pivot: Pivot,
    pub first_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Outcome {
    New,
    Old,
    DeemedNew,
    NoVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleSource {
    pub law_id: String,
    pub promul_no: PromulNo,
    pub promul_date: NaiveDate,
    pub addendum_article: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FullRevision {
    pub promul_no: PromulNo,
    pub effective: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AddendumRule {
    pub rule_id: String,
    pub source: RuleSource,
    pub rule_type: RuleType,
    pub targets: Vec<UnitKey>,
    pub trigger: Option<Trigger>,
    pub outcome: Outcome,
    pub scope_note: String,
    pub reviewed_by: Option<String>,
    pub original_text: String,
    pub retired_by_full_revision: Option<FullRevision>,
    pub kept_flag: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: Vec<AddendumRule>,
}

impl RuleSet {
    pub fn new(rules: Vec<AddendumRule>) -> Self {
        Self { rules }
    }

    pub fn rules(&self) -> &[AddendumRule] {
        &self.rules
    }

    pub fn for_unit(
        &self,
        unit: &UnitKey,
        store: &dyn VersionStore,
    ) -> Result<Vec<&AddendumRule>, Error> {
        let mut picked: Vec<(bool, &AddendumRule)> = Vec::new();
        for r in self.rules.iter().filter(|r| r.trigger.is_some()) {
            if r.targets.contains(unit) {
                picked.push((true, r));
            } else if r.targets.is_empty()
                && r.source.law_id == unit.law_id
                && store.by_promul(unit, &r.source.promul_no)?.is_some()
            {
                picked.push((false, r));
            }
        }
        picked.sort_by(|a, b| {
            b.0.cmp(&a.0).then(b.1.source.promul_date.cmp(&a.1.source.promul_date))
        });
        Ok(picked.into_iter().map(|(_, r)| r).collect())
    }
}
