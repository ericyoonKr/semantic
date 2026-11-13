use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};

use crate::http::middleware::REQUEST_ID;

pub const LAW: &str = "law";
pub const CASE: &str = "case";
const KEEP: Duration = Duration::from_secs(3600);
const MAX_REQUESTS: usize = 10_000;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub laws: u32,
    pub cases: u32,
}

#[derive(Debug, Serialize)]
pub struct CoverageReply {
    pub laws: u32,
    pub cases: u32,
    pub missing: Vec<&'static str>,
}

#[derive(Debug, Deserialize)]
pub struct CoverageParams {
    pub request_id: String,
}

#[derive(Default)]
pub struct Coverage(Mutex<HashMap<String, (Instant, Tally)>>);

fn request_of(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(REQUEST_ID)?.to_str().ok()?.trim();
    (!raw.is_empty()).then(|| raw.to_string())
}

pub fn missing(t: Tally) -> Vec<&'static str> {
    let mut out = Vec::new();
    if t.laws == 0 {
        out.push(LAW);
    }
    if t.cases == 0 {
        out.push(CASE);
    }
    out
}

impl Coverage {
    fn bump(&self, headers: &HeaderMap, add: impl FnOnce(&mut Tally)) {
        let Some(request) = request_of(headers) else { return };
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if map.len() >= MAX_REQUESTS {
            map.retain(|_, (at, _)| now.duration_since(*at) < KEEP);
        }
        let entry = map.entry(request).or_insert((now, Tally::default()));
        entry.0 = now;
        add(&mut entry.1);
    }

    pub fn law(&self, headers: &HeaderMap) {
        self.bump(headers, |t| t.laws += 1);
    }

    pub fn case(&self, headers: &HeaderMap) {
        self.bump(headers, |t| t.cases += 1);
    }

    pub fn reply(&self, request: &str) -> CoverageReply {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let t = map.get(request.trim()).map(|(_, t)| *t).unwrap_or_default();
        CoverageReply { laws: t.laws, cases: t.cases, missing: missing(t) }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::{CASE, Coverage, LAW, REQUEST_ID};

    #[test]
    fn each_question_counts_its_own_law_and_case_reads() {
        let c = Coverage::default();
        let mut a = HeaderMap::new();
        a.insert(REQUEST_ID, HeaderValue::from_static("req-a"));
        assert_eq!(c.reply("req-a").missing, vec![LAW, CASE]);
        c.law(&a);
        c.law(&a);
        assert_eq!(c.reply("req-a").missing, vec![CASE]);
        c.case(&a);
        let r = c.reply("req-a");
        assert_eq!((r.laws, r.cases, r.missing.len()), (2, 1, 0));
        assert_eq!(c.reply("req-b").missing, vec![LAW, CASE]);
        c.law(&HeaderMap::new());
        assert_eq!(c.reply("").laws, 0);
    }
}
