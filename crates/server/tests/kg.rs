mod common;

use axum::body::Body;
use axum::http::StatusCode;
use common::{SID, TestResult, app, bundle_lines, call, post_json, request, write_bundle};
use serde_json::json;

#[tokio::test]
async fn find_gives_short_ids_and_expand_resolves_them() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(&tmp.path().join("kg"), "b1", "g", &bundle_lines("g"))?;
    let (state, router) = app(tmp.path()).await?;

    let find_uri = format!("/sessions/{SID}/kg/find");
    let (status, found) =
        post_json(&router, &find_uri, json!({"graph": "g", "text": "alpha"})).await?;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["nodes"][0]["node"], "L1");
    assert_eq!(found["nodes"][0]["name"], "alpha concept");
    assert!(!found.to_string().contains("long-node-id"), "{found}");

    let (_, again) = post_json(&router, &find_uri, json!({"graph": "g", "text": "beta"})).await?;
    assert_eq!(again["nodes"][0]["node"], "L2");
    assert_eq!(again["nodes"][0]["status"]["judgment"], "unknown");

    let expand_uri = format!("/sessions/{SID}/kg/expand");
    let body = json!({"graph": "g", "node": "L1", "direction": "out", "depth": 2});
    let (status, ex) = post_json(&router, &expand_uri, body).await?;
    assert_eq!(status, StatusCode::OK, "{ex}");
    assert_eq!(ex["origin"], "L1");
    let edges = ex["edges"].as_array().cloned().unwrap_or_default();
    assert_eq!(edges.len(), 2, "{ex}");
    assert_eq!(edges[0]["to"], "L2");
    assert_eq!(edges[0]["evidence"][0]["quote"], "quoted line");
    assert_eq!(edges[1]["from"], "L2");
    assert_eq!(edges[1]["to"], "L3");
    assert_eq!(edges[1]["evidence"], "근거 없음");
    assert_eq!(edges[1]["neighbor"]["name"], "gamma group");
    assert!(!ex.to_string().contains("long-node-id"), "{ex}");

    let body = json!({"graph": "g", "node": "L3", "direction": "in"});
    let (status, back) = post_json(&router, &expand_uri, body).await?;
    assert_eq!(status, StatusCode::OK, "{back}");
    assert_eq!(back["edges"][0]["to"], "L2");

    let stored = state.db.get_state(&sessdb::kg_ids_key(SID)?)?;
    assert!(stored.is_some());

    let other = "11111111-2222-4333-8444-555555555555";
    let body = json!({"graph": "g", "node": "L1", "direction": "out"});
    let (status, _) = post_json(&router, &format!("/sessions/{other}/kg/expand"), body).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn short_ids_survive_restart() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(&tmp.path().join("kg"), "b1", "g", &bundle_lines("g"))?;
    {
        let (state, router) = app(tmp.path()).await?;
        let uri = format!("/sessions/{SID}/kg/find");
        let (_, found) = post_json(&router, &uri, json!({"graph": "g", "text": "gamma"})).await?;
        assert_eq!(found["nodes"][0]["node"], "L1");
        server::shutdown::finish(&state, std::time::Duration::from_secs(5)).await;
    }
    let (_state, router) = app(tmp.path()).await?;
    let body = json!({"graph": "g", "node": "L1", "direction": "in"});
    let (status, ex) = post_json(&router, &format!("/sessions/{SID}/kg/expand"), body).await?;
    assert_eq!(status, StatusCode::OK, "{ex}");
    assert_eq!(ex["edges"][0]["neighbor"]["name"], "beta idea");
    Ok(())
}

#[tokio::test]
async fn bad_kg_arguments_are_400() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(&tmp.path().join("kg"), "b1", "g", &bundle_lines("g"))?;
    let (_state, router) = app(tmp.path()).await?;
    let uri = format!("/sessions/{SID}/kg/find");
    for body in [
        json!({"graph": "nope", "text": "alpha"}),
        json!({"graph": "g"}),
        json!({"graph": "g", "law": "x", "unit": "y"}),
        json!({"graph": "g", "text": "alpha", "limit": 21}),
    ] {
        let (status, reply) = post_json(&router, &uri, body.clone()).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body} -> {reply}");
    }
    let (_, _) = post_json(&router, &uri, json!({"graph": "g", "text": "alpha"})).await?;
    let expand = format!("/sessions/{SID}/kg/expand");
    for body in [
        json!({"graph": "g", "node": "L9", "direction": "out"}),
        json!({"graph": "g", "node": "L1", "direction": "out", "depth": 3}),
        json!({"graph": "g", "node": "L1", "direction": "out", "relation": "rel_q"}),
    ] {
        let (status, reply) = post_json(&router, &expand, body.clone()).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body} -> {reply}");
    }
    Ok(())
}

#[tokio::test]
async fn disabled_bundle_does_not_stop_startup() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let kg_dir = tmp.path().join("kg");
    let mut broken = bundle_lines("bad");
    broken.remove(2);
    write_bundle(&kg_dir, "a_bad", "bad", &broken)?;
    write_bundle(&kg_dir, "b_good", "good", &bundle_lines("good"))?;
    let (state, router) = app(tmp.path()).await?;
    assert_eq!(state.kg.disabled().len(), 1);
    assert_eq!(state.kg.disabled()[0].bundle, "a_bad");
    let (status, catalog) = call(&router, request("GET", "/kg/catalog", Body::empty())?).await?;
    assert_eq!(status, StatusCode::OK);
    let graphs = catalog["graphs"].as_array().cloned().unwrap_or_default();
    assert_eq!(graphs.len(), 1, "{catalog}");
    assert_eq!(graphs[0]["graph"], "good");
    assert_eq!(graphs[0]["nodes"], 3);
    Ok(())
}
