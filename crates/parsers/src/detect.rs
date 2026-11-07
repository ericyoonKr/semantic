use std::io::{Cursor, Read};

use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::error::ParseError;

pub const RESAVE_GUIDANCE: &str =
    "HWPX, DOCX 또는 글자가 들어 있는 PDF로 저장한 뒤 다시 올려 주세요";

const PDF_MAGIC: &[u8] = b"%PDF-";
const CFB_MAGIC: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0];
const ZIP_MAGICS: [&[u8]; 2] = [b"PK\x03\x04", b"PK\x05\x06"];
const DOCX_MAIN: &str = "word/document.xml";
const HWPX_SECTION: &str = "Contents/section0.xml";
const HWPX_MIMETYPE: &str = "application/hwp+zip";
const MIMETYPE_READ_LIMIT: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    Docx,
    Hwpx,
    Pdf,
}

pub fn detect(bytes: &[u8]) -> Result<Format, ParseError> {
    if bytes.starts_with(PDF_MAGIC) {
        return Ok(Format::Pdf);
    }
    if bytes.starts_with(CFB_MAGIC) {
        return Err(unsupported());
    }
    if ZIP_MAGICS.iter().any(|m| bytes.starts_with(m)) {
        return detect_zip(bytes);
    }
    Err(unsupported())
}

fn detect_zip(bytes: &[u8]) -> Result<Format, ParseError> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|_| ParseError::Corrupted)?;
    if archive.index_for_name(DOCX_MAIN).is_some() {
        return Ok(Format::Docx);
    }
    if archive.index_for_name(HWPX_SECTION).is_some() {
        return Ok(Format::Hwpx);
    }
    if let Ok(file) = archive.by_name("mimetype") {
        let mut head = String::new();
        if file.take(MIMETYPE_READ_LIMIT).read_to_string(&mut head).is_ok()
            && head.trim() == HWPX_MIMETYPE
        {
            return Ok(Format::Hwpx);
        }
    }
    Err(unsupported())
}

fn unsupported() -> ParseError {
    ParseError::Unsupported(RESAVE_GUIDANCE.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testzip::build_zip;

    #[test]
    fn cfb_is_unsupported_with_guidance() {
        let mut bytes = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        bytes.extend(std::iter::repeat_n(0u8, 504));
        assert_eq!(detect(&bytes), Err(ParseError::Unsupported(RESAVE_GUIDANCE.to_string())));
    }

    #[test]
    fn random_bytes_are_rejected() {
        let mut state: u32 = 0x1234_5678;
        let bytes: Vec<u8> = (0..4096)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state & 0xFF) as u8
            })
            .collect();
        assert!(matches!(detect(&bytes), Err(ParseError::Unsupported(_))));
        assert!(matches!(detect(&[]), Err(ParseError::Unsupported(_))));
    }

    #[test]
    fn broken_zip_is_corrupted() {
        let mut bytes = b"PK\x03\x04".to_vec();
        bytes.extend([7u8; 200]);
        assert_eq!(detect(&bytes), Err(ParseError::Corrupted));
    }

    #[test]
    fn zip_members_decide_format() {
        let docx = build_zip(&[(DOCX_MAIN, b"<w:document/>".as_slice())]);
        let hwpx = build_zip(&[("mimetype", HWPX_MIMETYPE.as_bytes())]);
        let other = build_zip(&[("a.txt", b"x".as_slice())]);
        assert_eq!(detect(&docx), Ok(Format::Docx));
        assert_eq!(detect(&hwpx), Ok(Format::Hwpx));
        assert!(matches!(detect(&other), Err(ParseError::Unsupported(_))));
        assert_eq!(detect(b"%PDF-1.7\n"), Ok(Format::Pdf));
    }
}
