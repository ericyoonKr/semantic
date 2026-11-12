use chrono::NaiveDate;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchResult<T> {
    pub degraded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<String>,
    pub hits: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LawHit {
    pub law_name: String,
    pub unit_key: String,
    pub title: String,
    pub enforcement_date: Option<NaiveDate>,
    pub kind: String,
    pub lsi_seq: i64,
    pub text: String,
    pub score: f64,
    pub law_id: String,
    pub unit_kind: String,
    pub version_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_from: Option<NaiveDate>,
    pub matched: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched_units: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseHit {
    pub decision_id: i64,
    pub court: String,
    pub case_no: String,
    pub decided_on: NaiveDate,
    pub field: String,
    pub section_role: String,
    pub text: String,
    pub score: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LawQuery {
    pub text: String,
    pub k: usize,
    pub as_of: Option<NaiveDate>,
    pub law_type: Option<Vec<String>>,
    pub law_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseQuery {
    pub text: String,
    pub k: usize,
    pub field: Option<Vec<String>>,
    pub court_level: Option<Vec<String>>,
    pub decided_from: Option<NaiveDate>,
    pub decided_to: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LawVersion {
    pub kind: String,
    pub lsi_seq: i64,
    pub status: String,
    pub version_id: String,
    pub valid_from: Option<NaiveDate>,
    pub valid_to: Option<NaiveDate>,
    pub end_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LawUnit {
    pub law_name: String,
    pub unit_key: String,
    pub title: String,
    pub body: String,
    pub deleted: bool,
    pub content_status: String,
    pub source_urls: Value,
    pub enforcement_date: Option<NaiveDate>,
    pub version: LawVersion,
    pub law_id: String,
    pub unit_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_from: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseSection {
    pub role: String,
    pub heading: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaseDoc {
    pub court: String,
    pub case_no: String,
    pub decided_on: NaiveDate,
    pub field: String,
    pub sections: Vec<CaseSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CitationsBy {
    Decision(i64),
    LawUnit { law: String, unit: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CitedUnit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub law_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_key: Option<String>,
    pub relation: String,
    pub raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsi_seq: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CitingCase {
    pub decision_id: i64,
    pub court: String,
    pub case_no: String,
    pub decided_on: NaiveDate,
    pub unit_key: String,
    pub relation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum CitationItem {
    Cited(CitedUnit),
    Citing(CitingCase),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Citations {
    pub items: Vec<CitationItem>,
    pub extract_run: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnnexVersion {
    pub law_id: String,
    pub law_name: String,
    pub annex_key: String,
    pub annex_label: String,
    pub annex_type: String,
    pub version_id: String,
    pub title: String,
    pub title_related: Option<String>,
    pub valid_from: Option<NaiveDate>,
    pub valid_to: Option<NaiveDate>,
    pub end_reason: Option<String>,
    pub review_status: String,
    pub source_urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnnexRow {
    pub node_id: String,
    pub path: String,
    pub node_type: String,
    pub label: Option<String>,
    pub table: Option<usize>,
    pub text: String,
    pub cells: Value,
    pub amounts: Value,
    pub basis_unit_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnnexPara {
    pub node_id: String,
    pub path: String,
    pub node_type: String,
    pub label: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnnexRuleItem {
    pub rule_id: String,
    pub node_id: Option<String>,
    pub kind: String,
    pub params: Value,
    pub reviewed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelatedArticle {
    pub law_name: String,
    pub article_no: String,
    pub ref_type: String,
    pub ref_text: Option<String>,
    pub version_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnnexTable {
    pub annex: AnnexVersion,
    pub columns: Vec<Vec<String>>,
    pub rows: Vec<AnnexRow>,
    pub rows_matched: usize,
    pub paras: Vec<AnnexPara>,
    pub rules: Vec<AnnexRuleItem>,
    pub related: Vec<RelatedArticle>,
    pub failed_checks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnnexLookup {
    pub law: String,
    pub annex: String,
    pub as_of: Option<NaiveDate>,
    pub row: Option<String>,
    pub words: Option<String>,
    pub column: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnnexCompute {
    pub node_id: String,
    pub column: Option<String>,
    pub elapsed: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Computed {
    pub column: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<usize>,
    pub cell_text: String,
    pub kind: String,
    pub krw: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Computation {
    pub annex: AnnexVersion,
    pub node_id: String,
    pub path: String,
    pub label: Option<String>,
    pub row_text: String,
    pub basis_unit_ids: Vec<String>,
    pub results: Vec<Computed>,
    pub paras: Vec<AnnexPara>,
    pub rules: Vec<AnnexRuleItem>,
    pub notes: Vec<String>,
}
