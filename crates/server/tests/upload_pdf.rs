mod common;

use axum::http::StatusCode;
use common::{SID, TestResult, call, upload_request};
use parsers::PDF_PENDING;
use sessdb::{Part, doc_key};

fn tiny_pdf(lines: &[&str]) -> Vec<u8> {
    let shown: String = lines
        .iter()
        .enumerate()
        .map(|(i, t)| format!("BT /F1 14 Tf 72 {} Td ({t}) Tj ET\n", 720 - 40 * i))
        .collect();
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{shown}endstream", shown.len()),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    let size = objects.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    out
}

async fn upload_with(extra: &[(&str, &str)]) -> common::Boxed<(StatusCode, serde_json::Value)> {
    let tmp = tempfile::tempdir()?;
    let state = server::AppState::open(common::config(tmp.path(), extra)?).await?;
    let router = server::http::router(state);
    let pdf = tiny_pdf(&["Service agreement first line"]);
    call(&router, upload_request(SID, "a.pdf", &pdf)?).await
}

#[tokio::test]
async fn pdf_upload_without_library_dir_gets_the_unavailable_message() -> TestResult {
    let (status, body) = upload_with(&[]).await?;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{body}");
    assert!(
        body["error"].as_str().is_some_and(|e| e.contains(PDF_PENDING)),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn pdf_upload_with_a_library_dir_that_cannot_load_gets_the_unavailable_message() -> TestResult
{
    let empty = tempfile::tempdir()?;
    let dir = empty.path().to_string_lossy().into_owned();
    let (status, body) = upload_with(&[("PDFIUM_DIR", &dir)]).await?;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{body}");
    assert!(
        body["error"].as_str().is_some_and(|e| e.contains(PDF_PENDING)),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn pdf_upload_is_read_on_the_pdfium_thread_and_stored() -> TestResult {
    let Some(lib) = std::env::var_os("PDFIUM_DIR") else {
        return Ok(());
    };
    let lib = lib.to_string_lossy().into_owned();
    let tmp = tempfile::tempdir()?;
    let cfg = common::config(tmp.path(), &[("PDFIUM_DIR", &lib), ("UPLOAD_MAX_CHARS", "120")])?;
    let state = server::AppState::open(cfg).await?;
    let router = server::http::router(state.clone());
    let pdf = tiny_pdf(&["Service agreement first line", "Payment is due monthly"]);
    let (status, body) = call(&router, upload_request(SID, "contract.pdf", &pdf)?).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["format"], "pdf");
    assert_eq!(body["filename"], "contract.pdf");
    assert_eq!(body["pages"], 1);
    let text = body["text"].as_str().unwrap_or_default().to_string();
    assert!(text.contains("p1"), "{text}");
    assert!(text.contains("Service agreement first line"), "{text}");
    assert!(text.contains("Payment is due monthly"), "{text}");
    assert_eq!(body["findings"], serde_json::json!([]));
    let file_id = body["file_id"].as_str().unwrap_or_default().to_string();
    assert_eq!(file_id, parsers::file_id(&pdf).0);
    let stored = state.db.get_state(&doc_key(SID, &file_id, Part::Text)?)?;
    assert_eq!(stored.as_deref(), Some(text.as_bytes()));
    for part in [Part::Meta, Part::Model, Part::Spans, Part::Findings] {
        assert!(state.db.get_state(&doc_key(SID, &file_id, part)?)?.is_some(), "{part:?}");
    }

    let blank = tiny_pdf(&[]);
    let (status, body) = call(&router, upload_request(SID, "blank.pdf", &blank)?).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    let long = "Each party shall keep the terms of this agreement confidential for five years";
    let (status, body) = call(&router, upload_request(SID, "long.pdf", &tiny_pdf(&[long, long]))?).await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    Ok(())
}
