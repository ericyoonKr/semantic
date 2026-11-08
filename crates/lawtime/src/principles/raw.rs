use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawGraph {
    #[serde(rename = "기준일")]
    pub base_date: String,
    #[serde(rename = "검사")]
    pub check: RawCheck,
    #[serde(rename = "오류")]
    pub errors: Vec<String>,
    #[serde(rename = "인용검사")]
    pub quote_checks: Vec<RawQuoteCheck>,
    pub nodes: Vec<RawNode>,
    pub edges: Vec<RawEdge>,
    #[serde(rename = "판본")]
    pub laws: BTreeMap<String, RawMeta>,
    #[serde(rename = "_source")]
    pub source: RawSource,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCheck {
    #[serde(rename = "노드")]
    pub nodes: usize,
    #[serde(rename = "간선")]
    pub edges: usize,
    #[serde(rename = "종류별 노드")]
    pub by_kind: BTreeMap<String, usize>,
    #[serde(rename = "인용 원문 대조")]
    pub quote_match: String,
    #[serde(rename = "근거 없는 원칙")]
    pub unsupported: usize,
    #[serde(rename = "순서 관계 순환")]
    pub cycles: usize,
    #[serde(rename = "오류 수")]
    pub error_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawQuoteCheck {
    #[serde(rename = "원칙")]
    pub principle: String,
    #[serde(rename = "근거")]
    pub basis: String,
    #[serde(rename = "원문에 있음")]
    pub found: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSource {
    #[allow(dead_code)]
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    pub desc: Option<String>,
    pub field: Option<String>,
    pub times: Option<Vec<String>>,
    pub sources: Option<Vec<RawNodeSource>>,
    pub meta: Option<RawMeta>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawNodeSource {
    pub source: String,
    pub quote: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawMeta {
    #[serde(rename = "법령ID")]
    pub law_id: Option<String>,
    #[serde(rename = "일련번호")]
    pub serial: Option<String>,
    #[serde(rename = "시행일")]
    pub effective: Option<String>,
    #[serde(rename = "현행과같음")]
    pub same_as_current: Option<bool>,
    #[serde(rename = "법원")]
    pub court: Option<String>,
    #[serde(rename = "날짜")]
    pub date: Option<String>,
    #[serde(rename = "사건명")]
    pub case_name: Option<String>,
    #[serde(rename = "공포")]
    pub promulgated: Option<String>,
    #[serde(rename = "번호")]
    pub number: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawEdge {
    pub from: String,
    pub to: String,
    pub rel: String,
    pub quote: Option<String>,
}
