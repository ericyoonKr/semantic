mod common;

use lawtime::addendum::RuleSet;
use lawtime::adjust::AdjustTable;
use lawtime::principles::{Graph, NodeKind};
use lawtime::resolve::Engine;
use lawtime::versions::MemoryStore;

use common::{R, animal_store, animal_unit, d};
use lawtime::facts::{CaseFacts, CaseKind, DateSpec, EventKind};
use lawtime::resolve::Step;

#[test]
#[ignore]
fn real_principle_graph_loads_and_fits_engine() -> R {
    let g = Graph::from_env()?;
    assert_eq!(g.nodes().count(), 116);
    assert_eq!(g.edges().len(), 220);
    let kinds = [
        (NodeKind::RefTime, 18),
        (NodeKind::Field, 8),
        (NodeKind::Principle, 33),
        (NodeKind::Exception, 17),
        (NodeKind::Statute, 15),
        (NodeKind::Precedent, 13),
        (NodeKind::Addendum, 11),
        (NodeKind::History, 1),
    ];
    for (kind, n) in kinds {
        assert_eq!(g.count(kind), n, "{kind:?}");
    }
    assert_eq!(g.edges().iter().filter(|e| e.quote.is_some()).count(), 70);
    let store = MemoryStore::new();
    let rules = RuleSet::default();
    let table = AdjustTable::standard()?;
    Engine::new(&g, &store, &rules, &table)?;
    let quotes: usize = g
        .nodes()
        .filter(|n| n.kind.is_rule())
        .map(|n| g.reference(&n.id).map(|r| r.quotes.len()))
        .sum::<Result<usize, _>>()?;
    println!("근거 인용 {quotes}개, 인용검사 {}/{}", g.check().quote_checks_found, g.check().quote_checks);
    Ok(())
}

#[test]
#[ignore]
fn real_graph_gives_quotes_in_trace() -> R {
    let g = Graph::from_env()?;
    let store = animal_store()?;
    let rules = RuleSet::default();
    let table = AdjustTable::standard()?;
    let engine = Engine::new(&g, &store, &rules, &table)?;
    let facts = CaseFacts::new(CaseKind::AdminApplication)
        .with(EventKind::ApplicationFiled, DateSpec::Point(d(2017, 3, 8)?))
        .with(EventKind::Disposition, DateSpec::Point(d(2019, 4, 10)?));
    let r = engine.resolve_version(&facts, &animal_unit(), d(2023, 2, 2)?)?;
    let Some(Step::RefTime { principle, time_id, .. }) = r.trace.first() else {
        return Err("기준시 단계가 없음".into());
    };
    assert_eq!(principle.id, "P:처분시법");
    assert_eq!(time_id, "T:처분시");
    assert!(!principle.quotes.is_empty());
    println!("{}", serde_json::to_string_pretty(&r.trace)?);
    Ok(())
}
