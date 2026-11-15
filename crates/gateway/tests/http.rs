mod support;

use std::time::Duration;

use axum::http::StatusCode;
use gateway::GatewayError;
use serde_json::{Value, json};
use support::{PASSWORD, admin, drop_schema, fixture, login, new_session, signup};

#[tokio::test]
async fn login_owner_check_csrf_and_filtered_stream() -> Result<(), Box<dyn std::error::Error>> {
    let Some(f) = fixture().await else { return Ok(()) };
    let (bad, none) = login(&f, "grader_a", "wrong password!!").await?;
    assert_eq!(bad, StatusCode::UNAUTHORIZED);
    assert!(none.is_none());
    let (ok, a) = login(&f, "grader_a", PASSWORD).await?;
    assert_eq!(ok, StatusCode::OK);
    let a = a.ok_or("no session")?;
    assert!(a.cookie.starts_with("__Host-sid="));

    let anon = f.http.get(format!("{}/api/sessions", f.base)).send().await?;
    assert_eq!(anon.status().as_u16(), 401);

    let no_csrf = f.http.post(format!("{}/session", f.base)).header("cookie", &a.cookie).send().await?;
    assert_eq!(no_csrf.status().as_u16(), 403);

    let created: Value = f
        .http
        .post(format!("{}/session", f.base))
        .header("cookie", &a.cookie)
        .header("x-csrf-token", &a.csrf)
        .send()
        .await?
        .json()
        .await?;
    let sid = created.get("session_id").and_then(Value::as_str).ok_or("no session id")?.to_string();

    let listed: Value = f.http.get(format!("{}/api/sessions", f.base)).header("cookie", &a.cookie).send().await?.json().await?;
    assert_eq!(listed, json!([{ "session_id": sid }]));

    let res = f
        .http
        .post(format!("{}/session/{sid}/ask", f.base))
        .header("cookie", &a.cookie)
        .header("x-csrf-token", &a.csrf)
        .json(&json!({ "question": "q" }))
        .send()
        .await?;
    assert_eq!(res.status().as_u16(), 200);
    assert_eq!(res.headers().get("x-content-type-options").and_then(|v| v.to_str().ok()), Some("nosniff"));
    let stream = res.text().await?;
    assert!(stream.contains("visible answer"));
    assert!(!stream.contains("internal plan"));
    assert!(!stream.contains("search_laws"));

    let (_, b) = login(&f, "grader_b", PASSWORD).await?;
    let b = b.ok_or("no session")?;
    let foreign = f
        .http
        .post(format!("{}/session/{sid}/ask", f.base))
        .header("cookie", &b.cookie)
        .header("x-csrf-token", &b.csrf)
        .json(&json!({ "question": "q" }))
        .send()
        .await?;
    assert_eq!(foreign.status().as_u16(), 404);

    let me: Value = f.http.get(format!("{}/api/auth/me", f.base)).header("cookie", &a.cookie).send().await?.json().await?;
    assert_eq!(me, json!({ "login_id": "grader_a", "name": "", "is_master": false }));
    let out = f
        .http
        .post(format!("{}/api/auth/logout", f.base))
        .header("cookie", &a.cookie)
        .header("x-csrf-token", &a.csrf)
        .send()
        .await?;
    assert_eq!(out.status().as_u16(), 204);
    let after = f.http.get(format!("{}/api/auth/me", f.base)).header("cookie", &a.cookie).send().await?;
    assert_eq!(after.status().as_u16(), 401);

    drop_schema(&f.pool, &f.schema).await;
    Ok(())
}

#[tokio::test]
async fn retry_streams_a_new_answer_through_the_same_filter() -> Result<(), Box<dyn std::error::Error>> {
    let Some(f) = fixture().await else { return Ok(()) };
    let (_, a) = login(&f, "grader_a", PASSWORD).await?;
    let a = a.ok_or("no session")?;
    let sid = new_session(&f, &a).await?;
    let res = f.http.post(format!("{}/session/{sid}/retry", f.base)).header("cookie", &a.cookie).header("x-csrf-token", &a.csrf).send().await?;
    assert_eq!(res.status().as_u16(), 200);
    let stream = res.text().await?;
    assert!(stream.contains("visible answer"));
    assert!(!stream.contains("internal plan"));
    let no_csrf = f.http.post(format!("{}/session/{sid}/retry", f.base)).header("cookie", &a.cookie).send().await?;
    assert_eq!(no_csrf.status().as_u16(), 403);
    drop_schema(&f.pool, &f.schema).await;
    Ok(())
}

#[tokio::test]
async fn eleventh_login_in_a_minute_is_refused() -> Result<(), GatewayError> {
    let Some(f) = fixture().await else { return Ok(()) };
    let mut statuses = Vec::new();
    for _ in 0..11 {
        let (status, _) = login(&f, "grader_a", "wrong password!!").await.map_err(|_| GatewayError::Internal)?;
        statuses.push(status);
    }
    assert!(statuses[..10].iter().all(|s| *s == StatusCode::UNAUTHORIZED));
    assert_eq!(statuses[10], StatusCode::TOO_MANY_REQUESTS);
    drop_schema(&f.pool, &f.schema).await;
    Ok(())
}

