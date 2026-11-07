use chrono::NaiveDate;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchResult<T> {
    pub degraded: bool,
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
