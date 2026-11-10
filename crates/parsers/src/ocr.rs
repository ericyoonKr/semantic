mod boxes;

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;
use pdfium_render::prelude::*;

use crate::error::ParseError;
use boxes::{PixelBox, find_boxes};

pub const OCR_MISSING: &str = "글자 인식 모델을 찾지 못했습니다";

const DET_FILE: &str = "det.onnx";
const REC_FILE: &str = "rec.onnx";
const DICT_FILE: &str = "dict.txt";
pub const DET_MAX_SIDE: u32 = 1280;
const DET_STEP: u32 = 32;
const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];
const REC_HEIGHT: u32 = 48;
const REC_MAX_WIDTH: u32 = 3200;
const MIN_LINE_SCORE: f32 = 0.5;
pub const RENDER_SCALE: f32 = 300.0 / 72.0;

#[derive(Debug, Clone, PartialEq)]
pub struct OcrLine {
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub score: f32,
}

pub struct Ocr {
    _env: Environment,
    det: Mutex<Session>,
    rec: Mutex<Session>,
    dict: Vec<String>,
    det_max_side: u32,
}

struct Raster {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl Raster {
    fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

fn missing<E>(_: E) -> ParseError {
    ParseError::Unsupported(OCR_MISSING.to_string())
}

fn session(env: &Environment, path: &Path, threads: usize) -> Result<Session, ParseError> {
    let mut builder = Session::builder(env).map_err(missing)?.with_intra_threads(threads).map_err(missing)?;
    builder.commit_from_file(path).map_err(missing)
}

impl Ocr {
    pub fn load(dir: &Path, threads: usize) -> Result<Self, ParseError> {
        let dict_text =
            fs::read_to_string(dir.join(DICT_FILE)).map_err(|_| ParseError::Unsupported(OCR_MISSING.to_string()))?;
        let mut dict: Vec<String> = dict_text.lines().map(str::to_string).collect();
        dict.push(" ".to_string());
        let env = ort::init().build().map_err(missing)?;
        Ok(Self {
            det: Mutex::new(session(&env, &dir.join(DET_FILE), threads)?),
            rec: Mutex::new(session(&env, &dir.join(REC_FILE), threads)?),
            _env: env,
            dict,
            det_max_side: DET_MAX_SIDE,
        })
    }

    pub fn with_det_max_side(mut self, side: u32) -> Self {
        self.det_max_side = side.max(DET_STEP);
        self
    }

    pub fn read_page(&self, page: &PdfPage) -> Result<Vec<OcrLine>, ParseError> {
        let width_pt = page.width().value;
        let height_pt = page.height().value;
        let config = PdfRenderConfig::new()
            .set_target_width((width_pt * RENDER_SCALE).round() as i32)
            .set_maximum_height((height_pt * RENDER_SCALE).round() as i32);
        let bitmap = page.render_with_config(&config).map_err(|_| ParseError::Corrupted)?;
        let image = bitmap.as_image().to_rgb8();
        let (width, height) = image.dimensions();
        let raster = Raster { width, height, rgb: image.into_raw() };
        let scale_x = width as f32 / width_pt;
        let scale_y = height as f32 / height_pt;
        let lines = self.read_image(&raster)?;
        Ok(lines
            .into_iter()
            .map(|l| OcrLine {
                text: l.text,
                x0: l.x0 / scale_x,
                x1: l.x1 / scale_x,
                y0: height_pt - l.y1 / scale_y,
                y1: height_pt - l.y0 / scale_y,
                score: l.score,
            })
            .collect())
    }

    fn read_image(&self, img: &Raster) -> Result<Vec<OcrLine>, ParseError> {
        let boxes = self.detect(img)?;
        let mut out = Vec::with_capacity(boxes.len());
        for b in boxes {
            let Some((text, score)) = self.recognize(img, &b)? else { continue };
            if score < MIN_LINE_SCORE || text.trim().is_empty() {
                continue;
            }
            out.push(OcrLine {
                text,
                x0: b.x0 as f32,
                y0: b.y0 as f32,
                x1: b.x1 as f32,
                y1: b.y1 as f32,
                score,
            });
        }
        out.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
        Ok(out)
    }

