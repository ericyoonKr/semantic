mod layout;
mod words;

#[cfg(test)]
mod tests;

use std::path::Path;

use docmodel::{Block, Document, ExtractWarning, FactSource, FileId, Rect};
use pdfium_render::prelude::*;

use crate::error::ParseError;
use crate::ocr::error::{log_page_failure, log_page_unread};
use crate::ocr::{Ocr, OcrLine};
use crate::pdf_table::{Segment, TableSettings};
use layout::{PageInput, lay_out};
use words::CharBox;

pub const PDF_LIBRARY_MISSING: &str = "PDF 읽기 모듈을 찾지 못했습니다";

pub struct PdfReader {
    pdfium: Pdfium,
    settings: TableSettings,
    ocr: Option<Ocr>,
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

const THIN: f32 = 2.0;

fn path_points(path: &PdfPagePathObject) -> Vec<(PdfPathSegmentType, bool, (f32, f32))> {
    let Ok(matrix) = path.matrix() else {
        return Vec::new();
    };
    path.segments()
        .transform(matrix)
        .iter()
        .map(|seg| (seg.segment_type(), seg.is_close(), (seg.x().value, seg.y().value)))
        .collect()
}

fn stroked_lines(points: &[(PdfPathSegmentType, bool, (f32, f32))], out: &mut Vec<Segment>) {
    let mut start: Option<(f32, f32)> = None;
    let mut current: Option<(f32, f32)> = None;
    for (kind, close, point) in points {
        match kind {
            PdfPathSegmentType::MoveTo => start = Some(*point),
            PdfPathSegmentType::LineTo => {
                if let Some((x0, y0)) = current {
                    out.push(Segment { x0, y0, x1: point.0, y1: point.1 });
                }
            }
            _ => {}
        }
        if *close && let Some((sx, sy)) = start {
            out.push(Segment { x0: point.0, y0: point.1, x1: sx, y1: sy });
        }
        current = Some(*point);
    }
}

fn thin_fill(points: &[(PdfPathSegmentType, bool, (f32, f32))]) -> Option<Segment> {
    let xs = points.iter().map(|p| p.2.0);
    let ys = points.iter().map(|p| p.2.1);
    let (x0, x1) = xs.fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v), b.max(v)));
    let (y0, y1) = ys.fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v), b.max(v)));
    let (w, h) = (x1 - x0, y1 - y0);
    if !(w.is_finite() && h.is_finite()) {
        return None;
    }
    if w <= THIN && h > THIN {
        let x = (x0 + x1) / 2.0;
        Some(Segment { x0: x, y0, x1: x, y1 })
    } else if h <= THIN && w > THIN {
        let y = (y0 + y1) / 2.0;
        Some(Segment { x0, y0: y, x1, y1: y })
    } else {
        None
    }
}

fn page_segments(page: &PdfPage) -> Vec<Segment> {
    let mut out = Vec::new();
    for object in page.objects().iter() {
        let Some(path) = object.as_path_object() else {
            continue;
        };
        let points = path_points(path);
        if points.is_empty() {
            continue;
        }
        if path.is_stroked().unwrap_or(false) {
            stroked_lines(&points, &mut out);
        } else if !matches!(path.fill_mode(), Ok(PdfPathFillMode::None) | Err(_))
            && let Some(line) = thin_fill(&points)
        {
            out.push(line);
        }
    }
    out
}

impl PdfReader {
    pub fn load(library_dir: &Path) -> Result<Self, ParseError> {
        let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(library_dir))
            .map_err(|_| ParseError::Unsupported(PDF_LIBRARY_MISSING.to_string()))?;
        Ok(Self { pdfium: Pdfium::new(bindings), settings: TableSettings::default(), ocr: None })
    }

    pub fn with_ocr(mut self, ocr: Ocr) -> Self {
        self.ocr = Some(ocr);
        self
    }

    pub fn read(&self, bytes: &[u8], source: FileId) -> Result<Document, ParseError> {
        self.read_with(bytes, source, true)
    }

    pub fn read_quick(&self, bytes: &[u8], source: FileId) -> Result<Document, ParseError> {
        self.read_with(bytes, source, false)
    }

    pub fn can_recognize(&self) -> bool {
        self.ocr.is_some()
    }

    fn read_with(&self, bytes: &[u8], source: FileId, recognize: bool) -> Result<Document, ParseError> {
        let doc = self.pdfium.load_pdf_from_byte_slice(bytes, None).map_err(|e| match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => ParseError::Encrypted,
            _ => ParseError::Corrupted,
        })?;
        let fact = FactSource::File(source.clone());
        let mut blocks: Vec<Block> = Vec::new();
        let mut warnings: Vec<ExtractWarning> = Vec::new();
        let mut empty: Vec<u32> = Vec::new();
        let mut recognized: Vec<u32> = Vec::new();
        let total = doc.pages().len();
        for (index, page) in doc.pages().iter().enumerate() {
            let number = u32::try_from(index).map_err(|_| ParseError::TooLarge)? + 1;
            let input = PageInput { number, chars: page_chars(&page)?, segments: page_segments(&page) };
            let mut output = lay_out(input, &fact, &self.settings);
            if !output.has_text
                && recognize
                && let Some(ocr) = &self.ocr
            {
                match ocr.read_page(&page) {
                    Ok(lines) => {
                        let input = PageInput { number, chars: line_chars(&lines), segments: page_segments(&page) };
                        let again = lay_out(input, &fact, &self.settings);
                        if again.has_text {
                            output = again;
                            recognized.push(number);
                        } else {
                            log_page_unread(number, lines.len());
                        }
                    }
                    Err(e) => log_page_failure(&e, number),
                }
            }
            if !output.has_text {
                empty.push(number);
            }
            blocks.extend(output.blocks.into_iter().map(|(_, b)| b));
            warnings.extend(output.warnings);
        }
        if recognize && total > 0 && empty.len() == usize::from(total) {
            return Err(ParseError::EmptyText);
        }
        warnings.extend(ranges(&empty).into_iter().map(|(from, to)| ExtractWarning::PagesUnextracted { from, to }));
        warnings.extend(ranges(&recognized).into_iter().map(|(from, to)| ExtractWarning::PagesRecognized { from, to }));
        Ok(Document { blocks, source, warnings })
    }
}

fn line_chars(lines: &[OcrLine]) -> Vec<CharBox> {
    let mut out = Vec::new();
    for line in lines {
        let count = line.text.chars().count().max(1) as f32;
        let step = (line.x1 - line.x0) / count;
        for (i, ch) in line.text.chars().enumerate() {
            let x0 = line.x0 + step * i as f32;
            out.push(CharBox { ch, rect: Rect { x0, y0: line.y0, x1: x0 + step, y1: line.y1 } });
        }
    }
    out
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
