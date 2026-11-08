use chrono::NaiveDate;

use super::{AddendumRule, Holds, Outcome, Pivot, RuleSet, Unknown, holds};
use crate::Error;
use crate::facts::CaseFacts;
use crate::period::prev_day;
use crate::principles::Graph;
use crate::resolve::{Flag, Missing, MissingReason, Step, StepKind};
use crate::versions::{UnitKey, Version, VersionStore};

pub const COMMON_PRINCIPLE: &str = "P:경과규정우선";

#[derive(Debug, Clone, Default)]
pub struct AddendumResult {
    pub applied: bool,
    pub candidate: Option<Version>,
    pub steps: Vec<Step>,
    pub flags: Vec<Flag>,
    pub missing: Vec<Missing>,
}

fn pivot_date(
    rule: &AddendumRule,
    pivot: Pivot,
    unit: &UnitKey,
    store: &dyn VersionStore,
) -> Result<Option<NaiveDate>, Error> {
    match pivot {
        Pivot::Date(d) => Ok(Some(d)),
        Pivot::EffectiveDate => store.effective_date(unit, &rule.source.promul_no),
    }
}

fn apply(
    rule: &AddendumRule,
    unit: &UnitKey,
    cand: Option<Version>,
    store: &dyn VersionStore,
) -> Result<Option<Version>, Error> {
    let effective = store.effective_date(unit, &rule.source.promul_no)?;
    Ok(match rule.outcome {
        Outcome::DeemedNew => cand,
        Outcome::NoVersion => None,
        Outcome::New => match store.by_promul(unit, &rule.source.promul_no)? {
            Some(v) => Some(v),
            None => match effective {
                Some(e) => store.at(unit, e)?,
                None => None,
            },
        },
        Outcome::Old => match effective {
            Some(e) => store.at(unit, prev_day(e)?)?,
            None => None,
        },
    })
}

fn unknown_note(u: Unknown) -> (String, Option<Missing>) {
    let miss = |event, reason| Missing { event, reason, step: StepKind::Addendum };
    match u {
        Unknown::Absent(e) => (format!("사건 술어의 사실이 없음: {e:?}"), Some(miss(e, MissingReason::Absent))),
        Unknown::Several(e) => (format!("같은 종류 사실이 여럿: {e:?}"), Some(miss(e, MissingReason::Several))),
        Unknown::Straddles => ("사실의 날짜 구간이 기준 날짜를 걸침".to_string(), None),
        Unknown::NeedsSpan => ("존속·계속 여부는 기간으로 된 사실이 있어야 판단함".to_string(), None),
    }
}

pub fn evaluate(
    rules: &RuleSet,
    graph: &Graph,
    facts: &CaseFacts,
    unit: &UnitKey,
    cand: Option<Version>,
    store: &dyn VersionStore,
) -> Result<AddendumResult, Error> {
    let mut out = AddendumResult { candidate: cand, ..Default::default() };
    for rule in rules.for_unit(unit, store)? {
        let Some(trigger) = rule.trigger else { continue };
        let Some(pivot) = pivot_date(rule, trigger.pivot, unit, store)? else {
            out.flags.push(Flag::AddendumUndetermined {
                rule_id: rule.rule_id.clone(),
                note: "기준 날짜(시행일)를 판본 자료에서 찾지 못함".to_string(),
            });
            break;
        };
        match holds(&trigger, facts, pivot) {
            Holds::No => continue,
            Holds::Unknown(u) => {
                let (note, miss) = unknown_note(u);
                out.missing.extend(miss);
                out.flags.push(Flag::AddendumUndetermined { rule_id: rule.rule_id.clone(), note });
                break;
            }
            Holds::Yes => {}
        }
        if let Some(rev) = &rule.retired_by_full_revision {
            if !rule.kept_flag {
                out.steps.push(Step::AddendumSkipped {
                    rule_id: rule.rule_id.clone(),
                    by: rev.promul_no.clone(),
                    effective: rev.effective,
                });
                out.flags.push(Flag::FullRevisionRetired {
                    rule_id: rule.rule_id.clone(),
                    by: rev.promul_no.clone(),
                    effective: rev.effective,
                });
                continue;
            }
            out.flags.push(Flag::FullRevisionKept {
                rule_id: rule.rule_id.clone(),
                by: rev.promul_no.clone(),
                effective: rev.effective,
            });
        }
        out.candidate = apply(rule, unit, out.candidate.take(), store)?;
        out.applied = true;
        if rule.reviewed_by.is_none() {
            out.flags.push(Flag::UnreviewedRule { rule_id: rule.rule_id.clone() });
        }
        out.steps.push(Step::Addendum {
            rule_id: rule.rule_id.clone(),
            source: format!("{} {}", rule.source.promul_no, rule.source.addendum_article),
            outcome: rule.outcome,
            reviewed: rule.reviewed_by.is_some(),
            common: graph.reference(COMMON_PRINCIPLE)?,
        });
        break;
    }
    Ok(out)
}