#[tokio::test]
async fn signup_waits_for_master_approval() -> Result<(), Box<dyn std::error::Error>> {
    let Some(f) = fixture().await else { return Ok(()) };
    let (status, body) = signup(&f, "newbie_1", "eight888", " 신입 하나 ").await?;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body, json!({ "login_id": "newbie_1", "status": "pending" }));
    let (dup, body) = signup(&f, "newbie_1", "eight888", "신입 하나").await?;
    assert_eq!(dup, StatusCode::CONFLICT);
    assert_eq!(body, json!({ "message": "이미 있는 아이디입니다" }));
    for (id, pw, name) in [("abc", "eight888", "짧은 아이디"), ("Upper_1", "eight888", "대문자"), ("newbie_2", "seven77", "짧은 비밀번호"), ("newbie_2", "eight888", "   ")] {
        let (bad, _) = signup(&f, id, pw, name).await?;
        assert_eq!(bad, StatusCode::BAD_REQUEST, "{id} {pw} {name}");
    }

    let pending = f
        .http
        .post(format!("{}/api/auth/login", f.base))
        .json(&json!({ "login_id": "newbie_1", "password": "eight888" }))
        .send()
        .await?;
    assert_eq!(pending.status().as_u16(), 403);
    let pending_body: Value = pending.json().await?;
    assert_eq!(pending_body.get("error"), Some(&json!("승인 대기 중입니다")));
    let (wrong, _) = login(&f, "newbie_1", "wrong password!!").await?;
    assert_eq!(wrong, StatusCode::UNAUTHORIZED);

    let (_, a) = login(&f, "grader_a", PASSWORD).await?;
    let a = a.ok_or("no session")?;
    let denied = f.http.get(format!("{}/api/admin/users", f.base)).header("cookie", &a.cookie).send().await?;
    assert_eq!(denied.status().as_u16(), 403);
    assert_eq!(admin(&f, &a, "approve", "newbie_1").await?.status().as_u16(), 403);
    let (still, _) = login(&f, "newbie_1", "eight888").await?;
    assert_eq!(still, StatusCode::FORBIDDEN);

    assert!(f.store.set_master("grader_a").await?);
    let me: Value = f.http.get(format!("{}/api/auth/me", f.base)).header("cookie", &a.cookie).send().await?.json().await?;
    assert_eq!(me, json!({ "login_id": "grader_a", "name": "", "is_master": true }));
    let listed: Value = f.http.get(format!("{}/api/admin/users", f.base)).header("cookie", &a.cookie).send().await?.json().await?;
    let first = listed.get(0).ok_or("empty list")?;
    assert_eq!(first.get("login_id"), Some(&json!("newbie_1")));
    assert_eq!(first.get("name"), Some(&json!("신입 하나")));
    assert_eq!(first.get("status"), Some(&json!("pending")));
    assert!(first.get("requested_at").and_then(Value::as_str).is_some_and(|t| t.ends_with('Z')));
    assert_eq!(listed.as_array().map(Vec::len), Some(3));

    assert_eq!(admin(&f, &a, "approve", "newbie_1").await?.status().as_u16(), 204);
    let (ok, n) = login(&f, "newbie_1", "eight888").await?;
    assert_eq!(ok, StatusCode::OK);
    let n = n.ok_or("no session")?;
    let me: Value = f.http.get(format!("{}/api/auth/me", f.base)).header("cookie", &n.cookie).send().await?.json().await?;
    assert_eq!(me, json!({ "login_id": "newbie_1", "name": "신입 하나", "is_master": false }));
    let denied = f.http.get(format!("{}/api/admin/users", f.base)).header("cookie", &n.cookie).send().await?;
    assert_eq!(denied.status().as_u16(), 403);

    let (status, _) = signup(&f, "newbie_2", "eight888", "신입 둘").await?;
    assert_eq!(status, StatusCode::CREATED);
    let second = f.store.user_id("newbie_2").await?.ok_or("no newbie_2")?;
    assert_eq!(admin(&f, &a, "reject", "newbie_2").await?.status().as_u16(), 204);
    let (gone, _) = login(&f, "newbie_2", "eight888").await?;
    assert_eq!(gone, StatusCode::UNAUTHORIZED);
    assert_eq!(admin(&f, &a, "reject", "newbie_2").await?.status().as_u16(), 404);

    assert_eq!(admin(&f, &a, "disable", "grader_a").await?.status().as_u16(), 400);
    assert_eq!(admin(&f, &a, "reject", "grader_a").await?.status().as_u16(), 400);
    assert_eq!(admin(&f, &a, "disable", "newbie_1").await?.status().as_u16(), 204);
    let (off, _) = login(&f, "newbie_1", "eight888").await?;
    assert_eq!(off, StatusCode::UNAUTHORIZED);
    let kicked = f.http.get(format!("{}/api/auth/me", f.base)).header("cookie", &n.cookie).send().await?;
    assert_eq!(kicked.status().as_u16(), 401);

    tokio::time::sleep(Duration::from_millis(300)).await;
    let first_id = f.store.user_id("newbie_1").await?.ok_or("no newbie_1")?;
    let kinds = f.audit.kinds_of_user(first_id).await?;
    for want in ["signup", "login_fail", "approve", "login", "disable"] {
        assert!(kinds.iter().any(|k| k == want), "missing {want} in {kinds:?}");
    }
    assert_eq!(f.audit.kinds_of_user(second).await?, vec!["signup".to_string(), "reject".to_string()]);
    drop_schema(&f.pool, &f.schema).await;
    Ok(())
}

#[tokio::test]
async fn eleventh_signup_in_an_hour_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let Some(f) = fixture().await else { return Ok(()) };
    let mut statuses = Vec::new();
    for _ in 0..11 {
        let (status, _) = signup(&f, "abc", "eight888", "짧은 아이디").await?;
        statuses.push(status);
    }
    assert!(statuses[..10].iter().all(|s| *s == StatusCode::BAD_REQUEST));
    assert_eq!(statuses[10], StatusCode::TOO_MANY_REQUESTS);
    drop_schema(&f.pool, &f.schema).await;
    Ok(())
}
