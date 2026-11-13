mod boxes;
mod check;
mod components;
mod ctc;
mod det;
pub mod error;
mod fix;
mod raster;
mod rec;
mod split;

use std::path::Path;

use ort::environment::Environment;
use ort::session::Session;
use pdfium_render::prelude::*;

use det::Detector;
use error::{OcrError, Stage};
use raster::Raster;
use rec::Recognizer;
use split::{Kind, segments};

pub use det::DET_MAX_SIDE;
pub use rec::REC_WIDTH_SCALE;

const DET_FILE: &str = "det.onnx";
const REC_FILE: &str = "rec.onnx";
const DICT_FILE: &str = "dict.txt";
const MIN_LINE_SCORE: f32 = 0.5;
const MIN_SUPERSCRIPT_DIGITS: usize = 2;
const REREAD_STRETCHES: [f32; 3] = [1.25, 1.7, 0.85];
pub const OCR_RENDER_DPI: f32 = 300.0;
const POINTS_PER_INCH: f32 = 72.0;

#[derive(Debug, Clone, PartialEq)]
pub struct OcrLine {
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracePart {
    pub area: [u32; 4],
    pub superscript: bool,
    pub text: String,
    pub score: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceLine {
    pub area: [u32; 4],
    pub parts: Vec<TracePart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcrOptions {
    pub split_superscripts: bool,
    pub fix_text: bool,
    pub reread_suspicious: bool,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self { split_superscripts: true, fix_text: true, reread_suspicious: true }
    }
}

pub(crate) struct Rendered {
    raster: Raster,
    width_pt: f32,
    height_pt: f32,
}

impl Rendered {
    pub(crate) fn of(page: &PdfPage) -> Result<Self, OcrError> {
        let (width_pt, height_pt) = (page.width().value, page.height().value);
        let scale = OCR_RENDER_DPI / POINTS_PER_INCH;
        let raster = Raster::render(page, width_pt * scale, height_pt * scale)?;
        Ok(Self { raster, width_pt, height_pt })
    }
}

pub struct Ocr {
    det: Detector,
    rec: Recognizer,
    options: OcrOptions,
}

static ENVIRONMENT: std::sync::OnceLock<Result<Environment, String>> = std::sync::OnceLock::new();

fn environment() -> Result<&'static Environment, OcrError> {
    ENVIRONMENT.get_or_init(|| ort::init().build().map_err(|e| e.to_string())).as_ref().map_err(error::environment)
}

fn session(env: &Environment, stage: Stage, path: &Path, threads: usize) -> Result<Session, OcrError> {
    let mut builder = Session::builder(env)
        .map_err(error::session(stage, path))?
        .with_intra_threads(threads)
        .map_err(error::session(stage, path))?;
    builder.commit_from_file(path).map_err(error::session(stage, path))
}

fn plausible_superscript(part: &TracePart) -> bool {
    part.text.chars().filter(char::is_ascii_digit).count() >= MIN_SUPERSCRIPT_DIGITS
}

fn joined(parts: &[TracePart]) -> String {
    parts.iter().map(|p| p.text.as_str()).collect()
}

fn corners(b: &boxes::PixelBox) -> [u32; 4] {
    [b.x0, b.y0, b.x1, b.y1]
}

impl Ocr {
    pub fn load(dir: &Path, threads: usize) -> Result<Self, OcrError> {
        let env = environment()?;
        let det = Detector::new(session(env, Stage::Detection, &dir.join(DET_FILE), threads)?);
        let rec = Recognizer::new(session(env, Stage::Recognition, &dir.join(REC_FILE), threads)?, &dir.join(DICT_FILE))?;
        Ok(Self { det, rec, options: OcrOptions::default() })
    }

    pub fn with_det_max_side(mut self, side: u32) -> Self {
        self.det = self.det.with_max_side(side);
        self
    }

    pub fn with_rec_width_scale(mut self, scale: f32) -> Self {
        self.rec = self.rec.with_width_scale(scale);
        self
    }

    pub fn with_options(mut self, options: OcrOptions) -> Self {
        self.options = options;
        self
    }

    pub fn read_page(&self, page: &PdfPage) -> Result<Vec<OcrLine>, OcrError> {
        self.read_rendered(&Rendered::of(page)?)
    }

