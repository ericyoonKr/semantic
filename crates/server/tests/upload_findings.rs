mod common;

use std::io::{Cursor, Write};

use axum::http::StatusCode;
use common::{Boxed, SID, TestResult, app, call, upload_request};
use sessdb::{Part, doc_key};
use zip::write::{SimpleFileOptions, ZipWriter};

const W_NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";

fn docx(paras: &[&str]) -> Boxed<Vec<u8>> {
    let body: String = paras
        .iter()
        .map(|t| format!("<w:p><w:r><w:t xml:space=\"preserve\">{t}</w:t></w:r></w:p>"))
        .collect();
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {W_NS}><w:body>{body}</w:body></w:document>"
    );
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer.start_file("[Content_Types].xml", options)?;
    writer.write_all(b"<?xml version=\"1.0\"?><Types/>")?;
    writer.start_file("word/document.xml", options)?;
    writer.write_all(document.as_bytes())?;
    Ok(writer.finish()?.into_inner())
}

#[tokio::test]
async fn upload_fills_findings_from_the_deterministic_checks() -> TestResult {
    let tmp = tempfile::tempdir()?;
    let (state, router) = app(tmp.path()).await?;
    let bytes = docx(&[
        "제1조(목적) 이 계약은 용역의 범위를 정한다.",
        "제2조(기간) 계약기간은 1년으로 한다.",
        "제3조(대금) 대금은 매월 말일에 지급한다.",
        "제4조(해지) 제9조의 사유가 있으면 해지할 수 있다.",
    ])?;
    let (status, body) = call(&router, upload_request(SID, "c.docx", &bytes)?).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let findings = body["findings"].as_array().cloned().unwrap_or_default();
    assert_eq!(findings.len(), 1, "{body}");
    assert_eq!(findings[0]["kind"], "orphan_reference");
    assert!(findings[0]["message"].as_str().is_some_and(|m| m.contains("제9조")));
    let file_id = body["file_id"].as_str().unwrap_or_default();
    let stored = state.db.get_state(&doc_key(SID, file_id, Part::Findings)?)?;
    let stored: serde_json::Value = serde_json::from_slice(&stored.unwrap_or_default())?;
    assert_eq!(stored, body["findings"]);

    let clean = docx(&[
        "제1조(목적) 이 계약은 용역의 범위를 정한다.",
        "제2조(기간) 계약기간은 1년으로 한다.",
        "제3조(대금) 대금은 제2조의 기간 동안 매월 지급한다.",
    ])?;
    let (_, body) = call(&router, upload_request(SID, "d.docx", &clean)?).await?;
    assert_eq!(body["findings"], serde_json::json!([]), "{body}");
    Ok(())
}
