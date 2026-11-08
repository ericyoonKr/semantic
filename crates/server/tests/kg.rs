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
    assert_eq!(found["status"], "found");
    assert_eq!(found["nodes"][0]["node"], "L1");
    assert!(found["nodes"][0].get("status").is_none(), "{found}");
    assert_eq!(found["nodes"][0]["name"], "alpha concept");
    assert!(!found.to_string().contains("long-node-id"), "{found}");

    let (_, again) = post_json(&router, &find_uri, json!({"graph": "g", "text": "beta"})).await?;
    assert_eq!(again["nodes"][0]["node"], "L2");
    assert_eq!(again["nodes"][0]["status"]["judgment"], "unknown");

    let expand_uri = format!("/sessions/{SID}/kg/expand");
    let body = json!({"graph": "g", "node": "L1", "direction": "out", "depth": 2});
    let (status, ex) = post_json(&router, &expand_uri, body).await?;
    assert_eq!(status, StatusCode::OK, "{ex}");
    assert_eq!(ex["status"], "found");
    assert_eq!(ex["composition_mode"], "chain_only");
    assert_eq!(ex["refused_paths"], 0);
    assert_eq!(ex["origin"], "L1");
    let edges = ex["edges"].as_array().cloned().unwrap_or_default();
    assert_eq!(edges.len(), 2, "{ex}");
    assert_eq!(edges[0]["to"], "L2");
    assert_eq!(edges[0]["evidence"][0]["quote"], "quoted line");
    assert_eq!(edges[0]["evidence"][0]["kind"], "evidence");
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
        json!({"graph": "g"}),
        json!({"graph": "g", "law": "x"}),
        json!({"graph": "g", "text": "alpha", "limit": 21}),
        json!({"graph": "g", "text": "alpha", "as_of": "2024-13-01"}),
    ] {
        let (status, reply) = post_json(&router, &uri, body.clone()).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body} -> {reply}");
    }
    let (_, _) = post_json(&router, &uri, json!({"graph": "g", "text": "alpha"})).await?;
    let expand = format!("/sessions/{SID}/kg/expand");
    for body in [
        json!({"graph": "g", "node": "L9", "direction": "out"}),
        json!({"graph": "g", "node": "L1", "direction": "out", "depth": 3}),
    ] {
        let (status, reply) = post_json(&router, &expand, body.clone()).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body} -> {reply}");
    }
    Ok(())
}

#[tokio::test]
async fn statuses_reach_the_reply_instead_of_http_errors() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(&tmp.path().join("kg"), "b1", "g", &bundle_lines("g"))?;
    let (_state, router) = app(tmp.path()).await?;
    let find = format!("/sessions/{SID}/kg/find");
    let expand = format!("/sessions/{SID}/kg/expand");
    let (_, first) = post_json(&router, &find, json!({"graph": "g", "text": "alpha"})).await?;
    assert_eq!(first["nodes"][0]["node"], "L1");
    let cases = [
        (
            &find,
            json!({"graph": "nope", "text": "alpha"}),
            "out_of_scope",
        ),
        (
            &find,
            json!({"graph": "g", "law": "x", "unit": "y"}),
            "out_of_scope",
        ),
        (
            &find,
            json!({"graph": "g", "text": "nothing here"}),
            "unknown",
        ),
        (&find, json!({"graph": "g", "text": "gamma"}), "found"),
        (
            &expand,
            json!({"graph": "g", "node": "L1", "direction": "out", "relation": "rel_q"}),
            "out_of_scope",
        ),
        (
            &expand,
            json!({"graph": "g", "node": "L1", "direction": "in", "as_of": "2024-01-01"}),
            "not_examined",
        ),
    ];
    for (uri, body, want) in cases {
        let (status, reply) = post_json(&router, uri, body.clone()).await?;
        assert_eq!(status, StatusCode::OK, "{body} -> {reply}");
        assert_eq!(reply["status"], want, "{body} -> {reply}");
        if want != "found" {
            assert!(
                reply["reason"].as_str().is_some_and(|r| !r.is_empty()),
                "{reply}"
            );
        }
    }
    let (_, ex) = post_json(
        &router,
        &expand,
        json!({"graph": "g", "node": "L1", "direction": "out", "as_of": "2024-01-01"}),
    )
    .await?;
    assert_eq!(ex["status"], "found", "{ex}");
    assert_eq!(ex["as_of"]["not_checked"], 1, "{ex}");
    assert_eq!(ex["edges"][0]["period"], "not_checked", "{ex}");
    Ok(())
}

