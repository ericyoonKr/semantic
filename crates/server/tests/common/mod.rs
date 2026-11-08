#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::Path;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use server::{AppState, Config};
use tower::ServiceExt;
use zip::write::{SimpleFileOptions, ZipWriter};

pub const KEY: &str = "test-internal-key-0123";
pub const CONSUMER: &str = "test-consumer/1";
pub const MANIFEST: &str = "test-manifest/1";
pub const SID: &str = "0b1c2d3e-4f50-4617-8899-aabbccddeeff";

pub type TestResult = Result<(), Box<dyn std::error::Error>>;
pub type Boxed<T> = Result<T, Box<dyn std::error::Error>>;

pub fn config(root: &Path, extra: &[(&str, &str)]) -> Boxed<Config> {
    let mut map: HashMap<String, String> = HashMap::new();
    map.insert(
        "DATA_DIR".into(),
        root.join("data").to_string_lossy().into_owned(),
    );
    map.insert(
        "KG_DIR".into(),
        root.join("kg").to_string_lossy().into_owned(),
    );
    map.insert("KG_CONSUMER_SCHEMA".into(), CONSUMER.into());
    map.insert("KG_MANIFEST_SCHEMA".into(), MANIFEST.into());
    map.insert("INTERNAL_KEY".into(), KEY.into());
    map.insert("RAYON_THREADS".into(), "2".into());
    for (k, v) in extra {
        map.insert((*k).to_string(), (*v).to_string());
    }
    Ok(Config::from_lookup(move |name| map.get(name).cloned())?)
}

pub async fn app(root: &Path) -> Boxed<(AppState, Router)> {
    let state = AppState::open(config(root, &[])?).await?;
    let router = server::http::router(state.clone());
    Ok((state, router))
}

pub fn request(method: &str, uri: &str, body: Body) -> Boxed<Request<Body>> {
    Ok(Request::builder()
        .method(method)
        .uri(uri)
        .header("x-internal-key", KEY)
        .header("content-type", "application/json")
        .body(body)?)
}

pub async fn call(router: &Router, req: Request<Body>) -> Boxed<(StatusCode, Value)> {
    let res = router.clone().oneshot(req).await?;
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await?;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok((status, value))
}

pub async fn post_json(router: &Router, uri: &str, body: Value) -> Boxed<(StatusCode, Value)> {
    call(router, request("POST", uri, Body::from(body.to_string()))?).await
}

const W_NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";

fn para(text: &str, style: Option<&str>) -> String {
    let props = style
        .map(|s| format!("<w:pPr><w:outlineLvl w:val=\"{s}\"/></w:pPr>"))
        .unwrap_or_default();
    format!("<w:p>{props}<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

pub fn tiny_docx(heading: &str, body: &str) -> Boxed<Vec<u8>> {
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {W_NS}><w:body>{}{}</w:body></w:document>",
        para(heading, Some("0")),
        para(body, None)
    );
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer.start_file("[Content_Types].xml", options)?;
    writer.write_all(b"<?xml version=\"1.0\"?><Types/>")?;
    writer.start_file("word/document.xml", options)?;
    writer.write_all(document.as_bytes())?;
    Ok(writer.finish()?.into_inner())
}

pub fn multipart(filename: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let boundary = "testboundary7f3a";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

pub fn upload_request(sid: &str, filename: &str, bytes: &[u8]) -> Boxed<Request<Body>> {
    let (content_type, body) = multipart(filename, bytes);
    Ok(Request::builder()
        .method("POST")
        .uri(format!("/sessions/{sid}/files"))
        .header("x-internal-key", KEY)
        .header("content-type", content_type)
        .body(Body::from(body))?)
}

pub fn bundle_lines(graph: &str) -> Vec<String> {
    let edge = |id: &str, src: &str, dst: &str, ev: Value| {
        json!({"edge": {"id": id, "source": {"graph": graph, "id": src},
            "target": {"graph": graph, "id": dst}, "relation": "rel_x",
            "certainty": "uncertain", "evidence": ev}})
    };
    let items = vec![
        json!({"node": {"id": "long-node-id-0001", "kind": "kind_a", "label": "alpha concept", "evidence": []}}),
        json!({"node": {"id": "long-node-id-0002", "kind": "kind_a", "label": "beta idea", "evidence": [],
            "item_state": {"judgment": "unknown", "reason_ref": 1}}}),
        json!({"node": {"id": "long-node-id-0003", "kind": "kind_a", "label": "gamma group", "evidence": []}}),
        edge(
            "e1",
            "long-node-id-0001",
            "long-node-id-0002",
            json!([{"source": "s", "quote_ref": 0}]),
        ),
        edge("e2", "long-node-id-0002", "long-node-id-0003", json!([])),
    ];
    let head = json!({"head": {
        "schema": CONSUMER, "graph": graph, "shape": "연결망",
        "state": {"kind": "note", "values": ["해당", "해당 아님"]},
        "contract": {"node_kinds": ["kind_a"], "relations": {"rel_x": {"acyclic": true}},
            "compositions": [["rel_x", "kind_a", "rel_x"]], "composition_mode": "chain_only"},
        "measurement": {"state": "unmeasured"},
        "counts": {"nodes": 3, "edges": 2, "states": 1},
        "texts": ["quoted line", "reason text"]
    }});
    std::iter::once(head)
        .chain(items)
        .map(|v| v.to_string())
        .collect()
}

pub fn write_bundle(kg_dir: &Path, bundle: &str, graph: &str, lines: &[String]) -> TestResult {
    let dir = kg_dir.join("consumer").join(bundle);
    std::fs::create_dir_all(&dir)?;
    let file = format!("{graph}.jsonl");
    let mut body = lines.join("\n");
    body.push('\n');
    std::fs::write(dir.join(&file), body.as_bytes())?;
    let digest = sha256_hex(body.as_bytes());
    let manifest = json!({
        "schema": MANIFEST,
        "source_bundle": bundle,
        "graphs": [{"name": graph, "file": file, "sha256": digest, "shape": "연결망"}],
        "version_mismatches": []
    });
    std::fs::write(dir.join("manifest.json"), manifest.to_string())?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
