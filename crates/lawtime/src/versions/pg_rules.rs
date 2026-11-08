use chrono::NaiveDate;
use serde_json::Value;

use crate::addendum::{AddendumRule, Comparator, Outcome, Pivot, RuleSource, RuleType, Trigger, TriggerEvent};
use crate::facts::EventKind;
use crate::versions::{PromulNo, UnitKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gap {
    NoTrigger,
    NoTargets,
    Unreadable(String),
}

pub struct RawRule<'a> {
    pub rule_id: &'a str,
    pub law_id: &'a str,
    pub promul_no: &'a PromulNo,
    pub promul_date: Option<NaiveDate>,
    pub addendum_article: &'a str,
    pub rule_type: &'a str,
    pub trigger: Option<&'a str>,
    pub outcome: Option<&'a str>,
    pub targets: Option<&'a [String]>,
    pub scope_note: Option<&'a str>,
    pub reviewed_by: Option<&'a str>,
    pub original_text: &'a str,
    pub kept: bool,
}

fn rule_type(s: &str) -> Option<RuleType> {
    Some(match s {
        "APPLICATION" => RuleType::Application,
        "TRANSITION_KEEP_OLD" => RuleType::TransitionKeepOld,
        "TRANSITION_DEEM" => RuleType::TransitionDeem,
        _ => return None,
    })
}

fn event(s: &str) -> Option<TriggerEvent> {
    Some(match s {
        "TAX_PERIOD_START" => TriggerEvent::TaxPeriodStart,
        "TAX_PERIOD_END" => TriggerEvent::TaxPeriodEnd,
        "ACT" => TriggerEvent::Event(EventKind::Act),
        "APPLICATION_FILED" => TriggerEvent::Event(EventKind::ApplicationFiled),
        "DISPOSITION" => TriggerEvent::Event(EventKind::Disposition),
        "CONTRACT_CONCLUDED" => TriggerEvent::Event(EventKind::ContractConcluded),
        "INCOME_ACCRUED" => TriggerEvent::Event(EventKind::IncomeAccrued),
        "PROCEEDING_PENDING" => TriggerEvent::Event(EventKind::ProceedingPending),
        "PAYMENT_DUE" => TriggerEvent::Event(EventKind::PaymentDue),
        "TAX_LIABILITY" => TriggerEvent::Event(EventKind::TaxLiability),
        _ => return None,
    })
}

fn comparator(s: &str) -> Option<Comparator> {
    Some(match s {
        "<" => Comparator::Before,
        "<=" => Comparator::AtOrBefore,
        ">=" => Comparator::AtOrAfter,
        ">" => Comparator::After,
        "PENDING_AT" => Comparator::PendingAt,
        _ => return None,
    })
}

fn pivot(s: &str) -> Option<Pivot> {
    if s == "EFFECTIVE_DATE" {
        return Some(Pivot::EffectiveDate);
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().map(Pivot::Date)
}

fn outcome(s: &str) -> Option<Outcome> {
    Some(match s {
        "NEW" => Outcome::New,
        "OLD" => Outcome::Old,
        "DEEMED_NEW" => Outcome::DeemedNew,
        "NONE" => Outcome::NoVersion,
        _ => return None,
    })
}

fn unreadable(what: &str, raw: &str) -> Gap {
    Gap::Unreadable(format!("{what}: {}", raw.chars().take(120).collect::<String>()))
}

fn read_trigger(raw: &str) -> Result<Trigger, Gap> {
    let v: Value = serde_json::from_str(raw).map_err(|_| unreadable("trigger", raw))?;
    let text = |k: &str| v.get(k).and_then(Value::as_str);
    let event = text("event_type").and_then(event).ok_or_else(|| unreadable("trigger.event_type", raw))?;
    let comparator = text("comparator").and_then(comparator).ok_or_else(|| unreadable("trigger.comparator", raw))?;
    let pivot = text("pivot").and_then(pivot).ok_or_else(|| unreadable("trigger.pivot", raw))?;
    let first_only = match v.get("first_only") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(unreadable("trigger.first_only", raw)),
    };
    Ok(Trigger { event, comparator, pivot, first_only })
}

fn read_outcome(raw: &str) -> Result<Outcome, Gap> {
    let v: Value = serde_json::from_str(raw).map_err(|_| unreadable("outcome", raw))?;
    v.get("apply_version").and_then(Value::as_str).and_then(outcome).ok_or_else(|| unreadable("outcome.apply_version", raw))
}

pub fn read_rule(raw: &RawRule<'_>) -> Result<AddendumRule, Gap> {
    let kind = rule_type(raw.rule_type).ok_or_else(|| unreadable("rule_type", raw.rule_type))?;
    let trigger = raw.trigger.ok_or(Gap::NoTrigger).and_then(read_trigger)?;
    let outcome = raw.outcome.ok_or_else(|| unreadable("outcome", "")).and_then(read_outcome)?;
    let targets = raw.targets.ok_or(Gap::NoTargets)?;
    let promul_date = raw.promul_date.ok_or_else(|| unreadable("promul_date", &raw.promul_no.to_string()))?;
    Ok(AddendumRule {
        rule_id: raw.rule_id.to_string(),
        source: RuleSource {
            law_id: raw.law_id.to_string(),
            promul_no: raw.promul_no.clone(),
            promul_date,
            addendum_article: raw.addendum_article.to_string(),
        },
        rule_type: kind,
        targets: targets.iter().map(|k| UnitKey::article(raw.law_id, k)).collect(),
        trigger: Some(trigger),
        outcome,
        scope_note: raw.scope_note.unwrap_or_default().to_string(),
        reviewed_by: raw.reviewed_by.map(str::to_string),
        original_text: raw.original_text.to_string(),
        retired_by_full_revision: None,
        kept_flag: raw.kept,
    })
}
