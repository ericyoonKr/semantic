use chrono::NaiveDate;
use lawtime::addendum::RuleSet;
use lawtime::adjust::AdjustTable;
use lawtime::citation::{CiteStatus, parse_one, resolve_citation};
use lawtime::facts::{CaseFacts, CaseKind, DateSpec, EventKind};
use lawtime::lanes::{LaneOutcome, compare};
use lawtime::principles::Graph;
use lawtime::resolve::{Engine, Flag, Status};
use lawtime::versions::pg::{LawLookup, LawRef, PgStore, article_of, find_law};
use lawtime::versions::{PromulNo, UnitKey, VersionStore};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

type R<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn d(y: i32, m: u32, day: u32) -> R<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, day).ok_or_else(|| "날짜".into())
}

async fn pool() -> R<Option<PgPool>> {
    let Ok(url) = std::env::var("CORPUS_PG_URL") else {
        eprintln!("CORPUS_PG_URL이 없어 건너뜀");
        return Ok(None);
    };
    Ok(Some(PgPoolOptions::new().max_connections(2).connect(&url).await?))
}

async fn road(pool: &PgPool) -> R<LawRef> {
    match find_law(pool, "도로 교통법").await? {
        LawLookup::One(l) => Ok(l),
        other => Err(format!("도로교통법을 하나로 찾지 못함: {other:?}").into()),
    }
}

#[test]
fn article_is_read_at_article_level() -> R {
    assert_eq!(article_of("제148조의2 제1항")?, Some("제148조의2".to_string()));
    assert_eq!(article_of("제 33 조 제4항 제5호 (나)목")?, Some("제33조".to_string()));
    assert_eq!(article_of("부칙 제23조")?, None);
    Ok(())
}

#[tokio::test]
async fn real_db_versions_by_date_and_promulgation() -> R {
    let Some(pool) = pool().await? else { return Ok(()) };
    let law = road(&pool).await?;
    let unit = UnitKey::article(&law.law_id, "제156조");
    let store = PgStore::load(&pool, std::slice::from_ref(&law), std::slice::from_ref(&unit), None).await?;
    let v = store.at(&unit, d(2023, 1, 1)?)?.ok_or("2023-01-01 판본 없음")?;
    assert_eq!(v.promul_no, PromulNo::statute(18522));
    assert_eq!(v.valid_period.from, d(2022, 12, 1)?);
    assert_eq!(store.effective_date(&unit, &PromulNo::statute(18741))?, Some(d(2022, 7, 12)?));
    assert_eq!(store.promul_date(&law.law_id, &PromulNo::statute(18741))?, Some(d(2022, 1, 11)?));
    assert_eq!(store.between(&unit, d(2022, 7, 1)?, Some(d(2022, 12, 31)?))?.len(), 3);
    assert!(store.first_effective(&law.law_id).is_some());
    Ok(())
}

#[tokio::test]
async fn real_db_citations_resolve_or_say_outside_data() -> R {
    let Some(pool) = pool().await? else { return Ok(()) };
    let law = road(&pool).await?;
    let inside = parse_one("구 도로교통법(2022. 1. 11. 법률 제18741호로 개정되기 전의 것) 제156조")?.ok_or("인용 못 읽음")?;
    let outside = parse_one(
        "구 도로교통법(2020. 6. 9. 법률 제17371호로 개정되어 2020. 12. 10. 시행되기 전의 것) 제148조의2 제1항",
    )?
    .ok_or("인용 못 읽음")?;
    let u156 = UnitKey::article(&law.law_id, "제156조");
    let u148 = UnitKey::article(&law.law_id, "제148조의2");
    let store = PgStore::load(&pool, std::slice::from_ref(&law), &[u156.clone(), u148.clone()], None).await?;
    let judged = d(2023, 3, 1)?;
    let a = store.check_citation(resolve_citation(&store, &inside, &u156, judged)?, &[d(2022, 1, 11)?]);
    assert_eq!(a.status, CiteStatus::Resolved);
    let v = a.version.ok_or("판본 없음")?;
    assert_eq!(v.promul_no, PromulNo::statute(18491));
    assert_eq!(a.base_date, Some(d(2022, 7, 11)?));
    let b = store.check_citation(resolve_citation(&store, &outside, &u148, judged)?, &[d(2020, 6, 9)?]);
    assert!(matches!(b.status, CiteStatus::OutsideData { .. }), "{:?}", b.status);
    Ok(())
}

#[tokio::test]
async fn real_db_engine_holds_before_data_and_flags_addenda() -> R {
    let Some(pool) = pool().await? else { return Ok(()) };
    let Ok(graph) = Graph::from_env() else {
        eprintln!("LAWTIME_PRINCIPLES가 없어 건너뜀");
        return Ok(());
    };
    let law = road(&pool).await?;
    let unit = UnitKey::article(&law.law_id, "제156조");
    let store = PgStore::load(&pool, std::slice::from_ref(&law), std::slice::from_ref(&unit), None).await?;
    let rules = RuleSet::default();
    let table = AdjustTable::standard()?;
    let engine = Engine::new(&graph, &store, &rules, &table)?;
    let as_of = d(2026, 11, 8)?;
    let early = CaseFacts::new(CaseKind::CriminalSubstantive).with(EventKind::Act, DateSpec::Point(d(2020, 1, 5)?));
    let r = store.check_resolution(&unit, engine.resolve_version(&early, &unit, as_of)?, as_of);
    assert_eq!(r.status, Status::Undetermined);
    assert!(r.flags.iter().any(|f| matches!(f, Flag::DataHold { .. })));
    let inside = CaseFacts::new(CaseKind::CriminalSubstantive).with(EventKind::Act, DateSpec::Point(d(2023, 1, 1)?));
    let r = store.check_resolution(&unit, engine.resolve_version(&inside, &unit, as_of)?, as_of);
    let first = r.versions.first().ok_or("판본 없음")?;
    assert_eq!(first.valid_period.from, d(2022, 12, 1)?);
    assert!(!r.flags.iter().any(|f| matches!(f, Flag::DataHold { .. })));
    let addenda = r.flags.iter().filter(|f| matches!(f, Flag::AddendumUndetermined { .. })).count();
    println!("상태 {:?}, 판본 {}개, 표시 {}개(부칙 미평가 {addenda}개)", r.status, r.versions.len(), r.flags.len());
    if addenda > 0 {
        assert_ne!(r.status, Status::Decided);
    }
    let cite = parse_one("구 도로교통법(2022. 1. 11. 법률 제18741호로 개정되기 전의 것) 제156조")?.ok_or("인용")?;
    let answer = store.check_citation(resolve_citation(&store, &cite, &unit, as_of)?, &[]);
    assert!(matches!(compare(&answer, &r), LaneOutcome::Different | LaneOutcome::AmongSeveral));
    Ok(())
}