    fn detect(&self, img: &Raster) -> Result<Vec<PixelBox>, ParseError> {
        let ratio = (self.det_max_side as f32 / img.width.max(img.height) as f32).min(1.0);
        let fit = |v: u32| (((v as f32 * ratio) / DET_STEP as f32).round() as u32).max(1) * DET_STEP;
        let (w, h) = (fit(img.width), fit(img.height));
        let mut data = vec![0f32; (3 * w * h) as usize];
        let plane = (w * h) as usize;
        for y in 0..h {
            let sy = ((y as f32 + 0.5) * img.height as f32 / h as f32) as u32;
            for x in 0..w {
                let sx = ((x as f32 + 0.5) * img.width as f32 / w as f32) as u32;
                let px = img.pixel(sx.min(img.width - 1), sy.min(img.height - 1));
                let bgr = [px[2], px[1], px[0]];
                let i = (y * w + x) as usize;
                for c in 0..3 {
                    data[c * plane + i] = (bgr[c] as f32 / 255.0 - DET_MEAN[c]) / DET_STD[c];
                }
            }
        }
        let input = Tensor::from_array(([1usize, 3, h as usize, w as usize], data)).map_err(|_| ParseError::Corrupted)?;
        let mut det = self.det.lock().map_err(|_| ParseError::Corrupted)?;
        let outputs = det.run(ort::inputs![input]).map_err(|_| ParseError::Corrupted)?;
        let (shape, prob) = outputs[0].try_extract_tensor::<f32>().map_err(|_| ParseError::Corrupted)?;
        let (ph, pw) = match shape.as_ref() {
            [_, _, ph, pw] => (*ph as u32, *pw as u32),
            _ => return Err(ParseError::Corrupted),
        };
        let boxes = find_boxes(prob, pw, ph);
        let sx = img.width as f32 / pw as f32;
        let sy = img.height as f32 / ph as f32;
        Ok(boxes
            .into_iter()
            .map(|b| PixelBox {
                x0: ((b.x0 as f32 * sx) as u32).min(img.width - 1),
                y0: ((b.y0 as f32 * sy) as u32).min(img.height - 1),
                x1: ((b.x1 as f32 * sx).ceil() as u32).min(img.width),
                y1: ((b.y1 as f32 * sy).ceil() as u32).min(img.height),
            })
            .filter(|b| b.x1 > b.x0 + 2 && b.y1 > b.y0 + 2)
            .collect())
    }

    fn recognize(&self, img: &Raster, b: &PixelBox) -> Result<Option<(String, f32)>, ParseError> {
        let bw = b.x1 - b.x0;
        let bh = b.y1 - b.y0;
        let w = ((REC_HEIGHT as f32 * bw as f32 / bh as f32).ceil() as u32).clamp(8, REC_MAX_WIDTH);
        let h = REC_HEIGHT;
        let plane = (w * h) as usize;
        let mut data = vec![0f32; 3 * plane];
        for y in 0..h {
            let sy = b.y0 + ((y as f32 + 0.5) * bh as f32 / h as f32) as u32;
            for x in 0..w {
                let sx = b.x0 + ((x as f32 + 0.5) * bw as f32 / w as f32) as u32;
                let px = img.pixel(sx.min(img.width - 1), sy.min(img.height - 1));
                let bgr = [px[2], px[1], px[0]];
                let i = (y * w + x) as usize;
                for c in 0..3 {
                    data[c * plane + i] = (bgr[c] as f32 / 255.0 - 0.5) / 0.5;
                }
            }
        }
        let input = Tensor::from_array(([1usize, 3, h as usize, w as usize], data)).map_err(|_| ParseError::Corrupted)?;
        let mut rec = self.rec.lock().map_err(|_| ParseError::Corrupted)?;
        let outputs = rec.run(ort::inputs![input]).map_err(|_| ParseError::Corrupted)?;
        let (shape, logits) = outputs[0].try_extract_tensor::<f32>().map_err(|_| ParseError::Corrupted)?;
        let (steps, classes) = match shape.as_ref() {
            [_, t, c] => (*t as usize, *c as usize),
            _ => return Err(ParseError::Corrupted),
        };
        Ok(decode(logits, steps, classes, &self.dict))
    }
}

fn decode(logits: &[f32], steps: usize, classes: usize, dict: &[String]) -> Option<(String, f32)> {
    let mut text = String::new();
    let mut probs = Vec::new();
    let mut last = 0usize;
    for t in 0..steps {
        let row = logits.get(t * classes..(t + 1) * classes)?;
        let (best, p) = row
            .iter()
            .enumerate()
            .fold((0usize, f32::MIN), |acc, (i, v)| if *v > acc.1 { (i, *v) } else { acc });
        if best != 0 && best != last {
            if let Some(ch) = dict.get(best - 1) {
                text.push_str(ch);
                probs.push(p);
            }
        }
        last = best;
    }
    if probs.is_empty() {
        return None;
    }
    let score = probs.iter().sum::<f32>() / probs.len() as f32;
    Some((text, score))
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn ctc_decode_drops_blanks_and_repeats_and_maps_last_index_to_space() {
        let dict: Vec<String> = ["가", "나", " "].iter().map(|s| s.to_string()).collect();
        let classes = 4;
        let one_hot = |i: usize| {
            let mut row = vec![0.0f32; classes];
            row[i] = 0.9;
            row
        };
        let logits: Vec<f32> = [1, 1, 0, 1, 3, 2].iter().flat_map(|&i| one_hot(i)).collect();
        let (text, score) = decode(&logits, 6, classes, &dict).unwrap_or_default();
        assert_eq!(text, "가가 나");
        assert!((score - 0.9).abs() < 1e-6);
    }
}
