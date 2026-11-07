#[path = "fake/deps.rs"]
mod deps;
#[path = "fake/llm.rs"]
mod llm_fake;
#[path = "fake/tree.rs"]
mod tree;

use std::collections::HashMap;

use engine::{
    EngineError, FailureReason, GateType, JudgeRequest, NodeId, Outcome, judge,
    leaf::MODEL_FOUND_NOTHING,
};
use tokio_util::sync::CancellationToken;

use deps::{deps, flat, proposition};
use llm_fake::{Script, reply, serve};
use tree::TestResult;

const DOC: &str =
    "--- p1 ---\n첫째 문장은 갑이 신청서를 냈다는 것이다.\n둘째 문장은 을이 회신했다는 것이다.\n";
const QA: &str = "갑이 신청서를 냈다";
const QB: &str = "을이 회신했다";

fn request(root: &str) -> JudgeRequest {
    JudgeRequest {
        session: "session-1".into(),
        root: NodeId::new(root),
        assumptions: HashMap::new(),
        user_facts: Vec::new(),
        today: chrono::NaiveDate::from_ymd_opt(2026, 11, 8).unwrap_or_default(),
    }
}

#[tokio::test]
async fn judge_with_all_leaves_true_is_established() -> TestResult {
    let fake = serve(HashMap::from([
        (proposition("A"), Script::says(&[reply("True", QA, "doc")])),
        (proposition("B"), Script::says(&[reply("True", QB, "doc")])),
    ]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["A", "B"], &[])?;
    let v = judge(&g, request("ROOT"), &d, &CancellationToken::new()).await?;
    assert_eq!(v.outcome, Outcome::Established);
    assert_eq!(v.scope, NodeId::new("ROOT"));
    assert!(v.unknown_leaves.is_empty());
    assert!(v.needed_facts.is_empty());
    assert_eq!(v.path.len(), 1);
    Ok(())
}

#[tokio::test]
async fn judge_with_unknown_leaf_is_reserved_with_needed_facts() -> TestResult {
    let fake = serve(HashMap::from([
        (proposition("A"), Script::says(&[reply("True", QA, "doc")])),
        (proposition("B"), Script::says(&[reply("Unknown", "", "")])),
    ]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["A", "B"], &[])?;
    let v = judge(&g, request("ROOT"), &d, &CancellationToken::new()).await?;
    assert_eq!(v.outcome, Outcome::Reserved);
    assert_eq!(
        v.needed_facts,
        vec![(NodeId::new("B"), "ask-B".to_string())]
    );
    assert_eq!(
        v.unknown_leaves,
        vec![(
            NodeId::new("B"),
            FailureReason::NoEvidence(MODEL_FOUND_NOTHING.into())
        )]
    );
    assert_eq!(v.model_hint, Some(vec![(NodeId::new("B"), true)]));
    Ok(())
}

#[tokio::test]
async fn judge_with_false_leaf_is_not_established() -> TestResult {
    let fake = serve(HashMap::from([
        (proposition("A"), Script::says(&[reply("True", QA, "doc")])),
        (proposition("B"), Script::says(&[reply("False", QB, "doc")])),
    ]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["A", "B"], &[])?;
    let v = judge(&g, request("ROOT"), &d, &CancellationToken::new()).await?;
    assert_eq!(v.outcome, Outcome::NotEstablished);
    assert!(v.unsat_core.iter().any(|(id, _)| id.as_str() == "B"));
    Ok(())
}

#[tokio::test]
async fn judge_cancelled_before_start_is_reserved_without_llm() -> TestResult {
    let fake = serve(HashMap::from([(
        proposition("A"),
        Script::says(&[reply("True", QA, "doc")]),
    )]))
    .await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["A"], &[])?;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let v = judge(&g, request("ROOT"), &d, &cancel).await?;
    assert_eq!(v.outcome, Outcome::Reserved);
    assert!(matches!(
        v.unknown_leaves.as_slice(),
        [(_, FailureReason::Cancelled(_))]
    ));
    assert_eq!(fake.total(), 0);
    Ok(())
}

#[tokio::test]
async fn judge_rejects_unknown_root_and_stray_assumption_before_llm() -> TestResult {
    let fake = serve(HashMap::new()).await?;
    let d = deps(&fake.url, DOC)?;
    let g = flat(GateType::And, &["A"], &[])?;
    let missing = judge(&g, request("NOPE"), &d, &CancellationToken::new()).await;
    assert_eq!(missing, Err(EngineError::UnknownNode(NodeId::new("NOPE"))));
    let mut req = request("A");
    req.assumptions.insert(NodeId::new("ROOT"), true);
    let stray = judge(&g, req, &d, &CancellationToken::new()).await;
    assert_eq!(stray, Err(EngineError::UnknownNode(NodeId::new("ROOT"))));
    assert_eq!(fake.total(), 0);
    Ok(())
}
