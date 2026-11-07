mod support;

use axum::http::StatusCode;
use gateway::GatewayError;
use serde_json::{Value, json};
use support::{PASSWORD, drop_schema, fixture, login};

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
    assert_eq!(me, json!({ "login_id": "grader_a" }));
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
