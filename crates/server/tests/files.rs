mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{SID, TestResult, app, call, request, tiny_docx, upload_request};
use sessdb::{Part, doc_key};

#[tokio::test]
async fn missing_or_wrong_internal_key_is_forbidden() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (_state, router) = app(tmp.path()).await?;
    let bare = Request::builder().uri("/health").body(Body::empty())?;
    let (status, body) = call(&router, bare).await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["request_id"].as_str().is_some_and(|s| !s.is_empty()));
    let wrong = Request::builder()
        .uri(format!("/sessions/{SID}/files"))
        .header("x-internal-key", "nope")
        .body(Body::empty())?;
    assert_eq!(call(&router, wrong).await?.0, StatusCode::FORBIDDEN);
    let ok = request("GET", "/health", Body::empty())?;
    assert_eq!(call(&router, ok).await?.0, StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn request_id_is_passed_through_and_errors_carry_it() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (_state, router) = app(tmp.path()).await?;
    let mut req = request("GET", "/sessions/bad%2Fid/files", Body::empty())?;
    req.headers_mut()
        .insert("x-request-id", "req-abc-1".parse()?);
    let res = tower::ServiceExt::oneshot(router.clone(), req).await?;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        res.headers().get("x-request-id").map(|v| v.as_bytes()),
        Some(b"req-abc-1".as_slice())
    );
    assert!(res.headers().get("server").is_none());
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await?;
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(body["request_id"], "req-abc-1");
    assert!(body["error"].as_str().is_some_and(|s| !s.contains("bad")));
    Ok(())
}

#[tokio::test]
async fn docx_upload_returns_paged_text_outline_and_is_stored() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (state, router) = app(tmp.path()).await?;
    let docx = tiny_docx("Test heading", "First test paragraph body")?;
    let (status, body) = call(&router, upload_request(SID, "sample.docx", &docx)?).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["format"], "docx");
    assert_eq!(body["filename"], "sample.docx");
    let text = body["text"].as_str().unwrap_or_default();
    assert!(text.contains("First test paragraph body"), "{text}");
    let outline = body["outline"].as_array().cloned().unwrap_or_default();
    assert!(
        outline.iter().any(|e| e["title"] == "Test heading"),
        "{outline:?}"
    );
    assert_eq!(body["findings"], serde_json::json!([]));
    let file_id = body["file_id"].as_str().unwrap_or_default().to_string();
    assert_eq!(file_id.len(), 64);

    let stored_text = state.db.get_state(&doc_key(SID, &file_id, Part::Text)?)?;
    assert_eq!(stored_text.as_deref(), Some(text.as_bytes()));
    for part in [Part::Meta, Part::Model, Part::Spans, Part::Findings] {
        assert!(
            state
                .db
                .get_state(&doc_key(SID, &file_id, part)?)?
                .is_some(),
            "{part:?}"
        );
    }

    let (status, list) = call(
        &router,
        request("GET", &format!("/sessions/{SID}/files"), Body::empty())?,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["files"][0]["file_id"], file_id.as_str());
    assert_eq!(list["files"][0]["filename"], "sample.docx");

    let del = request("DELETE", &format!("/sessions/{SID}"), Body::empty())?;
    assert_eq!(call(&router, del).await?.0, StatusCode::NO_CONTENT);
    assert!(state.db.list_files(SID)?.is_empty());
    assert!(state.sessions.is_empty());
    Ok(())
}

#[tokio::test]
async fn bad_uploads_get_input_statuses() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (_state, router) = app(tmp.path()).await?;
    let (status, body) = call(&router, upload_request(SID, "a.txt", b"plain text file")?).await?;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{body}");
    let mut hwp = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    hwp.extend([0u8; 512]);
    assert_eq!(
        call(&router, upload_request(SID, "a.hwp", &hwp)?).await?.0,
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    let mut broken = b"PK\x03\x04".to_vec();
    broken.extend([7u8; 64]);
    assert_eq!(
        call(&router, upload_request(SID, "b.docx", &broken)?)
            .await?
            .0,
        StatusCode::BAD_REQUEST
    );
    Ok(())
}

#[tokio::test]
async fn oversized_upload_is_413() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let cfg = common::config(tmp.path(), &[("UPLOAD_MAX_BYTES", "64")])?;
    let state = server::AppState::open(cfg).await?;
    let router = server::http::router(state);
    let docx = tiny_docx("H", "a paragraph long enough to pass sixty four bytes")?;
    let (status, _) = call(&router, upload_request(SID, "big.docx", &docx)?).await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}

#[tokio::test]
async fn idle_actor_stops_and_a_new_one_takes_over() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let cfg = common::config(tmp.path(), &[("SESSION_IDLE_SECS", "1")])?;
    let state = server::AppState::open(cfg).await?;
    let router = server::http::router(state.clone());
    let first = tiny_docx("H1", "first body text")?;
    assert_eq!(
        call(&router, upload_request(SID, "a.docx", &first)?)
            .await?
            .0,
        StatusCode::OK
    );
    assert_eq!(state.sessions.len(), 1);
    tokio::time::sleep(std::time::Duration::from_millis(1600)).await;
    assert!(state.sessions.is_empty());
    let second = tiny_docx("H2", "second body text")?;
    assert_eq!(
        call(&router, upload_request(SID, "b.docx", &second)?)
            .await?
            .0,
        StatusCode::OK
    );
    assert_eq!(state.db.list_files(SID)?.len(), 2);
    Ok(())
}
