mod common;

use chrono::NaiveDate;
use common::{R, animal_store, animal_unit, d, graph, road_store, road_unit, simple_rule, trig};
use lawtime::addendum::{Comparator, Outcome, Pivot, RuleSet, RuleType};
use lawtime::adjust::AdjustTable;
use lawtime::facts::{CaseFacts, CaseKind, DateSpec, EventKind};
use lawtime::resolve::{Engine, Flag, Resolution, Step};
use lawtime::versions::{MemoryStore, PromulNo, UnitKey};

fn run(store: &MemoryStore, rules: &RuleSet, facts: &CaseFacts, unit: &UnitKey, as_of: NaiveDate) -> R<Resolution> {
    let g = graph()?;
    let table = AdjustTable::standard()?;
    Ok(Engine::new(&g, store, rules, &table)?.resolve_version(facts, unit, as_of)?)
}

fn keep_old(rule_type: RuleType, outcome: Outcome) -> R<RuleSet> {
    let t = trig(EventKind::Act, Comparator::Before, Pivot::EffectiveDate);
    Ok(RuleSet::new(vec![simple_rule("R-17371-2", &road_unit(), (17371, d(2020, 6, 9)?), rule_type, t, outcome)]))
}

fn yielded(r: &Resolution) -> Option<String> {
    r.trace.iter().find_map(|s| match s {
        Step::AdjustYielded { addendum_rule_id, .. } => Some(addendum_rule_id.clone()),
        _ => None,
    })
}

fn after_act(kind: CaseKind, extra: EventKind, at: NaiveDate) -> R<CaseFacts> {
    Ok(CaseFacts::new(kind)
        .with(EventKind::Act, DateSpec::Point(d(2020, 1, 5)?))
        .with(extra, DateSpec::Point(at)))
}

#[test]
fn criminal_lighter_law_yields_to_transition_rule() -> R {
    let rules = keep_old(RuleType::TransitionKeepOld, Outcome::Old)?;
    let facts = after_act(CaseKind::CriminalSubstantive, EventKind::Judgment, d(2022, 12, 22)?)?;
    let r = run(&road_store()?, &rules, &facts, &road_unit(), d(2022, 12, 22)?)?;
    assert_eq!(yielded(&r).as_deref(), Some("R-17371-2"));
    assert!(!r.has_review());
    assert_eq!(r.versions.len(), 1);
    assert_eq!(r.versions[0].promul_no, PromulNo::statute(0));
    Ok(())
}

#[test]
fn sanction_lighter_law_yields_to_transition_rule() -> R {
    let rules = keep_old(RuleType::TransitionKeepOld, Outcome::Old)?;
    let facts = after_act(CaseKind::AdminSanction, EventKind::Disposition, d(2022, 1, 10)?)?;
    let r = run(&road_store()?, &rules, &facts, &road_unit(), d(2022, 1, 10)?)?;
    assert_eq!(yielded(&r).as_deref(), Some("R-17371-2"));
    assert!(!r.has_review());
    Ok(())
}

#[test]
fn fine_lighter_law_yields_to_transition_rule() -> R {
    let rules = keep_old(RuleType::TransitionKeepOld, Outcome::Old)?;
    let facts = CaseFacts::new(CaseKind::AdminFine).with(EventKind::Act, DateSpec::Point(d(2020, 1, 5)?));
    let r = run(&road_store()?, &rules, &facts, &road_unit(), d(2022, 1, 10)?)?;
    assert_eq!(yielded(&r).as_deref(), Some("R-17371-2"));
    assert!(!r.has_review());
    let without = run(&road_store()?, &RuleSet::default(), &facts, &road_unit(), d(2022, 1, 10)?)?;
    assert!(without.has_review());
    Ok(())
}

#[test]
fn deeming_rule_does_not_stop_adjustment() -> R {
    let rules = keep_old(RuleType::TransitionDeem, Outcome::DeemedNew)?;
    let facts = after_act(CaseKind::CriminalSubstantive, EventKind::Judgment, d(2022, 12, 22)?)?;
    let r = run(&road_store()?, &rules, &facts, &road_unit(), d(2022, 12, 22)?)?;
    assert_eq!(yielded(&r), None);
    assert!(r.trace.iter().any(|s| matches!(s, Step::Addendum { rule_id, .. } if rule_id == "R-17371-2")));
    assert!(r.has_review());
    Ok(())
}

#[test]
fn application_adjustment_runs_even_with_addendum_rule() -> R {
    let t = trig(EventKind::Disposition, Comparator::AtOrAfter, Pivot::EffectiveDate);
    let rule = simple_rule("R-16075-2", &animal_unit(), (16075, d(2018, 12, 24)?), RuleType::Application, t, Outcome::New);
    let facts = CaseFacts::new(CaseKind::AdminApplication)
        .with(EventKind::ApplicationFiled, DateSpec::Point(d(2017, 3, 8)?))
        .with(EventKind::Disposition, DateSpec::Point(d(2019, 4, 10)?));
    let r = run(&animal_store()?, &RuleSet::new(vec![rule]), &facts, &animal_unit(), d(2023, 2, 2)?)?;
    assert!(r.trace.iter().any(|s| matches!(s, Step::Addendum { rule_id, .. } if rule_id == "R-16075-2")));
    assert!(r.flags.iter().any(|f| matches!(f, Flag::Review { note, .. } if note.contains("처리기간 763일"))));
    Ok(())
}
