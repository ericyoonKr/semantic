mod common;

use std::path::Path;
use std::time::Duration;

use axum::http::StatusCode;
use common::{
    Boxed, CONSUMER, SID, TestResult, call, config, post_json, request, tiny_docx, upload_request,
    write_bundle,
};
use serde_json::{Value, json};
use server::AppState;

const CLAIM: &str = "CLAIM_T";

const ENGINE: [(&str, &str); 6] = [
    ("LLM_BASE_URL", "http://localhost:9"),
    ("LLM_MODEL", "test-model"),
    ("LEAF_FACT_BUDGET_TOKENS", "100000"),
    ("LEAF_CANDIDATE_K", "4"),
    ("LEAF_AUTHORITY_K", "3"),
    ("SOLVER_TIMEOUT_MS", "2000"),
];

fn tree_lines() -> Vec<String> {
    let node = |id: &str, gate: &str, extra: Value| {
        let mut body = json!({"id": id, "gate": gate, "label": format!("{id} 설명"), "kind": "rule", "burden": "pending"});
        if let (Some(map), Value::Object(more)) = (body.as_object_mut(), extra) {
            map.extend(more);
        }
        json!({ "node": body })
    };
    let leaf = |id: &str| json!({"proposition": format!("{id} 명제"), "ask": format!("{id}에 관한 사실은 무엇입니까?")});
    let edge = |p: &str, c: &str| json!({"edge": {"source": p, "target": c, "relation": "element", "evidence": []}});
    let mut a = leaf("A");
    if let Some(map) = a.as_object_mut() {
        map.insert("evidence".into(), json!([{"law": "시험법", "unit": "제1조"}]));
    }
    let mut s1 = leaf("S1");
    if let Some(map) = s1.as_object_mut() {
        map.insert(
            "arithmetic".into(),
            json!({"within_days": {"from": "안_날", "to": "제소일", "days": 90}}),
        );
    }
    let head = json!({"head": {
        "schema": CONSUMER, "graph": CLAIM, "shape": "나무", "source_type": "R",
        "contract": {"node_kinds": ["rule"], "relations": {
            "element": {"acyclic": true}, "defense": {"acyclic": true}, "rebuttal": {"acyclic": true}}},
        "counts": {"nodes": 5, "edges": 4, "states": 0, "leaves": 3}
    }});
    vec![
        head,
        node("ROOT", "AND", json!({})),
        node("A", "LEAF", a),
        node("B", "LEAF", leaf("B")),
        node("S", "AND", json!({})),
        node("S1", "LEAF", s1),
        edge("ROOT", "A"),
        edge("ROOT", "B"),
        edge("ROOT", "S"),
        edge("S", "S1"),
    ]
    .into_iter()
    .map(|v| v.to_string())
    .collect()
}

async fn app_with(root: &Path, extra: &[(&str, &str)]) -> Boxed<(AppState, axum::Router)> {
    write_bundle(&root.join("kg"), "semantic-rules", CLAIM, &tree_lines())?;
    let state = AppState::open(config(root, extra)?).await?;
    let router = server::http::router(state.clone());
    Ok((state, router))
}

fn judge_uri() -> String {
    format!("/sessions/{SID}/judge")
}

#[tokio::test]
async fn catalog_lists_every_node_with_gate_and_evidence() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (state, router) = app_with(tmp.path(), &[]).await?;
    assert_eq!(state.rules.len(), 1);
    let (status, body) = call(&router, request("GET", "/catalog", axum::body::Body::empty())?).await?;
    assert_eq!(status, StatusCode::OK);
    let items = body.as_array().cloned().unwrap_or_default();
    assert_eq!(items.len(), 5, "{body}");
    assert_eq!(items[0]["node_id"], "ROOT");
    assert_eq!(items[0]["gate"], "AND");
    let a = items.iter().find(|i| i["node_id"] == "A").cloned().unwrap_or_default();
    assert_eq!(a["claim_type"], CLAIM);
    assert_eq!(a["label"], "A 설명");
    assert_eq!(a["gate"], "LEAF");
    assert_eq!(a["evidence"], json!([{"law": "시험법", "unit": "제1조"}]));
    Ok(())
}

#[tokio::test]
async fn tampered_tree_is_not_loaded_at_all() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let mut lines = tree_lines();
    lines[0] = lines[0].replace("\"leaves\":3", "\"leaves\":2");
    write_bundle(&tmp.path().join("kg"), "semantic-rules", CLAIM, &lines)?;
    let state = AppState::open(config(tmp.path(), &[])?).await?;
    assert!(state.rules.is_empty());
    assert!(state.rules.catalog().is_empty());
    Ok(())
}

#[tokio::test]
async fn judge_without_model_settings_says_it_cannot_judge() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (_state, router) = app_with(tmp.path(), &[]).await?;
    let (status, body) = post_json(&router, &judge_uri(), json!({"claim_type": CLAIM, "node_id": "A"})).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["verdict"].is_null());
    assert!(body["text"].as_str().is_some_and(|t| t.starts_with("판정 불가: 판정 기능이 꺼져 있음")), "{body}");
    assert_eq!(body["path"], json!([
        {"node_id": "A", "label": "A 설명", "gate": "LEAF"},
        {"node_id": "ROOT", "label": "ROOT 설명", "gate": "AND"}
    ]));
    Ok(())
}

