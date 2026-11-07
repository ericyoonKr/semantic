#[path = "fake/deps.rs"]
mod deps;
#[path = "fake/llm.rs"]
mod llm_fake;
#[path = "fake/tree.rs"]
mod tree;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use docmodel::{FactSource, FileId};
use engine::{FailureReason, GateType, LeafResult, NodeId, Prepared, UserFact, judge_all};
use tokio_util::sync::CancellationToken;

use deps::{FILE, deps, flat, prepare, proposition};
use llm_fake::{Script, reply, serve};
use tree::TestResult;

const DOC: &str = "--- p1 ---\n제1조(목적) 이 약정은 갑과 을 사이의 용역 조건을 정한다.\n제2조(기간) 약정 기간은 2026년 3월 1일부터\n2027년 2월 28일까지로 한다.\n";

fn scripts(pairs: &[(&str, Script)]) -> HashMap<String, Script> {
    pairs
        .iter()
        .map(|(name, s)| (proposition(name), s.clone()))
        .collect()
}

fn unknown_of(result: Option<&LeafResult>) -> Option<FailureReason> {
    match result {
        Some(LeafResult::Unknown { failure }) => Some(failure.clone()),
        _ => None,
    }
}

#[tokio::test]
async fn quote_present_in_paged_text_is_judged_with_span() -> TestResult {
    let quote = "약정 기간은 2026년 3월 1일부터 2027년 2월 28일까지로 한다.";
    let fake = serve(scripts(&[(
        "A",
        Script::says(&[reply("True", quote, "doc")]),
    )]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["A"], &[])?;
    let leaves = judge_all(
        prepare(&g, &d, Vec::new()).await,
        &d,
        "session-1",
        &CancellationToken::new(),
    )
    .await;
    let Some(LeafResult::Judged {
        value,
        anchor,
        quote: kept,
    }) = leaves.get(&NodeId::new("A"))
    else {
        return Err(format!("not judged: {leaves:?}").into());
    };
    assert!(*value);
    assert_eq!(kept, quote);
    assert_eq!(anchor.source, FactSource::File(FileId(FILE.into())));
    let (start, end) = anchor.span.ok_or("no span")?;
    let found: String = DOC
        .chars()
        .skip(start as usize)
        .take((end - start) as usize)
        .collect();
    assert_eq!(
        found,
        "약정 기간은 2026년 3월 1일부터\n2027년 2월 28일까지로 한다."
    );
    Ok(())
}

#[tokio::test]
async fn fabricated_quote_is_quote_not_found() -> TestResult {
    let fake = serve(scripts(&[(
        "A",
        Script::says(&[reply(
            "True",
            "약정 기간은 2026년 4월 1일부터 시작한다.",
            "doc",
        )]),
    )]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["A"], &[])?;
    let leaves = judge_all(
        prepare(&g, &d, Vec::new()).await,
        &d,
        "session-1",
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(
        unknown_of(leaves.get(&NodeId::new("A"))),
        Some(FailureReason::QuoteNotFound)
    );
    Ok(())
}

#[tokio::test]
async fn user_fact_source_anchors_to_user_message() -> TestResult {
    let fake = serve(scripts(&[(
        "A",
        Script::says(&[reply("False", "통지서는  5월 2일에\n받았습니다", "user:f1")]),
    )]))
    .await?;
    let d = deps(&fake.url, "")?;
    let g = flat(GateType::And, &["A"], &[])?;
    let facts = vec![UserFact {
        fact_id: "f1".into(),
        message_id: "msg-7".into(),
        text: "저는 통지서는 5월 2일에 받았습니다.".into(),
    }];
    let prepared = prepare(&g, &d, facts).await;
    assert!(matches!(prepared.first(), Some(Prepared::Ready(_))));
    let leaves = judge_all(prepared, &d, "session-1", &CancellationToken::new()).await;
    let Some(LeafResult::Judged { value, anchor, .. }) = leaves.get(&NodeId::new("A")) else {
        return Err(format!("not judged: {leaves:?}").into());
    };
    assert!(!*value);
    assert_eq!(
        anchor.source,
        FactSource::UserMessage {
            message_id: "msg-7".into()
        }
    );
    assert_eq!(anchor.span, Some((3, 20)));
    Ok(())
}

#[tokio::test]
async fn no_document_and_no_user_facts_is_no_evidence_without_llm() -> TestResult {
    let fake = serve(scripts(&[(
        "A",
        Script::says(&[reply("True", "x", "doc")]),
    )]))
    .await?;
    let d = deps(&fake.url, "  \n")?;
    let g = flat(GateType::And, &["A"], &[])?;
    let prepared = prepare(&g, &d, Vec::new()).await;
    assert!(matches!(
        prepared.first(),
        Some(Prepared::NoEvidence { .. })
    ));
    let leaves = judge_all(prepared, &d, "session-1", &CancellationToken::new()).await;
    assert!(matches!(
        unknown_of(leaves.get(&NodeId::new("A"))),
        Some(FailureReason::NoEvidence(_))
    ));
    assert_eq!(fake.total(), 0);
    Ok(())
}

#[tokio::test]
async fn cache_hit_skips_the_llm() -> TestResult {
    let quote = "이 약정은 갑과 을 사이의 용역 조건을 정한다.";
    let fake = serve(scripts(&[
        ("A", Script::says(&[reply("True", quote, "doc")])),
        ("B", Script::says(&[reply("Unknown", "", "")])),
    ]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["A", "B"], &[])?;
    let cancel = CancellationToken::new();
    let first = judge_all(prepare(&g, &d, Vec::new()).await, &d, "session-1", &cancel).await;
    assert_eq!(fake.total(), 2);
    assert_eq!(d.cache.len(), 2);
    let second = judge_all(prepare(&g, &d, Vec::new()).await, &d, "session-1", &cancel).await;
    assert_eq!(fake.total(), 2);
    assert_eq!(first, second);
    let changed = vec![UserFact {
        fact_id: "f1".into(),
        message_id: "m".into(),
        text: "새 진술".into(),
    }];
    judge_all(prepare(&g, &d, changed).await, &d, "session-1", &cancel).await;
    assert_eq!(fake.total(), 4);
    Ok(())
}

#[tokio::test]
async fn llm_server_error_is_unknown_and_not_cached() -> TestResult {
    let fake = serve(scripts(&[("A", Script::status(503))])).await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["A"], &[])?;
    let leaves = judge_all(
        prepare(&g, &d, Vec::new()).await,
        &d,
        "session-1",
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(
        unknown_of(leaves.get(&NodeId::new("A"))),
        Some(FailureReason::ModelServerDown)
    );
    assert_eq!(d.cache.len(), 0);
    Ok(())
}

#[tokio::test]
async fn cancel_before_completion_marks_pending_leaves_cancelled() -> TestResult {
    let quote = "이 약정은 갑과 을 사이의 용역 조건을 정한다.";
    let slow = Duration::from_secs(3);
    let fake = serve(scripts(&[
        ("FAST", Script::says(&[reply("True", quote, "doc")])),
        ("SLOW1", Script::slow(reply("True", quote, "doc"), slow)),
        ("SLOW2", Script::slow(reply("True", quote, "doc"), slow)),
    ]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["FAST", "SLOW1", "SLOW2"], &[])?;
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        trigger.cancel();
    });
    let started = Instant::now();
    let leaves = judge_all(prepare(&g, &d, Vec::new()).await, &d, "session-1", &cancel).await;
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
    assert!(matches!(
        leaves.get(&NodeId::new("FAST")),
        Some(LeafResult::Judged { .. })
    ));
    for slow in ["SLOW1", "SLOW2"] {
        assert!(matches!(
            unknown_of(leaves.get(&NodeId::new(slow))),
            Some(FailureReason::Cancelled(_))
        ));
    }
    assert_eq!(d.cache.len(), 1);
    Ok(())
}

#[tokio::test]
async fn negation_pair_both_true_is_rejudged_once_then_contradiction() -> TestResult {
    let quote = "이 약정은 갑과 을 사이의 용역 조건을 정한다.";
    let fake = serve(scripts(&[
        ("P", Script::says(&[reply("True", quote, "doc")])),
        ("N", Script::says(&[reply("True", quote, "doc")])),
    ]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::Or, &["P", "N"], &[("N", "P")])?;
    let leaves = judge_all(
        prepare(&g, &d, Vec::new()).await,
        &d,
        "session-1",
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(fake.calls(&proposition("P")), 2);
    assert_eq!(fake.calls(&proposition("N")), 2);
    for id in ["P", "N"] {
        assert_eq!(
            unknown_of(leaves.get(&NodeId::new(id))),
            Some(FailureReason::Contradiction)
        );
    }
    let stored = d.cache.values();
    assert_eq!(stored.len(), 2);
    assert!(
        stored
            .iter()
            .all(|r| unknown_of(Some(r)) == Some(FailureReason::Contradiction))
    );
    Ok(())
}

#[tokio::test]
async fn negation_pair_resolved_on_rejudge_keeps_new_values() -> TestResult {
    let quote = "이 약정은 갑과 을 사이의 용역 조건을 정한다.";
    let fake = serve(scripts(&[
        ("P", Script::says(&[reply("True", quote, "doc")])),
        (
            "N",
            Script::says(&[reply("True", quote, "doc"), reply("False", quote, "doc")]),
        ),
    ]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::Or, &["P", "N"], &[("N", "P")])?;
    let leaves = judge_all(
        prepare(&g, &d, Vec::new()).await,
        &d,
        "session-1",
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(fake.total(), 4);
    assert!(matches!(
        leaves.get(&NodeId::new("P")),
        Some(LeafResult::Judged { value: true, .. })
    ));
    assert!(matches!(
        leaves.get(&NodeId::new("N")),
        Some(LeafResult::Judged { value: false, .. })
    ));
    Ok(())
}
