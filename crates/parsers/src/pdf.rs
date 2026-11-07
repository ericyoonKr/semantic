mod layout;
mod words;

#[cfg(test)]
mod tests;

use std::path::Path;

use docmodel::{Block, Document, ExtractWarning, FactSource, FileId, Rect};
use pdfium_render::prelude::*;

use crate::error::ParseError;
use crate::pdf_table::{Segment, TableSettings};
use layout::{PageInput, lay_out};
use words::CharBox;

pub const PDF_LIBRARY_MISSING: &str = "PDF 읽기 모듈을 찾지 못했습니다";

pub struct PdfReader {
    pdfium: Pdfium,
    settings: TableSettings,
}

fn rect(r: &PdfRect) -> Rect {
    Rect { x0: r.left().value, y0: r.bottom().value, x1: r.right().value, y1: r.top().value }
}

fn page_chars(page: &PdfPage) -> Result<Vec<CharBox>, ParseError> {
    let text = page.text().map_err(|_| ParseError::Corrupted)?;
    let mut out = Vec::new();
    for c in text.chars().iter() {
        let (Some(ch), Ok(bounds)) = (c.unicode_char(), c.loose_bounds()) else {
            continue;
        };
        out.push(CharBox { ch, rect: rect(&bounds) });
    }
    Ok(out)
}

fn page_segments(page: &PdfPage) -> Vec<Segment> {
    let mut out = Vec::new();
    for object in page.objects().iter() {
        let Some(path) = object.as_path_object() else {
            continue;
        };
        let Ok(matrix) = path.matrix() else {
            continue;
        };
        let mut start: Option<(f32, f32)> = None;
        let mut current: Option<(f32, f32)> = None;
        for seg in path.segments().transform(matrix).iter() {
            let point = (seg.x().value, seg.y().value);
            match seg.segment_type() {
                PdfPathSegmentType::MoveTo => {
                    start = Some(point);
                }
                PdfPathSegmentType::LineTo => {
                    if let Some((x0, y0)) = current {
                        out.push(Segment { x0, y0, x1: point.0, y1: point.1 });
                    }
                }
                _ => {}
            }
            if seg.is_close()
                && let Some((sx, sy)) = start
            {
                out.push(Segment { x0: point.0, y0: point.1, x1: sx, y1: sy });
            }
            current = Some(point);
        }
    }
    out
}

impl PdfReader {
    pub fn load(library_dir: &Path) -> Result<Self, ParseError> {
        let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(library_dir))
            .map_err(|_| ParseError::Unsupported(PDF_LIBRARY_MISSING.to_string()))?;
        Ok(Self { pdfium: Pdfium::new(bindings), settings: TableSettings::default() })
    }

    pub fn read(&self, bytes: &[u8], source: FileId) -> Result<Document, ParseError> {
        let doc = self.pdfium.load_pdf_from_byte_slice(bytes, None).map_err(|e| match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => ParseError::Encrypted,
            _ => ParseError::Corrupted,
        })?;
        let fact = FactSource::File(source.clone());
        let mut blocks: Vec<Block> = Vec::new();
        let mut warnings: Vec<ExtractWarning> = Vec::new();
        let mut empty: Vec<u32> = Vec::new();
        let total = doc.pages().len();
        for (index, page) in doc.pages().iter().enumerate() {
            let number = u32::try_from(index).map_err(|_| ParseError::TooLarge)? + 1;
            let input = PageInput { number, chars: page_chars(&page)?, segments: page_segments(&page) };
            let output = lay_out(input, &fact, &self.settings);
            if !output.has_text {
                empty.push(number);
            }
            blocks.extend(output.blocks.into_iter().map(|(_, b)| b));
            warnings.extend(output.warnings);
        }
        if total > 0 && empty.len() == usize::from(total) {
            return Err(ParseError::EmptyText);
        }
        warnings.extend(ranges(&empty).into_iter().map(|(from, to)| ExtractWarning::PagesUnextracted { from, to }));
        Ok(Document { blocks, source, warnings })
    }
}

pub(crate) fn ranges(pages: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &p in pages {
        match out.last_mut() {
            Some((_, to)) if *to + 1 == p => *to = p,
            _ => out.push((p, p)),
        }
    }
    out
}
