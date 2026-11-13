use std::sync::Mutex;

use axum::http::HeaderMap;
use citegate::{ReadParts, ReadSet};
use sessdb::{SessDb, reads_key};

pub const THREAD_HEADER: &str = "x-thread-id";
const MAX_ID: usize = 80;

pub struct ReadLog {
    db: SessDb,
    write: Mutex<()>,
}

pub fn thread_of(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(THREAD_HEADER)?.to_str().ok()?.trim();
    let ok = !raw.is_empty() && raw.len() <= MAX_ID && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then(|| raw.to_string())
}

impl ReadLog {
    pub fn new(db: SessDb) -> Self {
        ReadLog { db, write: Mutex::new(()) }
    }

    pub fn record(&self, thread: &str, add: impl FnOnce(&mut ReadSet)) {
        let _guard = self.write.lock().unwrap_or_else(|e| e.into_inner());
        let mut set = self.snapshot(thread);
        add(&mut set);
        let Ok(key) = reads_key(thread) else { return };
        let saved = serde_json::to_vec(&set.parts()).map_err(|e| e.to_string()).and_then(|bytes| self.db.put_state(&key, &bytes).map_err(|e| e.to_string()));
        if let Err(error) = saved {
            tracing::warn!(%error, "readlog.save_failed");
        }
    }

    pub fn snapshot(&self, thread: &str) -> ReadSet {
        let Ok(key) = reads_key(thread) else { return ReadSet::default() };
        match self.db.get_state(&key) {
            Ok(Some(bytes)) => serde_json::from_slice::<ReadParts>(&bytes).map(ReadSet::from_parts).unwrap_or_default(),
            Ok(None) => ReadSet::default(),
            Err(error) => {
                tracing::warn!(%error, "readlog.load_failed");
                ReadSet::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::{ReadLog, THREAD_HEADER, thread_of};

    #[test]
    fn reads_are_kept_per_conversation_survive_reopening_and_go_with_the_session() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        {
            let log = ReadLog::new(sessdb::open(tmp.path())?);
            log.record("a", |r| r.add_unit_body("국가배상법", "제2조", "① 배상한다.\n② 구상한다."));
            log.record("a", |r| r.add_case("2024두35989"));
            assert_eq!(log.snapshot("a").len(), 2);
            assert!(log.snapshot("b").is_empty());
        }
        let db = sessdb::open(tmp.path())?;
        let log = ReadLog::new(db.clone());
        assert!(citegate::check("국가배상법 제2조 제2항과 2024두35989 판결", &log.snapshot("a")).ok);
        db.delete_session("a")?;
        assert!(log.snapshot("a").is_empty());
        Ok(())
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
