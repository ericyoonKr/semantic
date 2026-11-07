mod common;

#[path = "../../retrieval/tests/fixture/mod.rs"]
mod fixture;

use axum::Router;
use axum::body::Body;
use axum::http::StatusCode;
use common::{Boxed, call, config, post_json, request};
use fixture::{Checked, Failure, check};
use serde_json::{Value, json};
use server::AppState;

fn lift<T>(r: Boxed<T>) -> Result<T, Failure> {
    r.map_err(|e| e.to_string().into())
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

async fn get(router: &Router, uri: &str) -> Result<(StatusCode, Value), Failure> {
    lift(call(router, lift(request("GET", uri, Body::empty()))?).await)
}

async fn post(router: &Router, uri: &str, body: Value) -> Result<(StatusCode, Value), Failure> {
    lift(post_json(router, uri, body).await)
}

async fn router_with(extra: &[(&str, &str)]) -> Result<(tempfile::TempDir, Router), Failure> {
    let tmp = tempfile::tempdir()?;
    let cfg = lift(config(tmp.path(), extra))?;
    let state = AppState::open(cfg).await?;
    Ok((tmp, server::http::router(state)))
}

fn reason(v: &Value) -> String {
    v["error"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn without_corpus_database_search_reports_failure_not_crash() -> Checked {
    let (_tmp, router) = router_with(&[]).await?;
    let (status, body) = post(&router, "/search/laws", json!({"query": "국가배상"})).await?;
    check(status == StatusCode::INTERNAL_SERVER_ERROR, format!("{status} {body}"))?;
    check(reason(&body).starts_with("검색 실패"), format!("{body}"))?;
    check(body["request_id"].is_string(), "request id attached")?;
    let (status, body) = get(&router, "/cases/10").await?;
    check(status == StatusCode::INTERNAL_SERVER_ERROR, format!("{status} {body}"))?;
    check(reason(&body).starts_with("검색 실패"), format!("{body}"))?;
    Ok(())
}

#[tokio::test]
async fn bad_arguments_are_400_before_touching_the_database() -> Checked {
    let (_tmp, router) = router_with(&[]).await?;
    let cases = [
        ("/search/laws", json!({"query": "  "})),
        ("/search/laws", json!({"query": "가나", "k": 0})),
        ("/search/laws", json!({"query": "가나", "k": 81})),
        ("/search/laws", json!({"query": "가나", "as_of": "2024-13-01"})),
        ("/search/cases", json!({"query": "가나", "decided_from": "어제"})),
        ("/search/cases", json!({"k": 3})),
    ];
    for (uri, body) in cases {
        let (status, reply) = post(&router, uri, body.clone()).await?;
        check(status == StatusCode::BAD_REQUEST, format!("{uri} {body} -> {status} {reply}"))?;
        check(reason(&reply).starts_with("인자 오류"), format!("{reply}"))?;
    }
    for uri in [
        "/cases/abc",
        "/citations",
        "/citations?law=%EA%B0%80",
        "/laws/unit?law=x",
        "/laws/unit?law=x&unit=y&as_of=bad",
    ] {
        let (status, reply) = get(&router, uri).await?;
        check(status == StatusCode::BAD_REQUEST, format!("{uri} -> {status} {reply}"))?;
    }
    Ok(())
}

async fn database_checks(router: &Router) -> Checked {
    let (status, laws) = post(
        router,
        "/search/laws",
        json!({"query": "공무원 직무 과실 손해", "k": 3, "as_of": null, "law_type": null}),
    )
    .await?;
    check(status == StatusCode::OK, format!("{status} {laws}"))?;
    check(laws["degraded"] == true, "no embedding server configured means degraded")?;
    let hit = &laws["hits"][0];
    check(hit["law_name"] == "국가배상법" && hit["unit_key"] == "제2조", format!("{hit}"))?;
    check(hit["enforcement_date"] == "2023-01-01", format!("{hit}"))?;
    check(hit["kind"] == "law" && hit["lsi_seq"] == fixture::CURRENT_SEQ, format!("{hit}"))?;
    check(hit["score"].as_f64().unwrap_or(0.0) > 0.0, "score")?;

    let (status, cases) = post(
        router,
        "/search/cases",
        json!({"query": "공무원 과실", "k": 5, "field": [], "court_level": ["high"],
               "decided_from": "2023-01-01", "decided_to": null}),
    )
    .await?;
    check(status == StatusCode::OK, format!("{status} {cases}"))?;
    let hits = cases["hits"].as_array().cloned().unwrap_or_default();
    check(hits.len() == 1, format!("{cases}"))?;
    check(hits[0]["decision_id"] == 11 && hits[0]["court"] == "서울고등법원", format!("{cases}"))?;
    check(hits[0]["decided_on"] == "2023-05-06", format!("{cases}"))?;
    check(hits[0]["section_role"] == "holding_summary", format!("{cases}"))?;

    let uri = format!("/laws/unit?law={}&unit={}", encode("국가배상법"), encode("제2조"));
    let (status, unit) = get(router, &uri).await?;
    check(status == StatusCode::OK, format!("{status} {unit}"))?;
    check(unit["body"] == fixture::ART2_NOW, format!("{unit}"))?;
    check(unit["deleted"] == false && unit["content_status"] == "text", format!("{unit}"))?;
    check(unit["source_urls"] == json!([]), format!("{unit}"))?;
    check(
        unit["version"] == json!({"kind": "law", "lsi_seq": fixture::CURRENT_SEQ, "status": "current"}),
        format!("{unit}"),
    )?;
    let uri = format!(
        "/laws/unit?law={}&unit={}&as_of=2019-01-01",
        encode("국가배상법"),
        encode("제2조")
    );
    let (_, old) = get(router, &uri).await?;
    check(old["body"] == fixture::ART2_OLD, format!("{old}"))?;
    let uri = format!("/laws/unit?law={}&unit={}", encode("국가배상법"), encode("제99조"));
    let (status, missing) = get(router, &uri).await?;
    check(status == StatusCode::BAD_REQUEST, format!("{status} {missing}"))?;

    let uri = format!("/cases/10?sections={}", encode("이유,holding_summary"));
    let (status, case) = get(router, &uri).await?;
    check(status == StatusCode::OK, format!("{status} {case}"))?;
    check(case["court"] == "대법원" && case["case_no"] == "2020다1111", format!("{case}"))?;
    check(case["decided_on"] == "2021-03-04" && case["field"] == "civil", format!("{case}"))?;
    let sections = case["sections"].as_array().cloned().unwrap_or_default();
    check(sections.len() == 2, format!("{case}"))?;
    check(sections[1]["role"] == "reasons" && sections[1]["heading"] == "이유", format!("{case}"))?;
    let (status, _) = get(router, "/cases/999").await?;
    check(status == StatusCode::BAD_REQUEST, "missing case is 400")?;

    let (status, cited) = get(router, "/citations?decision_id=11").await?;
    check(status == StatusCode::OK, format!("{status} {cited}"))?;
    check(cited["extract_run"] == "2:v2", format!("{cited}"))?;
    check(cited["items"].as_array().map(Vec::len) == Some(1), format!("{cited}"))?;
    let uri = format!("/citations?law={}&unit={}", encode("국가배상법"), encode("제2조"));
    let (status, citing) = get(router, &uri).await?;
    check(status == StatusCode::OK, format!("{status} {citing}"))?;
    check(citing["items"][0]["decision_id"] == 11, format!("{citing}"))?;
    check(citing["items"][1]["decision_id"] == 10, format!("{citing}"))?;
    Ok(())
}

#[tokio::test]
async fn routes_return_the_shapes_the_agent_reads() -> Checked {
    let Some(fx) = fixture::open().await? else {
        return Ok(());
    };
    let result = async {
        let (_tmp, router) = router_with(&[("CORPUS_PG_URL", fx.url.as_str())]).await?;
        database_checks(&router).await
    }
    .await;
    fx.close().await?;
    result
}

#[tokio::test]
async fn judge_authority_search_reads_the_corpus_or_reports_unavailable() -> Checked {
    use engine::{AuthoritySearch, DepError};
    use server::deps::CorpusAuthority;
    let missing = CorpusAuthority::new(None).search("국가배상", 4).await;
    check(matches!(missing, Err(DepError::Unavailable(_))), format!("{missing:?}"))?;
    let Some(fx) = fixture::open().await? else {
        return Ok(());
    };
    let result = async {
        let pool = retrieval::CorpusDb::parse(&fx.url)?.connect_lazy(4, std::time::Duration::from_secs(10));
        let corpus = retrieval::Corpus::new(pool, std::sync::Arc::new(retrieval::NoEmbedder));
        let snap = CorpusAuthority::new(Some(corpus)).search("손해 배상", 4).await.map_err(|e| format!("{e:?}"))?;
        check(snap.degraded, "no embedder means degraded")?;
        check(snap.hits.len() <= 4 && !snap.hits.is_empty(), format!("{:?}", snap.hits))?;
        let law = snap.hits.iter().find(|h| h.source == "law").ok_or("no law hit")?;
        check(law.citation.starts_with("국가배상법 제"), law.citation.clone())?;
        check(snap.snapshot_hash.len() == 64, snap.snapshot_hash.clone())?;
        let again = CorpusAuthority::new(Some(retrieval::Corpus::new(
            retrieval::CorpusDb::parse(&fx.url)?.connect_lazy(4, std::time::Duration::from_secs(10)),
            std::sync::Arc::new(retrieval::NoEmbedder),
        )))
        .search("손해 배상", 4)
        .await
        .map_err(|e| format!("{e:?}"))?;
        check(again.snapshot_hash == snap.snapshot_hash, "same results give the same hash")?;
        Ok::<(), Failure>(())
    }
    .await;
    fx.close().await?;
    result
}
