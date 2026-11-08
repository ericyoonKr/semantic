mod common;

use common::{R, animal_store, animal_unit, d, graph, road_store, road_unit, tax_rules, tax_store, tax_unit};
use lawtime::addendum::RuleSet;
use lawtime::adjust::AdjustTable;
use lawtime::facts::{CaseFacts, CaseKind, DateSpec, EventKind, ProcState};
use lawtime::resolve::{Engine, Flag, Resolution, Status, Step};
use lawtime::versions::PromulNo;

fn criminal(judgment: chrono::NaiveDate) -> R<Resolution> {
    let g = graph()?;
    let store = road_store()?;
    let rules = RuleSet::default();
    let table = AdjustTable::standard()?;
    let engine = Engine::new(&g, &store, &rules, &table)?;
    let facts = CaseFacts::new(CaseKind::CriminalSubstantive)
        .with(EventKind::Act, DateSpec::Point(d(2020, 1, 5)?))
        .with(EventKind::Judgment, DateSpec::Point(judgment))
        .with_state(ProcState::Pending);
    Ok(engine.resolve_version(&facts, &road_unit(), judgment)?)
}

fn adjust_ids(r: &Resolution) -> Vec<String> {
    r.trace
        .iter()
        .filter_map(|s| match s {
            Step::Adjust { rule_id, .. } => Some(rule_id.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn road_traffic_lower_court_keeps_old_version() -> R {
    let r = criminal(d(2020, 11, 5)?)?;
    assert_eq!(r.status, Status::Decided);
    assert_eq!(r.versions.len(), 1);
    assert_eq!(r.versions[0].promul_no, PromulNo::statute(0));
    assert!(r.flags.is_empty());
    assert_eq!(adjust_ids(&r), vec!["ADJ-형사-경한신법-동기설"]);
    Ok(())
}

#[test]
fn road_traffic_supreme_court_gives_review_with_both_versions() -> R {
    let r = criminal(d(2022, 12, 22)?)?;
    assert_eq!(r.status, Status::Decided);
    assert!(r.has_review());
    let nos: Vec<u32> = r.versions.iter().map(|v| v.promul_no.number).collect();
    assert_eq!(nos, vec![0, 17371]);
    assert_eq!(adjust_ids(&r), vec!["ADJ-형사-경한신법-문언설"]);
    assert!(!r.needs_expert);
    Ok(())
}

#[test]
fn road_traffic_counterfactual_uses_motive_rule() -> R {
    let r = criminal(d(2022, 6, 1)?)?;
    assert!(r.has_review());
    assert_eq!(r.versions[0].promul_no, PromulNo::statute(0));
    assert_eq!(adjust_ids(&r), vec!["ADJ-형사-경한신법-동기설"]);
    let motive = r.flags.iter().any(|f| matches!(f, Flag::Review { note, .. } if note.contains("반성적 고려")));
    assert!(motive);
    Ok(())
}

#[test]
fn animal_protection_disposition_version_with_delay_flag() -> R {
    let g = graph()?;
    let store = animal_store()?;
    let rules = RuleSet::default();
    let table = AdjustTable::standard()?;
    let engine = Engine::new(&g, &store, &rules, &table)?;
    let facts = CaseFacts::new(CaseKind::AdminApplication)
        .with(EventKind::ApplicationFiled, DateSpec::Point(d(2017, 3, 8)?))
        .with(EventKind::Disposition, DateSpec::Point(d(2019, 4, 10)?))
        .with(EventKind::Judgment, DateSpec::Point(d(2023, 2, 2)?));
    let r = engine.resolve_version(&facts, &animal_unit(), d(2023, 2, 2)?)?;
    assert_eq!(r.status, Status::Decided);
    assert_eq!(r.versions.len(), 1);
    assert_eq!(r.versions[0].promul_no, PromulNo::statute(16075));
    assert_eq!(r.versions[0].valid_period.from, d(2019, 3, 25)?);
    let delay = r.flags.iter().any(|f| {
        matches!(f, Flag::Review { note, .. } if note.contains("처리기간 763일") && note.contains("정당한 이유 없는 지연"))
    });
    assert!(delay);
    Ok(())
}

fn tax_case(kept: bool) -> R<Resolution> {
    let g = graph()?;
    let store = tax_store()?;
    let rules = tax_rules(kept)?;
    let table = AdjustTable::standard()?;
    let engine = Engine::new(&g, &store, &rules, &table)?;
    let facts = CaseFacts::new(CaseKind::AdminLitigation)
        .with(EventKind::Disposition, DateSpec::Approx { from: d(1994, 1, 1)?, to: None });
    Ok(engine.resolve_version(&facts, &tax_unit(), d(2012, 7, 26)?)?)
}

#[test]
fn tax_addendum_retired_by_full_revision_is_skipped_with_flag() -> R {
    let r = tax_case(false)?;
    assert!(r.versions.is_empty());
    let full = d(1994, 1, 1)?;
    let retired = r.flags.iter().any(|f| {
        matches!(f, Flag::FullRevisionRetired { by, effective, .. } if *by == PromulNo::statute(4666) && *effective == full)
    });
    assert!(retired);
    assert!(r.needs_expert);
    assert!(r.trace.iter().any(|s| matches!(s, Step::AddendumSkipped { rule_id, .. } if rule_id == "L4285-ADD-23")));
    Ok(())
}

#[test]
fn tax_addendum_kept_flag_applies_rule() -> R {
    let r = tax_case(true)?;
    assert_eq!(r.versions.len(), 1);
    assert_eq!(r.versions[0].promul_no, PromulNo::statute(4285));
    assert!(r.trace.iter().any(|s| matches!(s, Step::Addendum { rule_id, .. } if rule_id == "L4285-ADD-23")));
    assert!(r.flags.iter().any(|f| matches!(f, Flag::FullRevisionKept { .. })));
    Ok(())
}
