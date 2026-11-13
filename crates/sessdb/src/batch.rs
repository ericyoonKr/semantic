use rocksdb::WriteBatch;

use crate::cf::{Part, doc_key, encode};
use crate::db::{CF_STATE, SessDb};
use crate::error::StorageError;

#[derive(Debug, Clone, Copy)]
pub struct UploadParts<'a> {
    pub meta: &'a [u8],
    pub doc: &'a [u8],
    pub paged_text: &'a [u8],
    pub span_map: &'a [u8],
    pub findings: &'a [u8],
    pub vectors: Option<&'a [u8]>,
    pub original: &'a [u8],
}

impl SessDb {
    pub fn put_upload(
        &self,
        session: &str,
        file_id: &str,
        parts: &UploadParts<'_>,
    ) -> Result<(), StorageError> {
        let cf = self.cf(CF_STATE)?;
        let mut batch = WriteBatch::default();
        if parts.vectors.is_none() {
            batch.delete_cf(cf, doc_key(session, file_id, Part::Vectors)?);
        }
        let mut put = |part: Part, value: &[u8]| -> Result<(), StorageError> {
            batch.put_cf(cf, doc_key(session, file_id, part)?, encode(value));
            Ok(())
        };
        put(Part::Model, parts.doc)?;
        put(Part::Text, parts.paged_text)?;
        put(Part::Spans, parts.span_map)?;
        put(Part::Findings, parts.findings)?;
        put(Part::Original, parts.original)?;
        if let Some(v) = parts.vectors {
            put(Part::Vectors, v)?;
        }
        put(Part::Meta, parts.meta)?;
        self.0.write(batch)?;
        Ok(())
    }
}
