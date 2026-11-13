use docmodel::{Anchor, ExtractWarning, OutlineEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub kind: String,
    pub message: String,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UploadReply {
    pub file_id: String,
    pub filename: String,
    pub format: String,
    pub pages: Option<u32>,
    pub chars: u64,
    pub outline: Vec<OutlineEntry>,
    pub text: String,
    pub warnings: Vec<String>,
    pub findings: Vec<Finding>,
    pub processing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileMeta {
    pub file_id: String,
    pub filename: String,
    pub format: String,
    pub pages: Option<u32>,
    pub chars: u64,
    pub outline: Vec<OutlineEntry>,
    pub warnings: Vec<String>,
    pub findings_count: usize,
    pub uploaded_at: u64,
    #[serde(default)]
    pub processing: bool,
}

impl FileMeta {
    pub fn of(reply: &UploadReply, uploaded_at: u64) -> Self {
        Self {
            file_id: reply.file_id.clone(),
            filename: reply.filename.clone(),
            format: reply.format.clone(),
            pages: reply.pages,
            chars: reply.chars,
            outline: reply.outline.clone(),
            warnings: reply.warnings.clone(),
            findings_count: reply.findings.len(),
            uploaded_at,
            processing: reply.processing,
        }
    }
}

fn page_label(anchor: &Anchor) -> String {
    anchor
        .page
        .map_or_else(|| "쪽 모름".to_string(), |p| format!("p{p}"))
}

pub fn describe_warning(w: &ExtractWarning) -> String {
    match w {
        ExtractWarning::PagesUnextracted { from, to } => {
            format!("추출 불가: p{from}–p{to}")
        }
        ExtractWarning::PagesRecognized { from, to } => {
            format!("글자 인식으로 읽음: p{from}–p{to} — 인쇄된 글자와 다를 수 있음")
        }
        ExtractWarning::TableDegraded { anchor } => {
            format!("{} 표를 평문으로 낮춤 — 산술 검증 제외", page_label(anchor))
        }
        ExtractWarning::LowConfidenceTable { anchor } => {
            format!("{} 표의 신뢰 등급 낮음", page_label(anchor))
        }
    }
}
