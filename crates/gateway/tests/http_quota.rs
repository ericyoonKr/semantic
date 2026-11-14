mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::{PASSWORD, ask, drop_schema, fixture, login, new_session};

#[tokio::test]
async fn busy_thread_user_share_usage_and_audit() -> Result<(), Box<dyn std::error::Error>> {
    let Some(f) = fixture().await else { return Ok(()) };
    let _ = login(&f, "grader_a", "wrong password!!").await?;
    let (_, a) = login(&f, "grader_a", PASSWORD).await?;
    let a = a.ok_or("no session")?;
    let first_sid = new_session(&f, &a).await?;
    let other_sid = new_session(&f, &a).await?;

    let slow = ask(&f, &a, &first_sid, "slow").await?;
    assert_eq!(slow.status().as_u16(), 200);
    let queued = ask(&f, &a, &first_sid, "q").await?;
    assert_eq!(queued.status().as_u16(), 200);
    let other = ask(&f, &a, &other_sid, "q").await?;
    assert_eq!(other.status().as_u16(), 200);
    let text = slow.text().await?;
    assert!(text.contains("visible answer"));
    assert!(!text.contains("Usage") && !text.contains("Audit") && !text.contains("1234") && !text.contains("guard_block"));
    let queued_text = queued.text().await?;
    assert!(queued_text.contains("앞 질문의 답을 기다리는 중") && queued_text.contains("visible answer"), "{queued_text}");
    let other_text = other.text().await?;
    assert!(other_text.contains("visible answer"));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(f.store.tokens_today(f.a_id).await?, 3 * 1234);

    f.store.add_tokens(f.a_id, 3_000_000).await?;
    let over = ask(&f, &a, &other_sid, "q").await?;
    assert_eq!(over.status().as_u16(), 429);
    let over_body: Value = over.json().await?;
    assert_eq!(over_body, json!({ "message": "오늘 쓸 수 있는 양을 다 썼습니다. 내일 다시 질문해 주세요" }));

    let removed = f
        .http
        .delete(format!("{}/session/{other_sid}", f.base))
        .header("cookie", &a.cookie)
        .header("x-csrf-token", &a.csrf)
        .send()
        .await?;
    assert_eq!(removed.status().as_u16(), 204);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let kinds = f.audit.kinds_of_user(f.a_id).await?;
    for want in ["login_fail", "login", "ask", "guard_block", "delete"] {
        assert!(kinds.iter().any(|k| k == want), "missing {want} in {kinds:?}");
    }
    assert_eq!(kinds.iter().filter(|k| *k == "ask").count(), 3);
    assert_eq!(kinds.iter().filter(|k| *k == "guard_block").count(), 3);
    drop_schema(&f.pool, &f.schema).await;
    Ok(())
}
