use docmodel::{Document, FileId};
use sha2::{Digest, Sha256};

use crate::detect::{Format, detect};
use crate::docx::parse_docx;
use crate::error::ParseError;
use crate::guard::{GuardLimits, check_document, check_size};
use crate::hwpx::parse_hwpx;

pub const PDF_PENDING: &str = "PDF 읽기는 아직 지원하지 않습니다";

pub fn parse(bytes: &[u8], limits: &GuardLimits) -> Result<Document, ParseError> {
    check_size(bytes, limits)?;
    let source = file_id(bytes);
    let doc = match detect(bytes)? {
        Format::Docx => parse_docx(bytes, limits, source)?,
        Format::Hwpx => parse_hwpx(bytes, limits, source)?,
        Format::Pdf => return Err(ParseError::Unsupported(PDF_PENDING.to_string())),
    };
    check_document(&doc, limits)?;
    Ok(doc)
}

pub fn file_id(bytes: &[u8]) -> FileId {
    let digest = Sha256::digest(bytes);
    FileId(digest.iter().map(|b| format!("{b:02x}")).collect())
}
