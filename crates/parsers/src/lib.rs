mod detect;
mod docx;
mod error;
mod guard;
mod hwpx;
mod parse;
mod ocr;
mod pdf;
mod pdf_table;
mod xml;

#[cfg(test)]
mod testzip;

pub use detect::{Format, RESAVE_GUIDANCE, detect};
pub use error::ParseError;
pub use guard::{GuardLimits, check_document, check_size};
pub use parse::{PDF_PENDING, file_id, parse};
pub use ocr::{DET_MAX_SIDE, OCR_MISSING, Ocr, OcrLine};
pub use pdf::{PDF_LIBRARY_MISSING, PdfReader};
