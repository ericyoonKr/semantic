use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_SECURITY_POLICY, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS};
use axum::http::{HeaderMap, HeaderValue};
use axum::response::Response;
use uuid::Uuid;

use crate::error::GatewayError;
use crate::http::chat::file_id_ok;
use crate::http::guard::{Signed, thread_id};
use crate::state::AppState;

const PDF: &str = "application/pdf";
const KNOWN_TYPES: [&str; 3] = [PDF, "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "application/hwp+zip"];
const FALLBACK_TYPE: &str = "application/octet-stream";
const FALLBACK_NAME: &str = "file";
const NAME_MAX_BYTES: usize = 1024;
const PDF_POLICY: &str = "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'";
const DOWNLOAD_POLICY: &str = "default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; sandbox";

pub fn known_type(raw: Option<&str>) -> &'static str {
    let essence = raw.and_then(|v| v.split(';').next()).map(str::trim).unwrap_or("");
    KNOWN_TYPES.into_iter().find(|t| t.eq_ignore_ascii_case(essence)).unwrap_or(FALLBACK_TYPE)
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn percent_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = hex(*bytes.get(i + 1)?)?;
            let lo = hex(*bytes.get(i + 2)?)?;
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn attr_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~')
}

pub fn rfc5987(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for b in name.bytes() {
        if attr_char(b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn ascii_fallback(name: &str) -> String {
    let plain: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    if plain.trim_matches(|c| c == '_' || c == '.').is_empty() { FALLBACK_NAME.to_string() } else { plain }
}

pub fn clean_name(encoded: Option<&str>) -> String {
    encoded
        .filter(|v| v.len() <= NAME_MAX_BYTES)
        .and_then(percent_decode)
        .map(|n| n.chars().filter(|c| !c.is_control() && !matches!(c, '/' | '\\')).collect::<String>())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| FALLBACK_NAME.to_string())
}

pub fn original_headers(content_type: Option<&str>, encoded_name: Option<&str>) -> Result<HeaderMap, GatewayError> {
    let kind = known_type(content_type);
    let pdf = kind == PDF;
    let name = clean_name(encoded_name);
    let disposition = format!("{}; filename=\"{}\"; filename*=UTF-8''{}", if pdf { "inline" } else { "attachment" }, ascii_fallback(&name), rfc5987(&name));
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(kind));
    headers.insert(CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).map_err(|_| GatewayError::Internal)?);
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(if pdf { PDF_POLICY } else { DOWNLOAD_POLICY }));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static(if pdf { "SAMEORIGIN" } else { "DENY" }));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(headers)
}

async fn owned(state: &AppState, signed: &Signed, raw: &str) -> Result<Uuid, GatewayError> {
    let id = thread_id(raw)?;
    if !state.store.owns(signed.user_id, id).await? {
        return Err(GatewayError::NotFound);
    }
    Ok(id)
}

pub async fn original(State(state): State<AppState>, signed: Signed, Path((raw, file_id)): Path<(String, String)>) -> Result<Response, GatewayError> {
    let id = owned(&state, &signed, &raw).await?;
    if !file_id_ok(&file_id) {
        return Err(GatewayError::BadRequest);
    }
    let res = state
        .http
        .get(format!("{}/session/{id}/files/{file_id}/original", state.cfg.agent_url))
        .header("x-internal-key", &state.cfg.internal_key)
        .header("x-user-id", signed.user_id.to_string())
        .header("x-request-id", Uuid::new_v4().simple().to_string())
        .timeout(state.cfg.call_deadline)
        .send()
        .await
        .map_err(|_| GatewayError::Upstream)?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(GatewayError::NotFound);
    }
    if !res.status().is_success() {
        return Err(GatewayError::Upstream);
    }
    let text = |name: &str| res.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let mut headers = original_headers(text("content-type").as_deref(), text("x-file-name").as_deref())?;
    if let Some(len) = res.headers().get(CONTENT_LENGTH.as_str()).and_then(|v| HeaderValue::from_bytes(v.as_bytes()).ok()) {
        headers.insert(CONTENT_LENGTH, len);
    }
    let mut out = Response::new(Body::from_stream(res.bytes_stream()));
    out.headers_mut().extend(headers);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{clean_name, known_type, original_headers, percent_decode, rfc5987};

    fn get(h: &axum::http::HeaderMap, name: &str) -> String {
        h.get(name).and_then(|v| v.to_str().ok()).unwrap_or("").to_string()
    }

    #[test]
    fn only_known_types_pass_and_others_become_octet_stream() {
        assert_eq!(known_type(Some("application/pdf")), "application/pdf");
        assert_eq!(known_type(Some("Application/PDF; charset=x")), "application/pdf");
        assert_eq!(known_type(Some("text/html")), "application/octet-stream");
        assert_eq!(known_type(None), "application/octet-stream");
    }

    #[test]
    fn names_are_decoded_cleaned_and_reencoded() {
        assert_eq!(percent_decode("%EA%B0%80.pdf").as_deref(), Some("가.pdf"));
        assert_eq!(percent_decode("%E"), None);
        assert_eq!(percent_decode("%FF"), None);
        assert_eq!(clean_name(Some("a%0D%0Ab%2F..%5Cc.pdf")), "ab..c.pdf");
        assert_eq!(clean_name(Some("%zz")), "file");
        assert_eq!(clean_name(None), "file");
        assert_eq!(rfc5987("가 \"x\";.pdf"), "%EA%B0%80%20%22x%22%3B.pdf");
    }

    #[test]
    fn pdf_is_inline_and_frameable_by_us_only() -> Result<(), crate::error::GatewayError> {
        let h = original_headers(Some("application/pdf"), Some("%EC%B2%98%EB%B6%84%20%EC%84%9C.pdf"))?;
        assert_eq!(get(&h, "content-type"), "application/pdf");
        assert_eq!(get(&h, "content-disposition"), "inline; filename=\"____.pdf\"; filename*=UTF-8''%EC%B2%98%EB%B6%84%20%EC%84%9C.pdf");
        assert_eq!(get(&h, "x-content-type-options"), "nosniff");
        assert_eq!(get(&h, "x-frame-options"), "SAMEORIGIN");
        let csp = get(&h, "content-security-policy");
        assert!(csp.contains("default-src 'none'") && csp.contains("frame-ancestors 'self'"));
        Ok(())
    }

    #[test]
    fn other_files_are_downloads_in_a_sandbox() -> Result<(), crate::error::GatewayError> {
        let h = original_headers(Some("application/hwp+zip"), Some("a%22b.hwpx"))?;
        assert_eq!(get(&h, "content-type"), "application/hwp+zip");
        assert!(get(&h, "content-disposition").starts_with("attachment; filename=\"a_b.hwpx\"; filename*=UTF-8''a%22b.hwpx"));
        assert_eq!(get(&h, "x-frame-options"), "DENY");
        assert!(get(&h, "content-security-policy").contains("sandbox"));
        let html = original_headers(Some("text/html"), None)?;
        assert_eq!(get(&html, "content-type"), "application/octet-stream");
        assert!(get(&html, "content-disposition").starts_with("attachment; filename=\"file\""));
        Ok(())
    }
}
