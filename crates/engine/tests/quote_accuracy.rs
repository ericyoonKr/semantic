#[path = "fake/deps.rs"]
mod deps;
#[path = "fake/llm.rs"]
mod llm_fake;
#[path = "fake/tree.rs"]
mod tree;

use std::collections::HashMap;

use engine::{FailureReason, GateType, LeafResult, NodeId, judge_all};
use tokio_util::sync::CancellationToken;

use deps::{deps, flat, prepare, proposition};
use llm_fake::{Script, reply, serve};
use tree::TestResult;

const DOC: &str = "--- p1 ---\n갑은 2026년 1월 5일 신청서를 제출하였다. 을은 같은 해 1월 20일\n보완을 요구하였다.\n보완 기한은  통지를 받은 날부터 14일로 정하였다.\n갑은 1월 30일 보완 서류를 보냈다.\n--- p2 ---\n을은 2월 10일 신청을 반려하였다. 반려 사유는 첨부 서류 중\n도면이 빠졌다는 것이다.\n갑은 도면을 처음부터 첨부하였다고 주장한다.\n수수료 12,000원은 신청 당일 납부되었다.\n을의 담당 부서는\t민원 처리 부서이다.\n이 문서는 두 쪽으로 되어 있다.\n";

const GENUINE: [&str; 10] = [
    "갑은 2026년 1월 5일\n신청서를 제출하였다.",
    "을은 같은 해 1월 20일 보완을 요구하였다.",
    "보완 기한은 통지를 받은 날부터 14일로 정하였다.",
    "  갑은 1월 30일   보완 서류를 보냈다.  ",
    "을은 2월 10일\t신청을 반려하였다.",
    "반려 사유는 첨부 서류 중 도면이 빠졌다는 것이다.",
    "갑은 도면을\r\n처음부터 첨부하였다고 주장한다.",
    "수수료 12,000원은 신청\n\n당일 납부되었다.",
    "을의 담당 부서는 민원 처리 부서이다.",
    "이 문서는\u{3000}두 쪽으로 되어 있다.",
];

const CHANGED: [&str; 10] = [
    "갑은 2026년 1월 6일 신청서를 제출하였다.",
    "을은 같은 해 1월 20일 보완을 요청하였다.",
    "보완 기한은 통지를 받은 날부터 15일로 정하였다.",
    "갑은 1월 30일 보완 서류를 보낸다.",
    "을은 2월 10일 신청을 반려하였다,",
    "반려 사유는 첨부 서류 중 도편이 빠졌다는 것이다.",
    "을은 도면을 처음부터 첨부하였다고 주장한다.",
    "수수료 13,000원은 신청 당일 납부되었다.",
    "갑의 담당 부서는 민원 처리 부서이다.",
    "이 문서는 세 쪽으로 되어 있다.",
];

const SPACE_INSERTED: [&str; 3] = [
    "갑은 2026년 1월 5 일 신청서를 제출하였다.",
    "보완을 요구 하였다.",
    "수수료 12,000 원은 신청 당일 납부되었다.",
];

fn names(prefix: &str, n: usize) -> Vec<String> {
    (0..n).map(|i| format!("{prefix}{i}")).collect()
}

fn count(leaves: &HashMap<NodeId, LeafResult>, ids: &[String]) -> (usize, usize) {
    let mut passed = 0;
    let mut blocked = 0;
    for id in ids {
        match leaves.get(&NodeId::new(id.as_str())) {
            Some(LeafResult::Judged { .. }) => passed += 1,
            Some(LeafResult::Unknown {
                failure: FailureReason::QuoteNotFound,
            }) => blocked += 1,
            _ => {}
        }
    }
    (passed, blocked)
}

#[tokio::test]
async fn quote_check_accuracy_on_sample() -> TestResult {
    let genuine = names("G", GENUINE.len());
    let changed = names("C", CHANGED.len());
    let inserted = names("S", SPACE_INSERTED.len());
    let mut scripts = HashMap::new();
    for (ids, quotes) in [
        (&genuine, &GENUINE[..]),
        (&changed, &CHANGED[..]),
        (&inserted, &SPACE_INSERTED[..]),
    ] {
        for (id, quote) in ids.iter().zip(quotes) {
            scripts.insert(
                proposition(id),
                Script::says(&[reply("True", quote, "doc")]),
            );
        }
    }
    let fake = serve(scripts).await?;
    let d = deps(&fake.url, DOC)?;
    let all: Vec<&str> = genuine
        .iter()
        .chain(&changed)
        .chain(&inserted)
        .map(String::as_str)
        .collect();
    let g = flat(GateType::And, &all, &[])?;
    let leaves = judge_all(
        prepare(&g, &d, Vec::new()).await,
        &d,
        "session-1",
        &CancellationToken::new(),
    )
    .await;
    let (genuine_passed, genuine_blocked) = count(&leaves, &genuine);
    let (changed_passed, changed_blocked) = count(&leaves, &changed);
    let (inserted_passed, inserted_blocked) = count(&leaves, &inserted);
    println!(
        "quote check: genuine passed {genuine_passed}/{} (blocked {genuine_blocked}), changed blocked {changed_blocked}/{} (passed {changed_passed}), space inserted passed {inserted_passed}/{} (blocked {inserted_blocked})",
        GENUINE.len(),
        CHANGED.len(),
        SPACE_INSERTED.len(),
    );
    assert_eq!(genuine_passed, GENUINE.len());
    assert_eq!(changed_blocked, CHANGED.len());
    assert_eq!(inserted_passed, SPACE_INSERTED.len());
    Ok(())
}
