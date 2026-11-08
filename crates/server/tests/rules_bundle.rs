mod common;

use std::path::Path;

use axum::http::StatusCode;
use common::{TestResult, call, config, request};
use serde_json::Value;
use server::AppState;

const CLAIM: &str = "CLAIM_국가배상";

fn copy_bundle(from: &Path, kg_dir: &Path) -> Result<(String, String, Value), Box<dyn std::error::Error>> {
    let to = kg_dir.join("consumer").join("semantic-rules");
    std::fs::create_dir_all(&to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        std::fs::copy(entry.path(), to.join(entry.file_name()))?;
    }
    let manifest: Value = serde_json::from_slice(&std::fs::read(to.join("manifest.json"))?)?;
    let body = std::fs::read_to_string(to.join(format!("{CLAIM}.jsonl")))?;
    let first = body.lines().next().ok_or("empty tree file")?;
    let head: Value = serde_json::from_str(first)?;
    let manifest_schema = manifest["schema"].as_str().ok_or("manifest schema")?.to_string();
    let consumer_schema = head["head"]["schema"].as_str().ok_or("head schema")?.to_string();
    Ok((consumer_schema, manifest_schema, head["head"]["counts"].clone()))
}

#[tokio::test]
#[ignore = "RULES_BUNDLE_DIR에 ruleloader build가 쓴 저장소 밖 묶음 폴더를 넣고 돌린다"]
async fn catalog_lists_every_node_of_the_built_state_liability_tree() -> TestResult {
    let from = std::env::var("RULES_BUNDLE_DIR")?;
    let tmp = tempfile::tempdir()?;
    let kg_dir = tmp.path().join("kg");
    let (consumer, manifest, counts) = copy_bundle(Path::new(&from), &kg_dir)?;
    let extra = [("KG_CONSUMER_SCHEMA", consumer.as_str()), ("KG_MANIFEST_SCHEMA", manifest.as_str())];
    let state = AppState::open(config(tmp.path(), &extra)?).await?;
    assert_eq!(state.rules.len(), 1);
    let router = server::http::router(state.clone());
    let (status, body) = call(&router, request("GET", "/catalog", axum::body::Body::empty())?).await?;
    assert_eq!(status, StatusCode::OK);
    let items = body.as_array().cloned().unwrap_or_default();
    assert_eq!(Some(items.len() as u64), counts["nodes"].as_u64(), "{body}");
    assert!(items.iter().all(|i| i["claim_type"] == CLAIM));
    assert_eq!(items[0]["node_id"], CLAIM);
    let leaves = items.iter().filter(|i| i["gate"] == "LEAF").count() as u64;
    assert_eq!(Some(leaves), counts["leaves"].as_u64());
    let tree = state.rules.tree(CLAIM).ok_or("tree missing")?;
    assert_eq!(tree.edges().len() as u64, counts["edges"].as_u64().unwrap_or_default());
    assert!(items.iter().filter(|i| i["gate"] == "LEAF").all(|i| i["evidence"].as_array().is_some_and(|e| !e.is_empty())));
    println!("catalog nodes {} leaves {} edges {}", items.len(), leaves, tree.edges().len());
    Ok(())
}
