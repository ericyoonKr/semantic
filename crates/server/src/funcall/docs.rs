use sessdb::{Part, SessDb, doc_key};

use super::next_step::Doc;
use crate::upload::FileMeta;

pub fn session_docs(db: &SessDb, session: &str) -> Vec<Doc> {
    let Ok(stored) = db.list_files(session) else {
        return Vec::new();
    };
    stored
        .iter()
        .filter_map(|f| serde_json::from_slice::<FileMeta>(&f.meta).ok())
        .filter_map(|meta| {
            let key = doc_key(session, &meta.file_id, Part::Text).ok()?;
            let text = String::from_utf8(db.get_state(&key).ok()??).ok()?;
            Some(Doc {
                file_id: meta.file_id,
                filename: meta.filename,
                chars: meta.chars,
                uploaded_at: meta.uploaded_at,
                processing: meta.processing,
                text,
            })
        })
        .collect()
}
