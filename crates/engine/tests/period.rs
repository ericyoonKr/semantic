#[path = "fake/deps.rs"]
mod deps;
#[path = "fake/llm.rs"]
mod llm_fake;
#[path = "fake/tree.rs"]
mod tree;

use std::collections::HashMap;
use std::sync::Arc;

use checks::holidays::HolidayData;
use checks::periods::Holidays;
use chrono::NaiveDate;
use engine::{
    ArithSpec, ClaimGraph, DateSource, EngineError, ExpectedCounts, FailureReason, GateType,
    JudgeRequest, NodeId, Outcome, Relation, StartRule, UserFact, Verdict, judge,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use deps::{Deps, deps, proposition};
use llm_fake::{Script, serve};
use tree::{TestResult, edge, node};

const DOC: &str = "--- p1 ---\n원고는 2026. 3. 2. 처분서를 받아 처분이 있음을 알았다.\n원고는 2026. 6. 1. 이 소를 제기하였다.\n";
const KNEW: &str = "원고는 2026. 3. 2. 처분서를 받아 처분이 있음을 알았다.";
const FILED: &str = "원고는 2026. 6. 1. 이 소를 제기하였다.";

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or_default()
}

fn graph() -> Result<ClaimGraph, EngineError> {
    let mut s1 = node("S1", GateType::Leaf);
    s1.arithmetic = Some(ArithSpec::WithinDays {
        from: "처분이_있음을_안_날".into(),
        to: "제소일".into(),
        days: 90,
    });
    ClaimGraph::from_rows(
        vec![node("S", GateType::And), s1],
        vec![edge("S", "S1", Relation::Element)],
        &ExpectedCounts { nodes: 2, edges: 1, leaves: 1 },
    )
}

fn answer(from: (&str, &str, &str), to: (&str, &str, &str)) -> String {
    let item = |name: &str, (date, quote, source): (&str, &str, &str)| {
        json!({"name": name, "date": date, "quote": quote, "source": source})
    };
    json!({"dates": [item("처분이_있음을_안_날", from), item("제소일", to)],
           "start": {"value": "Unknown", "quote": "", "source": ""}})
    .to_string()
}

fn request(user_facts: Vec<UserFact>) -> JudgeRequest {
    JudgeRequest {
        session: "session-1".into(),
        root: NodeId::new("S"),
        assumptions: HashMap::new(),
        user_facts,
        today: d(2026, 11, 8),
    }
}

fn with_holidays(mut d: Deps, years: &[i32], days: &[NaiveDate]) -> Deps {
    let table = Holidays::with_years(days.iter().copied(), years.iter().copied());
    d.settings.holidays = Arc::new(HolidayData::Loaded(table));
    d
}

async fn run(text: &str, reply: String, years: &[i32], days: &[NaiveDate], facts: Vec<UserFact>) -> Result<Verdict, Box<dyn std::error::Error>> {
    let fake = serve(HashMap::from([(proposition("S1"), Script::says(&[reply]))])).await?;
    let d = with_holidays(deps(&fake.url, text)?, years, days);
    Ok(judge(&graph()?, request(facts), &d, &CancellationToken::new()).await?)
}

#[tokio::test]
async fn dates_with_quotes_are_counted_and_judged_through_z3() -> TestResult {
    let v = run(DOC, answer(("2026-03-02", KNEW, "doc"), ("2026-06-01", FILED, "doc")), &[2026], &[], Vec::new()).await?;
    assert_eq!(v.outcome, Outcome::Established, "{v:?}");
    assert!(v.unknown_leaves.is_empty());
    let p = &v.periods[0];
    assert_eq!(p.value, Some(true));
    assert_eq!(p.start, StartRule::BothChecked);
    assert_eq!((p.lo, p.hi), (Some(d(2026, 6, 1)), Some(d(2026, 6, 1))));
    assert_eq!(p.cases[0].skipped, vec![d(2026, 5, 31)]);
    assert_eq!(p.from.quote.as_deref(), Some(KNEW));
    assert_eq!(p.to.source, Some(DateSource::Document { page: None }));
    Ok(())
}

