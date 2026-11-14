use serde_json::{Value, json};

const STATUS_SEARCH: &str = "자료를 찾는 중";
const STATUS_READ: &str = "문서를 읽는 중";
const STATUS_JUDGE: &str = "판단하는 중";
const STATUS_WRITE: &str = "답을 쓰는 중";
const WARN_DEGRADED: &str = "일부 자료를 찾지 못했습니다";
const WARN_EXTRACT: &str = "이 파일의 일부를 읽지 못했습니다";
const WARN_INJECTION: &str = "읽은 자료 안에 지시처럼 보이는 문장이 있었습니다";
const ERROR_TEXT: &str = "답을 만들지 못했습니다. 잠시 뒤 다시 시도해 주세요";
const ERROR_MODEL_DOWN: &str = "모델 서버에 연결되지 않아 답을 만들지 못했습니다. 모델 서버가 다시 켜진 뒤 시도해 주세요";

fn status_for(tool: &str) -> &'static str {
    match tool {
        "ls" | "glob" | "grep" | "read_file" | "doc_reader" | "search_document" => STATUS_READ,
        "judge" => STATUS_JUDGE,
        "search_laws" | "search_cases" | "read_law" | "read_case" | "case_citations" | "kg_find" | "kg_expand" | "researcher"
        | "kg_explorer" | "task" => STATUS_SEARCH,
        _ => STATUS_WRITE,
    }
}

const SOURCE_BODY_MAX: usize = 400_000;

fn source_item(v: &Value) -> Option<Value> {
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let body: String = text("body")?.chars().take(SOURCE_BODY_MAX).collect();
    let len = body.chars().count() as u64;
    let spans: Vec<Value> = v
        .get("spans")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    let p = s.as_array()?;
                    let (start, end) = (p.first()?.as_u64()?, p.get(1)?.as_u64()?);
                    (start < end && end <= len).then(|| json!([start, end]))
                })
                .collect()
        })
        .unwrap_or_default();
    let displays: Vec<Value> = v
        .get("displays")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).filter(|s| !s.is_empty()).map(|s| json!(s)).collect())
        .unwrap_or_default();
    let kind = text("kind").filter(|k| k == "law" || k == "case")?;
    let norm = if kind == "law" { norms::label(text("norm_kind").as_deref().unwrap_or("law")) } else { "판례" };
    Some(json!({
        "kind": kind,
        "norm": norm,
        "label": text("label")?,
        "note": text("note").unwrap_or_default(),
        "displays": displays,
        "body": body,
        "spans": spans,
        "id": text("id").unwrap_or_default(),
        "verified": v.get("verified").and_then(Value::as_bool).unwrap_or(false),
        "verify_note": text("verify_note").unwrap_or_default(),
    }))
}

pub(crate) fn frame(name: &str, data: Value) -> String {
    format!("event: {name}\ndata: {data}\n\n")
}

#[derive(Debug, Clone, PartialEq)]
pub enum SideEvent {
    Usage { tokens: i64 },
    Audit { kind: String, detail: Value },
}

#[derive(Debug)]
pub struct SseFilter {
    buffer: String,
    request_id: String,
    last_status: Option<&'static str>,
    hidden: Vec<(String, String)>,
    hiding: Option<usize>,
    side: Vec<SideEvent>,
}

