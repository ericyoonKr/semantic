use rocksdb::{Direction, IteratorMode};

use crate::db::{CF_LEAF, CF_STATE, SessDb};
use crate::error::StorageError;

pub const FORMAT_VERSION: u8 = 1;

const MAX_KEY_PART: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Meta,
    Model,
    Text,
    Spans,
    Findings,
    Vectors,
}

impl Part {
    fn as_str(self) -> &'static str {
        match self {
            Part::Meta => "meta",
            Part::Model => "model",
            Part::Text => "text",
            Part::Spans => "spans",
            Part::Findings => "findings",
            Part::Vectors => "vectors",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    pub file_id: String,
    pub meta: Vec<u8>,
}

pub(crate) fn check_part(value: &str, what: &'static str) -> Result<(), StorageError> {
    let ok = !value.is_empty()
        && value.len() <= MAX_KEY_PART
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(StorageError::Key(what))
    }
}

fn session_prefix(session: &str) -> Result<String, StorageError> {
    check_part(session, "session")?;
    Ok(format!("{session}/"))
}

fn files_prefix(session: &str) -> Result<String, StorageError> {
    Ok(format!("{}doc/", session_prefix(session)?))
}

pub fn doc_key(session: &str, file_id: &str, part: Part) -> Result<String, StorageError> {
    check_part(file_id, "file_id")?;
    Ok(format!(
        "{}{file_id}/{}",
        files_prefix(session)?,
        part.as_str()
    ))
}

pub fn kg_ids_key(session: &str) -> Result<String, StorageError> {
    Ok(format!("{}kg_ids", session_prefix(session)?))
}

pub(crate) fn encode(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() + 1);
    out.push(FORMAT_VERSION);
    out.extend_from_slice(value);
    out
}

fn decode(mut raw: Vec<u8>) -> Option<Vec<u8>> {
    if raw.first() != Some(&FORMAT_VERSION) {
        return None;
    }
    raw.remove(0);
    Some(raw)
}

impl SessDb {
    fn get_in(&self, cf: &'static str, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self.0.get_cf(self.cf(cf)?, key)?.and_then(decode))
    }

    fn put_in(&self, cf: &'static str, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        Ok(self.0.put_cf(self.cf(cf)?, key, encode(value))?)
    }

    pub fn get_state(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        self.get_in(CF_STATE, key.as_bytes())
    }

    pub fn put_state(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        self.put_in(CF_STATE, key.as_bytes(), value)
    }

    pub fn get_leaf(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        self.get_in(CF_LEAF, key)
    }

    pub fn put_leaf(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.put_in(CF_LEAF, key, value)
    }

    pub fn list_files(&self, session: &str) -> Result<Vec<StoredFile>, StorageError> {
        let prefix = files_prefix(session)?;
        let meta_suffix = format!("/{}", Part::Meta.as_str());
        let mode = IteratorMode::From(prefix.as_bytes(), Direction::Forward);
        let mut out = Vec::new();
        for item in self.0.iterator_cf(self.cf(CF_STATE)?, mode) {
            let (key, value) = item?;
            let Some(rest) = key.strip_prefix(prefix.as_bytes()) else {
                break;
            };
            let Some(file_id) = rest.strip_suffix(meta_suffix.as_bytes()) else {
                continue;
            };
            let (Ok(file_id), Some(meta)) =
                (std::str::from_utf8(file_id), decode(value.into_vec()))
            else {
                continue;
            };
            out.push(StoredFile {
                file_id: file_id.to_string(),
                meta,
            });
        }
        Ok(out)
    }

    pub fn delete_session(&self, session: &str) -> Result<(), StorageError> {
        check_part(session, "session")?;
        let from = format!("{session}/");
        let to = format!("{session}0");
        Ok(self.0.delete_range_cf(self.cf(CF_STATE)?, from, to)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_format_version_reads_as_missing() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let db = crate::open(tmp.path())?;
        let key = kg_ids_key("s")?;
        db.0.put_cf(db.cf(CF_STATE)?, key.as_bytes(), [9u8, b'x'])?;
        assert_eq!(db.get_state(&key)?, None);
        db.0.put_cf(db.cf(CF_STATE)?, key.as_bytes(), [])?;
        assert_eq!(db.get_state(&key)?, None);
        db.put_state(&key, b"x")?;
        assert_eq!(db.get_state(&key)?, Some(b"x".to_vec()));
        Ok(())
    }
}
