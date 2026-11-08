mod common;

use common::{R, d, graph, graph_json, road_store, road_unit};
use lawtime::Error;
use lawtime::addendum::RuleSet;
use lawtime::adjust::AdjustTable;
use lawtime::facts::{CaseFacts, CaseKind, DateSpec, EventKind};
use lawtime::principles::Graph;
use lawtime::resolve::{Engine, Flag, MissingReason, Status};
use serde_json::json;

fn run(facts: &CaseFacts, as_of: chrono::NaiveDate) -> R<lawtime::resolve::Resolution> {
    let g = graph()?;
    let store = road_store()?;
    let rules = RuleSet::default();
    let table = AdjustTable::standard()?;
    Ok(Engine::new(&g, &store, &rules, &table)?.resolve_version(facts, &road_unit(), as_of)?)
}

#[test]
fn missing_reference_fact_is_returned_not_guessed() -> R {
    let facts = CaseFacts::new(CaseKind::CriminalSubstantive);
    let r = run(&facts, d(2022, 12, 22)?)?;
    assert_eq!(r.status, Status::MissingFacts);
    assert!(r.versions.is_empty());
    assert_eq!(r.missing[0].event, EventKind::Act);
    assert_eq!(r.missing[0].reason, MissingReason::Absent);
    Ok(())
}

#[test]
fn month_level_date_across_effective_date_is_undetermined() -> R {
    let facts = CaseFacts::new(CaseKind::CriminalSubstantive)
        .with(EventKind::Act, DateSpec::month(2020, 12)?)
        .with(EventKind::Judgment, DateSpec::Point(d(2021, 5, 1)?));
    let r = run(&facts, d(2021, 5, 1)?)?;
    assert_eq!(r.status, Status::Undetermined);
    assert_eq!(r.versions.len(), 2);
    assert!(r.flags.iter().any(|f| matches!(f, Flag::RefTimeUndetermined { .. })));
    Ok(())
}

#[test]
fn reference_date_on_effective_date_gives_both_versions() -> R {
    let facts = CaseFacts::new(CaseKind::CriminalSubstantive)
        .with(EventKind::Act, DateSpec::Point(d(2020, 12, 10)?))
        .with(EventKind::Judgment, DateSpec::Point(d(2021, 5, 1)?));
    let r = run(&facts, d(2021, 5, 1)?)?;
    assert!(r.flags.iter().any(|f| matches!(f, Flag::Boundary { .. })));
    let nos: Vec<u32> = r.versions.iter().map(|v| v.promul_no.number).collect();
    assert_eq!(nos, vec![17371, 0]);
    Ok(())
}

#[test]
fn graph_with_unregistered_kind_or_relation_fails() -> R {
    let bad_kind = graph_json(vec![json!({"id": "Z:1", "type": "학설", "label": "x"})], Vec::new());
    assert!(matches!(Graph::from_json(&bad_kind.to_string()), Err(Error::UnknownNodeKind(_))));
    let bad_rel = graph_json(Vec::new(), vec![json!({"from": "P:행위시법", "to": "P:처분시법", "rel": "닮음"})]);
    assert!(matches!(Graph::from_json(&bad_rel.to_string()), Err(Error::UnknownRelation(_))));
    let mut extra_field = graph_json(Vec::new(), Vec::new());
    extra_field["nodes"][0]["새칸"] = json!(1);
    assert!(matches!(Graph::from_json(&extra_field.to_string()), Err(Error::Json(_))));
    Ok(())
}