    pub(crate) fn read_rendered(&self, rendered: &Rendered) -> Result<Vec<OcrLine>, OcrError> {
        let Rendered { raster, width_pt, height_pt } = rendered;
        let (width_pt, height_pt) = (*width_pt, *height_pt);
        let to_x = width_pt / raster.width as f32;
        let to_y = height_pt / raster.height as f32;
        Ok(self
            .read_raster(raster)?
            .into_iter()
            .map(|l| OcrLine {
                x0: l.x0 * to_x,
                x1: l.x1 * to_x,
                y0: height_pt - l.y1 * to_y,
                y1: height_pt - l.y0 * to_y,
                ..l
            })
            .collect())
    }

    pub fn trace_page(&self, page: &PdfPage) -> Result<Vec<TraceLine>, OcrError> {
        let scale = OCR_RENDER_DPI / POINTS_PER_INCH;
        let raster = Raster::render(page, page.width().value * scale, page.height().value * scale)?;
        let mut out = Vec::new();
        for line in self.det.boxes(&raster)? {
            out.push(TraceLine { area: corners(&line), parts: self.read_parts(&raster, &line, 1.0)? });
        }
        Ok(out)
    }

    fn read_segments(
        &self,
        raster: &Raster,
        segments: &[split::Segment],
        stretch: f32,
    ) -> Result<Vec<TracePart>, OcrError> {
        let mut out = Vec::with_capacity(segments.len());
        for seg in segments {
            let Some((text, score)) = self.rec.read(raster, &seg.crop, seg.kind.stretch() * stretch)? else { continue };
            out.push(TracePart { area: corners(&seg.crop.area), superscript: seg.kind == Kind::Superscript, text, score });
        }
        Ok(out)
    }

    fn read_parts(&self, raster: &Raster, line: &boxes::PixelBox, stretch: f32) -> Result<Vec<TracePart>, OcrError> {
        let whole = [split::Segment::whole(line)];
        if !self.options.split_superscripts {
            return self.read_segments(raster, &whole, stretch);
        }
        let parts = self.read_segments(raster, &segments(raster, line), stretch)?;
        if parts.iter().filter(|p| p.superscript).all(plausible_superscript) {
            Ok(parts)
        } else {
            self.read_segments(raster, &whole, stretch)
        }
    }

    fn best_parts(&self, raster: &Raster, line: &boxes::PixelBox) -> Result<Vec<TracePart>, OcrError> {
        let mut best = self.read_parts(raster, line, 1.0)?;
        let mut doubts = check::suspicious(&joined(&best));
        if !self.options.reread_suspicious {
            return Ok(best);
        }
        for stretch in REREAD_STRETCHES {
            if doubts == 0 {
                break;
            }
            let again = self.read_parts(raster, line, stretch)?;
            let again_doubts = check::suspicious(&joined(&again));
            if again_doubts < doubts {
                best = again;
                doubts = again_doubts;
            }
        }
        Ok(best)
    }

    fn read_line(&self, raster: &Raster, line: &boxes::PixelBox) -> Result<Option<(String, f32)>, OcrError> {
        let parts = self.best_parts(raster, line)?;
        let chars: usize = parts.iter().map(|p| p.text.chars().count()).sum();
        if chars == 0 {
            return Ok(None);
        }
        let weighted: f32 = parts.iter().map(|p| p.score * p.text.chars().count() as f32).sum();
        let mut text: String = parts.into_iter().map(|p| p.text).collect();
        if self.options.fix_text {
            text = fix::correct(&text);
        }
        Ok(Some((text, weighted / chars as f32)))
    }

    fn read_raster(&self, raster: &Raster) -> Result<Vec<OcrLine>, OcrError> {
        let mut out = Vec::new();
        for b in self.det.boxes(raster)? {
            let Some((text, score)) = self.read_line(raster, &b)? else { continue };
            if score < MIN_LINE_SCORE || text.trim().is_empty() {
                continue;
            }
            out.push(OcrLine { text, x0: b.x0 as f32, y0: b.y0 as f32, x1: b.x1 as f32, y1: b.y1 as f32, score });
        }
        out.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
        Ok(out)
    }
}