impl SseFilter {
    pub fn new(request_id: impl Into<String>, hidden: Vec<(String, String)>) -> Self {
        Self { buffer: String::new(), request_id: request_id.into(), last_status: None, hidden, hiding: None, side: Vec::new() }
    }

    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(&chunk.replace("\r\n", "\n"));
        let mut out = Vec::new();
        while let Some(end) = self.buffer.find("\n\n") {
            let block: String = self.buffer.drain(..end + 2).collect();
            if let Some(f) = self.translate(&block) {
                out.push(f);
            }
        }
        out
    }

    pub fn take_side(&mut self) -> Vec<SideEvent> {
        std::mem::take(&mut self.side)
    }

    fn translate(&mut self, block: &str) -> Option<String> {
        let mut name = None;
        let mut data = String::new();
        for line in block.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = Some(v.trim().to_string());
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let value: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
        match name?.as_str() {
            "Status" => {
                let tool = value.get("tool").and_then(Value::as_str).unwrap_or("");
                let phase = value.get("phase").and_then(Value::as_str).unwrap_or("");
                let text = if phase == "end" { STATUS_WRITE } else { status_for(tool) };
                if self.last_status == Some(text) {
                    return None;
                }
                self.last_status = Some(text);
                Some(frame("Status", json!({ "text": text })))
            }
            "Token" => {
                let text = value.get("text").and_then(Value::as_str)?;
                let visible = self.strip_hidden(text);
                (!visible.is_empty()).then(|| frame("Token", json!({ "text": visible })))
            }
            "VerdictReady" => {
                let text = value.get("text").and_then(Value::as_str)?;
                Some(frame("VerdictReady", json!({ "text": text })))
            }
            "NeededFacts" => {
                let asks: Vec<&str> = value.get("asks")?.as_array()?.iter().filter_map(Value::as_str).collect();
                (!asks.is_empty()).then(|| frame("NeededFacts", json!({ "asks": asks })))
            }
            "Warning" => {
                let message = match value.get("kind").and_then(Value::as_str)? {
                    "degraded" => WARN_DEGRADED,
                    "extract" => WARN_EXTRACT,
                    "injection" => WARN_INJECTION,
                    _ => return None,
                };
                Some(frame("Warning", json!({ "message": message })))
            }
            "Resume" => {
                let question = value.get("question").and_then(Value::as_str)?;
                Some(frame("Resume", json!({ "question": question })))
            }
            "Sources" => {
                let items: Vec<Value> = value.get("items")?.as_array()?.iter().filter_map(source_item).collect();
                (!items.is_empty()).then(|| frame("Sources", json!({ "items": items })))
            }
            "Error" => {
                let message = match value.get("kind").and_then(Value::as_str) {
                    Some("model_unreachable") => ERROR_MODEL_DOWN,
                    _ => ERROR_TEXT,
                };
                Some(frame("Error", json!({ "message": message, "request_id": self.request_id })))
            }
            "Done" => Some(frame("Done", json!({}))),
            "Usage" => {
                let tokens = value.get("tokens").and_then(Value::as_i64)?;
                self.side.push(SideEvent::Usage { tokens: tokens.max(0) });
                let used = value.get("context").and_then(Value::as_i64)?;
                let limit = value.get("context_limit").and_then(Value::as_i64)?;
                let summary_at = value.get("summary_at").and_then(Value::as_i64).unwrap_or(0);
                (used > 0 && limit > 0).then(|| frame("Context", json!({ "used": used, "limit": limit, "summary_at": summary_at })))
            }
            "Audit" => {
                let kind = value.get("kind").and_then(Value::as_str)?;
                if !crate::audit::AGENT_KINDS.contains(&kind) {
                    return None;
                }
                let detail = value.get("detail").cloned().unwrap_or_else(|| json!({}));
                self.side.push(SideEvent::Audit { kind: kind.to_string(), detail });
                None
            }
            _ => None,
        }
    }

    fn strip_hidden(&mut self, text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        loop {
            match self.hiding {
                Some(i) => {
                    let close = self.hidden[i].1.as_str();
                    match rest.find(close) {
                        Some(p) => {
                            rest = &rest[p + close.len()..];
                            self.hiding = None;
                        }
                        None => return out,
                    }
                }
                None => {
                    let next = self.hidden.iter().enumerate().filter_map(|(i, (open, _))| rest.find(open.as_str()).map(|p| (p, i))).min();
                    match next {
                        Some((p, i)) => {
                            out.push_str(&rest[..p]);
                            rest = &rest[p + self.hidden[i].0.len()..];
                            self.hiding = Some(i);
                        }
                        None => {
                            out.push_str(rest);
                            return out;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
