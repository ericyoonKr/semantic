use std::sync::Arc;

use docmodel::{Anchor, SpanMap, locate_quote};
use engine::{DepError, FactProvider, Passage};
use sessdb::{Part, SessDb, doc_key};
use tokio::sync::OnceCell;

use crate::upload::FileMeta;

pub const FILES_UNREADABLE: &str = "업로드 문서를 읽지 못함";
pub const NO_CANDIDATES: &str = "긴 문서의 문단 고르기가 아직 없음";
pub const OTHER_SESSION: &str = "다른 세션의 사실 자료를 요청함";

#[derive(Debug, Clone, PartialEq)]
pub struct SessionFile {
    pub filename: String,
    pub text: String,
    pub spans: Vec<SpanMap>,
}

pub struct SessionFacts {
    db: SessDb,
    session: String,
    files: OnceCell<Arc<[SessionFile]>>,
}

fn unreadable(e: impl std::fmt::Display) -> DepError {
    tracing::warn!(error = %e, "session facts could not be read");
    DepError::Other(FILES_UNREADABLE.to_string())
}

fn read_part(db: &SessDb, session: &str, file_id: &str, part: Part) -> Result<Option<Vec<u8>>, DepError> {
    let key = doc_key(session, file_id, part).map_err(unreadable)?;
    db.get_state(&key).map_err(unreadable)
}

pub fn read_files(db: &SessDb, session: &str) -> Result<Vec<SessionFile>, DepError> {
    let stored = db.list_files(session).map_err(unreadable)?;
    let mut metas: Vec<FileMeta> = stored
        .iter()
        .filter_map(|f| serde_json::from_slice(&f.meta).ok())
        .collect();
    metas.sort_by(|a, b| {
        a.uploaded_at
            .cmp(&b.uploaded_at)
            .then_with(|| a.file_id.cmp(&b.file_id))
    });
    let mut out = Vec::with_capacity(metas.len());
    for meta in metas {
        let text = read_part(db, session, &meta.file_id, Part::Text)?
            .and_then(|b| String::from_utf8(b).ok());
        let spans = read_part(db, session, &meta.file_id, Part::Spans)?
            .and_then(|b| serde_json::from_slice::<Vec<SpanMap>>(&b).ok());
        match (text, spans) {
            (Some(text), Some(spans)) => out.push(SessionFile {
                filename: meta.filename,
                text,
                spans,
            }),
            _ => tracing::warn!(file_id = %meta.file_id, "stored upload is incomplete; skipped"),
        }
    }
    Ok(out)
}

pub fn joined(files: &[SessionFile]) -> String {
    files
        .iter()
        .map(|f| format!("[파일: {}]\n{}", f.filename, f.text))
        .collect::<Vec<_>>()
        .join("\n")
}

fn anchor_at(spans: &[SpanMap], start: u32, end: u32) -> Option<Anchor> {
    let hit = spans
        .iter()
        .find(|m| m.start <= start && start < m.end)
        .or_else(|| spans.iter().find(|m| m.start < end && start < m.end))?;
    let mut anchor = hit.anchor.clone();
    anchor.span = Some((start, end));
    Some(anchor)
}

pub fn find(files: &[SessionFile], quote: &str) -> Option<Anchor> {
    files.iter().find_map(|f| {
        let (start, end) = locate_quote(&f.text, quote)?;
        anchor_at(&f.spans, start, end)
    })
}

impl SessionFacts {
    pub fn new(db: SessDb, session: impl Into<String>) -> Self {
        Self {
            db,
            session: session.into(),
            files: OnceCell::new(),
        }
    }

    async fn files(&self, session: &str) -> Result<Arc<[SessionFile]>, DepError> {
        if session != self.session {
            return Err(DepError::Other(OTHER_SESSION.to_string()));
        }
        let loaded = self
            .files
            .get_or_try_init(|| async {
                let db = self.db.clone();
                let session = self.session.clone();
                tokio::task::spawn_blocking(move || read_files(&db, &session))
                    .await
                    .map_err(unreadable)?
                    .map(Arc::from)
            })
            .await?;
        Ok(loaded.clone())
    }
}

impl FactProvider for SessionFacts {
    async fn paged_text(&self, session: &str) -> Result<Arc<str>, DepError> {
        let files = self.files(session).await?;
        Ok(Arc::from(joined(&files)))
    }

    async fn candidates(
        &self,
        _session: &str,
        _proposition: &str,
        _k: usize,
    ) -> Result<Vec<Passage>, DepError> {
        Err(DepError::Unavailable(NO_CANDIDATES.to_string()))
    }

    async fn locate(&self, session: &str, quote: &str) -> Result<Option<Anchor>, DepError> {
        let files = self.files(session).await?;
        let quote = quote.to_string();
        tokio::task::spawn_blocking(move || find(&files, &quote))
            .await
            .map_err(unreadable)
    }
}

#[cfg(test)]
mod tests {
    use docmodel::{Block, Document, FactSource, FileId};
    use sessdb::UploadParts;

    use super::*;

    fn para(text: &str, page: u32) -> Block {
        Block::Para {
            text: text.to_string(),
            anchor: Anchor {
                source: FactSource::File(FileId("file-a".into())),
                page: Some(page),
                bbox: None,
                path: None,
                span: None,
            },
        }
    }

    fn store(db: &SessDb, session: &str) -> Result<(), Box<dyn std::error::Error>> {
        let doc = Document {
            blocks: vec![para("첫째 쪽 문장이다.", 1), para("처분서는 2026년 3월 2일에 송달되었다.", 2)],
            source: FileId("file-a".into()),
            warnings: Vec::new(),
        };
        let (text, spans) = doc.render_paged();
        let meta = serde_json::json!({
            "file_id": "file-a", "filename": "a.docx", "format": "docx", "pages": 2,
            "chars": 10, "outline": [], "warnings": [], "findings_count": 0, "uploaded_at": 1
        });
        let parts = UploadParts {
            meta: &serde_json::to_vec(&meta)?,
            doc: b"{}",
            paged_text: text.as_bytes(),
            span_map: &serde_json::to_vec(&spans)?,
            findings: b"[]",
            vectors: None,
        };
        db.put_upload(session, "file-a", &parts)?;
        Ok(())
    }

    #[tokio::test]
    async fn quotes_are_found_per_file_and_mapped_to_the_block_page() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        let db = sessdb::open(tmp.path())?;
        store(&db, "s1")?;
        let facts = SessionFacts::new(db.clone(), "s1");
        let text = facts.paged_text("s1").await?;
        assert!(text.starts_with("[파일: a.docx]\n--- p1 ---"), "{text}");
        let anchor = facts.locate("s1", "2026년 3월  2일에\n송달되었다").await?;
        assert_eq!(anchor.as_ref().and_then(|a| a.page), Some(2));
        assert_eq!(
            anchor.map(|a| a.source),
            Some(FactSource::File(FileId("file-a".into())))
        );
        assert_eq!(facts.locate("s1", "2026년 3월 3일에 송달").await?, None);
        assert_eq!(facts.locate("s1", "[파일: a.docx]").await?, None);
        assert!(facts.paged_text("s2").await.is_err());
        let empty = SessionFacts::new(db, "s3");
        assert_eq!(&*empty.paged_text("s3").await?, "");
        Ok(())
    }
}