#[tokio::test]
async fn short_ids_from_an_older_bundle_are_kept_or_sent_back_to_find() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let kg_dir = tmp.path().join("kg");
    write_bundle(&kg_dir, "b1", "g", &bundle_lines("g"))?;
    {
        let (state, router) = app(tmp.path()).await?;
        let uri = format!("/sessions/{SID}/kg/find");
        let (_, a) = post_json(&router, &uri, json!({"graph": "g", "text": "alpha"})).await?;
        let (_, b) = post_json(&router, &uri, json!({"graph": "g", "text": "beta"})).await?;
        assert_eq!(
            (a["nodes"][0]["node"].clone(), b["nodes"][0]["node"].clone()),
            (json!("L1"), json!("L2"))
        );
        server::shutdown::finish(&state, std::time::Duration::from_secs(5)).await;
    }
    let renamed: Vec<String> = bundle_lines("g")
        .into_iter()
        .map(|l| l.replace("long-node-id-0001", "long-node-id-0009"))
        .collect();
    write_bundle(&kg_dir, "b1", "g", &renamed)?;
    let (_state, router) = app(tmp.path()).await?;
    let expand = format!("/sessions/{SID}/kg/expand");
    let (status, gone) = post_json(
        &router,
        &expand,
        json!({"graph": "g", "node": "L1", "direction": "out"}),
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["status"], "unknown");
    assert!(
        gone["reason"]
            .as_str()
            .is_some_and(|r| r.contains("kg_find")),
        "{gone}"
    );
    let (_, kept) = post_json(
        &router,
        &expand,
        json!({"graph": "g", "node": "L2", "direction": "out"}),
    )
    .await?;
    assert_eq!(kept["status"], "found", "{kept}");
    assert_eq!(kept["edges"][0]["neighbor"]["name"], "gamma group");
    Ok(())
}

#[tokio::test]
async fn state_check_accepts_only_declared_values_of_known_nodes() -> TestResult {
    let tmp = tempfile::tempdir()?;
    write_bundle(&tmp.path().join("kg"), "b1", "g", &bundle_lines("g"))?;
    let (_state, router) = app(tmp.path()).await?;
    let (_, _) = post_json(
        &router,
        &format!("/sessions/{SID}/kg/find"),
        json!({"graph": "g", "text": "alpha"}),
    )
    .await?;
    let uri = format!("/sessions/{SID}/kg/state-check");
    let (status, ok) = post_json(
        &router,
        &uri,
        json!({"graph": "g", "node": "L1", "value": "해당"}),
    )
    .await?;
    assert_eq!(status, StatusCode::OK, "{ok}");
    assert_eq!(ok["accepted"], true, "{ok}");
    assert_eq!(ok["name"], "alpha concept");
    assert_eq!(ok["state_kind"], "note");
    assert_eq!(ok["sha256"].as_str().map(str::len), Some(64));
    let (_, bad) = post_json(
        &router,
        &uri,
        json!({"graph": "g", "node": "L1", "value": "maybe"}),
    )
    .await?;
    assert_eq!(bad["accepted"], false, "{bad}");
    assert!(
        bad["reason"]
            .as_str()
            .is_some_and(|r| r.contains("해당 아님")),
        "{bad}"
    );
    let (status, _) = post_json(
        &router,
        &uri,
        json!({"graph": "g", "node": "L7", "value": "해당"}),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
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
    assert_eq!(graphs[0]["compositions"], 1);
    assert_eq!(graphs[0]["relations"], json!(["rel_x"]));
    assert_eq!(graphs[0]["state"]["values"], json!(["해당", "해당 아님"]));
    assert_eq!(catalog["disabled"][0]["bundle"], "a_bad", "{catalog}");
    Ok(())
}
