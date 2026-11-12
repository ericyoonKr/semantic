use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct GoldLaw {
    pub law: String,
    pub unit: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub named_in_query: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoldCase {
    pub case_no: String,
    #[serde(default)]
    pub decided_on: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaseRef {
    pub case_no: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Filters {
    pub as_of: Option<String>,
    pub decided_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UnitParams {
    pub law: String,
    pub unit: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McqGold {
    pub official_answers: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub params: Option<UnitParams>,
    #[serde(default)]
    pub filters: Option<Filters>,
    #[serde(default)]
    pub gold_laws: Vec<GoldLaw>,
    #[serde(default)]
    pub gold_cases: Vec<GoldCase>,
    #[serde(default)]
    pub exclude_cases: Vec<CaseRef>,
    #[serde(default)]
    pub gold: Option<McqGold>,
}

pub fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("파일을 읽지 못했습니다: {}", path.display()))?;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v = serde_json::from_str(line)
            .with_context(|| format!("{}의 {}번째 줄을 읽지 못했습니다", path.display(), n + 1))?;
        out.push(v);
    }
    Ok(out)
}