#[tokio::test]
async fn judge_reports_unknown_leaves_needed_facts_and_assumptions() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (_state, router) = app_with(tmp.path(), &ENGINE).await?;
    let (status, body) = post_json(&router, &judge_uri(), json!({"claim_type": CLAIM, "node_id": "A"})).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["verdict"]["outcome"], "Reserved");
    assert_eq!(body["verdict"]["scope"], "A");
    assert_eq!(body["verdict"]["unknown_leaves"][0]["reason"], "업로드 문서와 사용자 진술이 없음");
    assert_eq!(body["needed_facts"], json!([{"node_id": "A", "ask": "A에 관한 사실은 무엇입니까?"}]));
    let text = body["text"].as_str().unwrap_or_default();
    assert!(text.contains("이 판정은 A 서브트리에 한정되며"), "{text}");
    assert!(text.contains("- A A 설명: 업로드 문서와 사용자 진술이 없음 — 판정 불가"), "{text}");

    let said = json!({"claim_type": CLAIM, "node_id": "A",
        "user_facts": [{"fact_id": "F1", "text": "그 일은 어제 있었다", "message_id": "m1"}]});
    let (_, body) = post_json(&router, &judge_uri(), said).await?;
    assert_eq!(body["verdict"]["unknown_leaves"][0]["reason"], "근거 자료를 검색하지 못함", "{body}");

    let assumed = json!({"claim_type": CLAIM, "node_id": "A",
        "assumptions": [{"node_id": "A", "value": true, "said": "A는 된다고 치고"}]});
    let (_, body) = post_json(&router, &judge_uri(), assumed).await?;
    assert_eq!(body["verdict"]["outcome"], "Established", "{body}");
    assert_eq!(body["verdict"]["assumed"][0]["value"], true);
    let text = body["text"].as_str().unwrap_or_default();
    assert!(text.contains("가정한 값 (문서 근거 없음·가정)"), "{text}");
    assert!(text.contains("「A는 된다고 치고」"), "{text}");
    Ok(())
}

#[tokio::test]
async fn arithmetic_scope_and_bad_inputs_are_answered_plainly() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (_state, router) = app_with(tmp.path(), &ENGINE).await?;
    let (status, body) = post_json(&router, &judge_uri(), json!({"claim_type": CLAIM, "node_id": "ROOT"})).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["verdict"].is_null());
    assert!(body["text"].as_str().is_some_and(|t| t.contains("산술 리프") && t.contains("S1")), "{body}");
    for (bad, why) in [
        (json!({"claim_type": "NOPE", "node_id": "A"}), "알 수 없는 청구 유형"),
        (json!({"claim_type": CLAIM, "node_id": "ZZ"}), "요건 트리에 없는 노드"),
        (json!({"claim_type": CLAIM, "node_id": "A", "assumptions": [{"node_id": "B", "value": true}]}), "판정 범위 밖"),
        (json!({"claim_type": CLAIM, "node_id": "A", "assumptions": [
            {"node_id": "A", "value": true}, {"node_id": "A", "value": false}]}), "서로 다른 가정 값"),
        (json!({"claim_type": CLAIM, "node_id": "A", "user_facts": [
            {"fact_id": "F1", "text": "x", "message_id": "m"}, {"fact_id": "F1", "text": "y", "message_id": "m"}]}), "fact_id"),
    ] {
        let (status, body) = post_json(&router, &judge_uri(), bad).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(body["error"].as_str().is_some_and(|e| e.contains(why)), "{body}");
    }
    Ok(())
}

#[tokio::test]
async fn upload_cancels_a_judge_waiting_for_the_solver_cap() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (state, router) = app_with(tmp.path(), &ENGINE).await?;
    assert_eq!(state.z3.available_permits(), 4);
    let held = state.z3.acquire_many(4).await?;
    let body = json!({"claim_type": CLAIM, "node_id": "A"}).to_string();
    let req = request("POST", &judge_uri(), axum::body::Body::from(body))?;
    let pending = tokio::spawn(tower::ServiceExt::oneshot(router.clone(), req));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!pending.is_finished());
    let docx = tiny_docx("Heading", "Body text")?;
    let (status, upload) = call(&router, upload_request(SID, "new.docx", &docx)?).await?;
    assert_eq!(status, StatusCode::OK, "{upload}");
    let res = pending.await??;
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await?;
    let body: Value = serde_json::from_slice(&bytes)?;
    assert!(body["verdict"].is_null());
    assert_eq!(body["text"], "판정 불가: 문서 수정판 도착");
    drop(held);
    let (_, again) = post_json(&router, &judge_uri(), json!({"claim_type": CLAIM, "node_id": "A"})).await?;
    assert_eq!(again["verdict"]["outcome"], "Reserved", "{again}");
    Ok(())
}
