mod common;

use common::{R, d, graph, road_store, road_unit, simple_rule, trig};
use lawtime::addendum::{Comparator, Outcome, Pivot, RuleSet, RuleType};
use lawtime::adjust::AdjustTable;
use lawtime::facts::{CaseFacts, CaseKind, DateSpec, EventKind};
use lawtime::resolve::{Engine, Flag, MissingReason, Resolution, Status, Step};
use lawtime::versions::PromulNo;

fn run(rules: &RuleSet, facts: &CaseFacts) -> R<Resolution> {
    let g = graph()?;
    let store = road_store()?;
    let table = AdjustTable::standard()?;
    Ok(Engine::new(&g, &store, rules, &table)?.resolve_version(facts, &road_unit(), d(2023, 1, 1)?)?)
}

fn answer_basis(r: &Resolution) -> Option<String> {
    r.trace.iter().find_map(|s| match s {
        Step::RefTimeAnswer { source, basis, .. } if source == "해답 3장 표" => Some(basis.clone()),
        _ => None,
    })
}

#[test]
fn civil_contract_date_is_reference_with_unverified_basis() -> R {
    let facts = CaseFacts::new(CaseKind::CivilSubstantive)
        .with(EventKind::ContractConcluded, DateSpec::Point(d(2020, 3, 1)?));
    let r = run(&RuleSet::default(), &facts)?;
    assert_eq!(r.status, Status::Decided);
    assert_eq!(r.versions[0].promul_no, PromulNo::statute(0));
    assert!(answer_basis(&r).is_some_and(|b| b.contains("민법 부칙 제2조") && b.contains("약관규제법 부칙 제2조(1986)")));
    assert!(r.flags.iter().any(|f| matches!(f, Flag::BasisUnverified { basis, .. } if basis.contains("근거 원문 미확인"))));
    assert!(r.needs_expert);
    Ok(())
}

#[test]
fn civil_tort_date_is_used_when_only_tort_is_given() -> R {
    let facts = CaseFacts::new(CaseKind::CivilSubstantive).with(EventKind::Tort, DateSpec::Point(d(2021, 3, 1)?));
    let r = run(&RuleSet::default(), &facts)?;
    assert_eq!(r.versions[0].promul_no, PromulNo::statute(17371));
    Ok(())
}

#[test]
fn civil_with_both_or_neither_asks_back() -> R {
    let both = CaseFacts::new(CaseKind::CivilSubstantive)
        .with(EventKind::ContractConcluded, DateSpec::Point(d(2020, 3, 1)?))
        .with(EventKind::Tort, DateSpec::Point(d(2021, 3, 1)?));
    let r = run(&RuleSet::default(), &both)?;
    assert_eq!(r.status, Status::MissingFacts);
    assert_eq!(r.missing.len(), 2);
    assert!(r.missing.iter().all(|m| m.reason == MissingReason::Conflicting));
    let none = run(&RuleSet::default(), &CaseFacts::new(CaseKind::CivilSubstantive))?;
    assert_eq!(none.status, Status::MissingFacts);
    let events: Vec<EventKind> = none.missing.iter().map(|m| m.event).collect();
    assert_eq!(events, vec![EventKind::ContractConcluded, EventKind::Tort]);
    Ok(())
}

#[test]
fn civil_still_goes_through_addendum_rules() -> R {
    let t = trig(EventKind::ContractConcluded, Comparator::AtOrAfter, Pivot::Date(d(2020, 1, 1)?));
    let rule = simple_rule("R-적용례", &road_unit(), (17371, d(2020, 6, 9)?), RuleType::Application, t, Outcome::New);
    let facts = CaseFacts::new(CaseKind::CivilSubstantive)
        .with(EventKind::ContractConcluded, DateSpec::Point(d(2020, 3, 1)?));
    let r = run(&RuleSet::new(vec![rule]), &facts)?;
    assert_eq!(r.versions[0].promul_no, PromulNo::statute(17371));
    assert!(r.trace.iter().any(|s| matches!(s, Step::Addendum { rule_id, .. } if rule_id == "R-적용례")));
    Ok(())
}

#[test]
fn omission_suit_uses_factual_trial_close() -> R {
    let facts = CaseFacts::new(CaseKind::AdminOmission)
        .with(EventKind::FactualTrialClosed, DateSpec::Point(d(2021, 1, 5)?));
    let r = run(&RuleSet::default(), &facts)?;
    assert_eq!(r.versions[0].promul_no, PromulNo::statute(17371));
    assert_eq!(answer_basis(&r).as_deref(), Some("판례·통설 정리"));
    assert!(r.needs_expert);
    Ok(())
}
