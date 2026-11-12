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
    let annex = [
        ("/annex/lookup", json!({"law": " ", "annex": "별표 1"})),
        ("/annex/lookup", json!({"law": "가법", "annex": "별표 1", "limit": 0})),
        ("/annex/lookup", json!({"law": "가법"})),
        ("/annex/compute", json!({"node_id": ""})),
        ("/annex/compute", json!({"node_id": "x", "elapsed": -1})),
    ];
    for (uri, body) in annex {
        let (status, reply) = post(&router, uri, body.clone()).await?;
        check(status == StatusCode::BAD_REQUEST, format!("{uri} {body} -> {status} {reply}"))?;
        check(reason(&reply).starts_with("인자 오류"), format!("{reply}"))?;
    }
    Ok(())
}

async fn database_checks(router: &Router) -> Checked {
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
    check(hits[0]["disposition"] == "reject" && hits[0]["en_banc"] == false, format!("{cases}"))?;
    let (status, keyed) = post(
        router,
        "/search/cases",
        json!({"query": "전혀없는낱말", "article_keys": ["L2:0003"], "include_minority": false}),
    )
    .await?;
    check(status == StatusCode::OK && keyed["hits"][0]["matched"] == "cited", format!("{status} {keyed}"))?;

    let uri = format!("/cases/10?sections={}", encode("이유,holding_summary"));
    let (status, case) = get(router, &uri).await?;
    check(status == StatusCode::OK, format!("{status} {case}"))?;
    check(case["court"] == "대법원" && case["case_no"] == "2020다1111", format!("{case}"))?;
    check(case["decided_on"] == "2021-03-04" && case["field"] == "civil", format!("{case}"))?;
    let sections = case["sections"].as_array().cloned().unwrap_or_default();
    check(sections.len() == 2, format!("{case}"))?;
    check(sections[1]["role"] == "reasons" && sections[1]["heading"] == "이유", format!("{case}"))?;
    let (status, all) = get(router, "/cases/10?include_minority=true").await?;
    check(status == StatusCode::OK, format!("{status} {all}"))?;
    let opinions: Vec<Value> = all["sections"].as_array().cloned().unwrap_or_default().iter().map(|s| s["opinion"].clone()).collect();
    check(opinions.iter().any(|o| o == "dissent"), format!("{all}"))?;
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
        let case = snap.hits.iter().find(|h| h.source == "case").ok_or("no case hit")?;
        check(case.citation.contains("판결"), case.citation.clone())?;
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

async fn live_checks(router: &Router) -> Checked {
    let (status, laws) = post(
        router,
        "/search/laws",
        json!({"query": "지방공무원법 제25조의2", "k": 3, "as_of": "2022-01-01"}),
    )
    .await?;
    check(status == StatusCode::OK, format!("{status} {laws}"))?;
    check(laws["degraded"] == true && laws["degraded_reason"].is_string(), format!("{laws}"))?;
    let hit = &laws["hits"][0];
    check(hit["law_name"] == "지방공무원법" && hit["unit_key"] == "제25조의2", format!("{hit}"))?;
    check(hit["matched"] == "direct" && hit["version_id"] == "001653:0025021@2021-12-09", format!("{hit}"))?;

    let uri = format!("/laws/unit?law={}&unit={}", encode("지방공무원법"), encode("제25조의2"));
    let (status, unit) = get(router, &uri).await?;
    check(status == StatusCode::OK, format!("{status} {unit}"))?;
    check(unit["title"] == "외국인과 복수국적자의 임용" && unit["unit_kind"] == "article", format!("{unit}"))?;
    check(unit["version"]["valid_to"].is_null(), format!("{unit}"))?;

    let law = "산업기술의 유출방지 및 보호에 관한 법률 시행령";
    let (status, table) = post(
        router,
        "/annex/lookup",
        json!({"law": law, "annex": "별표 2", "as_of": "2025-10-02", "words": "국가 핵심기술 보유기관 등록"}),
    )
    .await?;
    check(status == StatusCode::OK, format!("{status} {table}"))?;
    check(table["rows_matched"] == 1, format!("{table}"))?;
    let node = table["rows"][0]["node_id"].as_str().unwrap_or_default().to_string();
    let (status, out) = post(router, "/annex/compute", json!({"node_id": node, "column": "2차"})).await?;
    check(status == StatusCode::OK, format!("{status} {out}"))?;
    check(out["results"][0]["krw"] == 7_000_000, format!("{out}"))?;
    check(out["results"].as_array().map(Vec::len) == Some(1), format!("{out}"))?;
    let (status, missing) = post(router, "/annex/lookup", json!({"law": law, "annex": "별표 99"})).await?;
    check(status == StatusCode::BAD_REQUEST, format!("{status} {missing}"))?;
    Ok(())
}

#[tokio::test]
async fn law_and_annex_routes_read_the_live_corpus() -> Checked {
    let Ok(url) = std::env::var("CORPUS_PG_URL") else {
        eprintln!("CORPUS_PG_URL 없음 — 실제 법령 DB 시험을 건너뜀");
        return Ok(());
    };
    let (_tmp, router) = router_with(&[("CORPUS_PG_URL", url.as_str())]).await?;
    live_checks(&router).await
}
