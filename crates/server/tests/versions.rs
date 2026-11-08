mod common;

use axum::Router;
use axum::http::StatusCode;
use common::{Boxed, TestResult, config, post_json};
use serde_json::{Value, json};
use server::AppState;

async fn router_with(extra: &[(&str, &str)]) -> Boxed<(tempfile::TempDir, Router)> {
    let tmp = tempfile::tempdir()?;
    let state = AppState::open(config(tmp.path(), extra)?).await?;
    Ok((tmp, server::http::router(state)))
}

fn reason(v: &Value) -> String {
    v["error"].as_str().unwrap_or_default().to_string()
}

fn real_env() -> Option<(String, String)> {
    let url = std::env::var("CORPUS_PG_URL").ok()?;
    let graph = std::env::var("LAWTIME_PRINCIPLES").ok()?;
    Some((url, graph))
}

const ROAD_156: &str = "구 도로교통법(2022. 1. 11. 법률 제18741호로 개정되기 전의 것) 제156조, \
     구 도로교통법(2020. 6. 9. 법률 제17371호로 개정되어 2020. 12. 10. 시행되기 전의 것) 제148조의2 제1항, \
     구 조세감면규제법(1990. 12. 31. 법률 제4285호) 부칙 제23조";

#[tokio::test]
async fn without_database_or_graph_reports_failure_not_crash() -> TestResult {
    let (_tmp, router) = router_with(&[]).await?;
    let body = json!({"law": "도로교통법", "unit": "제156조", "kind": "CriminalSubstantive",
                      "facts": [{"event": "Act", "on": "2023-01-01"}]});
    let (status, v) = post_json(&router, "/versions/resolve", body).await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{v}");
    assert!(reason(&v).starts_with("판본 결정 실패"), "{v}");
    let (status, v) =
        post_json(&router, "/versions/citations", json!({"text": ROAD_156, "judgment_date": "2023-03-01"})).await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{v}");
    assert!(reason(&v).starts_with("판본 결정 실패"), "{v}");
    Ok(())
}

#[tokio::test]
async fn bad_unit_or_fact_dates_are_argument_errors() -> TestResult {
    let (_tmp, router) = router_with(&[]).await?;
    let body = json!({"law": "도로교통법", "unit": "부칙 제2조", "kind": "CriminalSubstantive"});
    let (status, v) = post_json(&router, "/versions/resolve", body).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    let body = json!({"law": "도로교통법", "unit": "제156조", "kind": "CriminalSubstantive",
                      "facts": [{"event": "Act", "on": "2023-01-01", "from": "2022-01-01"}]});
    let (status, v) = post_json(&router, "/versions/resolve", body).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    let body = json!({"law": "도로교통법", "unit": "제156조", "kind": "CriminalSubstantive",
                      "facts": [{"event": "Act", "from": "2023-02-01", "to": "2023-01-01"}]});
    let (status, v) = post_json(&router, "/versions/resolve", body).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    let (status, v) = post_json(&router, "/versions/citations", json!({"text": " ", "judgment_date": "2023-03-01"})).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{v}");
    Ok(())
}

#[tokio::test]
async fn real_db_resolve_citations_and_lanes() -> TestResult {
    let Some((url, graph)) = real_env() else {
        eprintln!("CORPUS_PG_URL 또는 LAWTIME_PRINCIPLES가 없어 건너뜀");
        return Ok(());
    };
    let (_tmp, router) = router_with(&[("CORPUS_PG_URL", url.as_str()), ("LAWTIME_PRINCIPLES", graph.as_str())]).await?;
    let body = json!({"law": "도로교통법", "unit": "제156조 제1호", "kind": "CriminalSubstantive",
                      "facts": [{"event": "Act", "on": "2023-01-01"}, {"event": "Judgment", "on": "2023-06-01"}],
                      "state": "Pending", "as_of": "2023-06-01"});
    let (status, v) = post_json(&router, "/versions/resolve", body).await?;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["unit"]["key"], "제156조");
    assert_eq!(v["resolution"]["versions"][0]["valid_period"]["from"], "2022-12-01", "{v}");
    println!("판본 결정 상태 {} 표시 {}개", v["resolution"]["status"], v["resolution"]["flags"].as_array().map_or(0, Vec::len));

    let (status, v) =
        post_json(&router, "/versions/citations", json!({"text": ROAD_156, "judgment_date": "2023-03-01"})).await?;
    assert_eq!(status, StatusCode::OK, "{v}");
    let items = v["items"].as_array().ok_or("items")?;
    assert_eq!(items.len(), 3, "{v}");
    assert_eq!(items[0]["resolution"]["status"], "Resolved", "{v}");
    assert_eq!(items[0]["resolution"]["version"]["promul_no"]["number"], 18491);
    assert!(items[1]["resolution"]["status"].get("OutsideData").is_some(), "{v}");
    assert!(items[2]["unresolved"].is_string(), "{v}");

    let body = json!({"text": ROAD_156, "judgment_date": "2023-03-01", "kind": "CriminalSubstantive",
                      "facts": [{"event": "Act", "on": "2022-07-01"}, {"event": "Judgment", "on": "2023-03-01"}],
                      "state": "Pending"});
    let (status, v) = post_json(&router, "/versions/trace", body).await?;
    assert_eq!(status, StatusCode::OK, "{v}");
    let rows = v["rows"].as_array().ok_or("rows")?;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2]["outcome"], "NoAnswer");
    println!("답변 줄·예측 줄 대조 {}", v["counts"]);
    Ok(())
}
