use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};

use crate::http::middleware::REQUEST_ID;

pub const LAW: &str = "law";
pub const CASE: &str = "case";
pub const NAMED_LAW: &str = "law:";
pub const LAW_MIN: u32 = 3;
pub const CASE_MIN: u32 = 1;
const KEEP: Duration = Duration::from_secs(3600);
const MAX_REQUESTS: usize = 10_000;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tally {
    pub laws: u32,
    pub cases: u32,
    pub read: Vec<String>,
    pub required: Vec<String>,
    pub rounds: Vec<(String, u32)>,
}

#[derive(Debug, Serialize)]
pub struct CoverageReply {
    pub laws: u32,
    pub cases: u32,
    pub missing: Vec<String>,
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

fn same_law(a: &str, b: &str) -> bool {
    a.ends_with(b) || b.ends_with(a)
}

pub fn missing(t: &Tally) -> Vec<String> {
    let mut out = Vec::new();
    if t.laws < LAW_MIN {
        out.push(LAW.to_string());
    }
    if t.cases < CASE_MIN {
        out.push(CASE.to_string());
    }
    for law in &t.required {
        if !t.read.iter().any(|r| same_law(r, law)) {
            out.push(format!("{NAMED_LAW}{law}"));
        }
    }
    out
}

impl Coverage {
    fn with(&self, request: String, change: impl FnOnce(&mut Tally)) {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if map.len() >= MAX_REQUESTS {
            map.retain(|_, (at, _)| now.duration_since(*at) < KEEP);
        }
        let entry = map.entry(request).or_insert((now, Tally::default()));
        entry.0 = now;
        change(&mut entry.1);
    }

    pub fn law(&self, headers: &HeaderMap, law_name: &str) {
        let Some(request) = request_of(headers) else { return };
        let name = citegate::canonical(law_name);
        self.with(request, |t| {
            t.laws += 1;
            if !t.read.contains(&name) {
                t.read.push(name);
            }
        });
    }

    pub fn case(&self, headers: &HeaderMap) {
        let Some(request) = request_of(headers) else { return };
        self.with(request, |t| t.cases += 1);
    }

    pub fn require(&self, request: &str, laws: &[String]) {
        if request.trim().is_empty() || laws.is_empty() {
            return;
        }
        self.with(request.trim().to_string(), |t| {
            for law in laws {
                let name = citegate::canonical(law);
                if !t.required.contains(&name) {
                    t.required.push(name);
                }
            }
        });
    }

    pub fn tally(&self, request: &str) -> Tally {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(request.trim()).map(|(_, t)| t.clone()).unwrap_or_default()
    }

    pub fn round(&self, request: &str, kind: &str) -> u32 {
        let mut count = 0;
        self.with(request.trim().to_string(), |t| {
            match t.rounds.iter_mut().find(|(k, _)| k == kind) {
                Some((_, n)) => *n += 1,
                None => t.rounds.push((kind.to_string(), 1)),
            }
            count = t.rounds.iter().find(|(k, _)| k == kind).map_or(1, |(_, n)| *n);
        });
        count
    }

    pub fn reply(&self, request: &str) -> CoverageReply {
        let t = self.tally(request);
        CoverageReply { laws: t.laws, cases: t.cases, missing: missing(&t) }
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
        c.law(&a, "지방자치단체를 당사자로 하는 계약에 관한 법률");
        c.law(&a, "지방자치단체를 당사자로 하는 계약에 관한 법률");
        assert_eq!(c.reply("req-a").missing, vec![LAW, CASE]);
        c.law(&a, "지방자치단체를 당사자로 하는 계약에 관한 법률");
        assert_eq!(c.reply("req-a").missing, vec![CASE]);
        c.case(&a);
        let r = c.reply("req-a");
        assert_eq!((r.laws, r.cases, r.missing.len()), (3, 1, 0));
        assert_eq!(c.reply("req-b").missing, vec![LAW, CASE]);
        c.law(&HeaderMap::new(), "민법");
        assert_eq!(c.reply("").laws, 0);
    }

    #[test]
    fn laws_named_in_the_question_must_be_read() {
        let c = Coverage::default();
        let mut a = HeaderMap::new();
        a.insert(REQUEST_ID, HeaderValue::from_static("req-a"));
        c.require("req-a", &["지방공무원법".to_string()]);
        for _ in 0..3 {
            c.law(&a, "지방자치단체를 당사자로 하는 계약에 관한 법률");
        }
        c.case(&a);
        assert_eq!(c.reply("req-a").missing, vec!["law:지방공무원법".to_string()]);
        c.law(&a, "지방공무원법");
        assert!(c.reply("req-a").missing.is_empty());
    }
}
