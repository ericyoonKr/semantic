use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use citegate::ReadSet;

pub const THREAD_HEADER: &str = "x-thread-id";
const KEEP: Duration = Duration::from_secs(24 * 3600);
const MAX_THREADS: usize = 10_000;
const MAX_ID: usize = 80;

#[derive(Default)]
pub struct ReadLog(Mutex<HashMap<String, (Instant, ReadSet)>>);

pub fn thread_of(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(THREAD_HEADER)?.to_str().ok()?.trim();
    let ok = !raw.is_empty() && raw.len() <= MAX_ID && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then(|| raw.to_string())
}

impl ReadLog {
    pub fn record(&self, thread: &str, add: impl FnOnce(&mut ReadSet)) {
        let mut map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if map.len() >= MAX_THREADS {
            map.retain(|_, (at, _)| now.duration_since(*at) < KEEP);
        }
        if map.len() >= MAX_THREADS
            && !map.contains_key(thread)
            && let Some(oldest) = map.iter().min_by_key(|(_, (at, _))| *at).map(|(k, _)| k.clone())
        {
            map.remove(&oldest);
        }
        let entry = map.entry(thread.to_string()).or_insert_with(|| (now, ReadSet::default()));
        entry.0 = now;
        add(&mut entry.1);
    }

    pub fn snapshot(&self, thread: &str) -> ReadSet {
        let map = self.0.lock().unwrap_or_else(|e| e.into_inner());
        map.get(thread).map(|(_, r)| r.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::{ReadLog, THREAD_HEADER, thread_of};

    #[test]
    fn reads_are_kept_per_conversation() {
        let log = ReadLog::default();
        log.record("a", |r| r.add_unit("국가배상법", "제2조"));
        assert_eq!(log.snapshot("a").len(), 1);
        assert!(log.snapshot("b").is_empty());
    }

    #[test]
    fn only_plain_conversation_ids_are_taken() {
        let mut h = HeaderMap::new();
        assert_eq!(thread_of(&h), None);
        h.insert(THREAD_HEADER, HeaderValue::from_static("4b12774b-de63"));
        assert_eq!(thread_of(&h).as_deref(), Some("4b12774b-de63"));
        h.insert(THREAD_HEADER, HeaderValue::from_static("../x"));
        assert_eq!(thread_of(&h), None);
    }
}