#[tokio::test]
async fn quote_not_in_the_facts_leaves_the_date_unknown() -> TestResult {
    let made_up = "원고는 2026. 3. 2. 처분을 알았다고 진술하였다.";
    let v = run(DOC, answer(("2026-03-02", made_up, "doc"), ("2026-06-01", FILED, "doc")), &[2026], &[], Vec::new()).await?;
    assert_eq!(v.outcome, Outcome::Reserved);
    assert_eq!(v.unknown_leaves, vec![(NodeId::new("S1"), FailureReason::QuoteNotFound)]);
    assert_eq!(v.periods[0].value, None);
    let wrong_date = run(DOC, answer(("2026-03-01", KNEW, "doc"), ("2026-06-01", FILED, "doc")), &[2026], &[], Vec::new()).await?;
    assert!(matches!(&wrong_date.unknown_leaves[0].1, FailureReason::NoEvidence(s) if s.contains("인용문에 그 날짜")));
    Ok(())
}

#[tokio::test]
async fn missing_filing_date_uses_the_judging_date() -> TestResult {
    let said = UserFact { fact_id: "F1".into(), message_id: "m1".into(), text: "저는 2026. 3. 2. 처분서를 받았습니다.".into() };
    let reply = answer(("2026-03-02", "저는 2026. 3. 2. 처분서를 받았습니다.", "user:F1"), ("", "", ""));
    let v = run("", reply, &[2026], &[], vec![said]).await?;
    let p = &v.periods[0];
    assert_eq!(p.to.source, Some(DateSource::Today));
    assert_eq!(p.to.date, Some(d(2026, 11, 8)));
    assert_eq!(p.from.source, Some(DateSource::User { fact_id: "F1".into() }));
    assert_eq!(p.value, Some(false));
    assert_eq!(v.outcome, Outcome::NotEstablished, "{v:?}");
    assert_eq!(v.unsat_core.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec!["S1"]);
    Ok(())
}

#[tokio::test]
async fn lunar_holidays_from_the_data_file_move_the_last_day() -> TestResult {
    let doc = "--- p1 ---\n원고는 2025. 11. 17. 처분이 있음을 알았다.\n원고는 2026. 2. 19. 소를 제기하였다.\n";
    let reply = answer(
        ("2025-11-17", "원고는 2025. 11. 17. 처분이 있음을 알았다.", "doc"),
        ("2026-02-19", "원고는 2026. 2. 19. 소를 제기하였다.", "doc"),
    );
    let seollal = [d(2026, 2, 16), d(2026, 2, 17), d(2026, 2, 18)];
    let with = run(doc, reply.clone(), &[2025, 2026], &seollal, Vec::new()).await?;
    assert_eq!(with.periods[0].value, Some(true), "{with:?}");
    assert_eq!(with.periods[0].lo, Some(d(2026, 2, 19)));
    assert_eq!(with.outcome, Outcome::Established);
    let without = run(doc, reply, &[], &[], Vec::new()).await?;
    assert_eq!(without.periods[0].value, None);
    assert!(matches!(&without.unknown_leaves[0].1, FailureReason::NoEvidence(s) if s.contains("공휴일 자료가 없는 해")));
    Ok(())
}

#[tokio::test]
async fn stated_start_time_decides_the_first_day_rule() -> TestResult {
    let doc = "--- p1 ---\n원고는 2026. 1. 9. 오후 3시 처분서를 받았다.\n원고는 2026. 4. 9. 소를 제기하였다.\n";
    let knew = "원고는 2026. 1. 9. 오후 3시 처분서를 받았다.";
    let filed = "원고는 2026. 4. 9. 소를 제기하였다.";
    let reply = |value: &str| {
        json!({"dates": [
            {"name": "처분이_있음을_안_날", "date": "2026-01-09", "quote": knew, "source": "doc"},
            {"name": "제소일", "date": "2026-04-09", "quote": filed, "source": "doc"}],
            "start": {"value": value, "quote": knew, "source": "doc"}})
        .to_string()
    };
    let stated = run(doc, reply("False"), &[2026], &[], Vec::new()).await?;
    assert_eq!(stated.periods[0].start, StartRule::FirstDayExcluded);
    assert_eq!(stated.periods[0].value, Some(true));
    let open = run(doc, reply("Unknown"), &[2026], &[], Vec::new()).await?;
    assert_eq!(open.periods[0].start, StartRule::BothChecked);
    assert_eq!(open.periods[0].value, None);
    let wrong = run(doc, reply("True"), &[2026], &[], Vec::new()).await?;
    assert_eq!(wrong.periods[0].start, StartRule::BothChecked);
    Ok(())
}
