use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use super::boxes::{PixelBox, find_boxes};
use super::error::{self, OcrError, Stage};
use super::raster::{Crop, Norm, Raster};

pub const DET_MAX_SIDE: u32 = 1280;
const STEP: u32 = 32;
const NORM: Norm = Norm { mean: [0.485, 0.456, 0.406], std: [0.229, 0.224, 0.225] };
const MIN_SIDE: u32 = 3;

pub(crate) struct Detector {
    session: Mutex<Session>,
    max_side: u32,
}

impl Detector {
    pub fn new(session: Session) -> Self {
        Self { session: Mutex::new(session), max_side: DET_MAX_SIDE }
    }

    pub fn with_max_side(mut self, side: u32) -> Self {
        self.max_side = side.max(STEP);
        self
    }

    fn input_size(&self, raster: &Raster) -> (u32, u32) {
        let ratio = (self.max_side as f32 / raster.width.max(raster.height) as f32).min(1.0);
        let fit = |v: u32| (((v as f32 * ratio) / STEP as f32).round() as u32).max(1) * STEP;
        (fit(raster.width), fit(raster.height))
    }

    pub fn boxes(&self, raster: &Raster) -> Result<Vec<PixelBox>, OcrError> {
        let (w, h) = self.input_size(raster);
        let data = raster.tensor(&Crop::plain(raster.whole()), w, h, NORM);
        let stage = Stage::Detection;
        let input = Tensor::from_array(([1usize, 3, h as usize, w as usize], data)).map_err(error::input(stage))?;
        let mut session = self.session.lock().map_err(error::poisoned(stage))?;
        let outputs = session.run(ort::inputs![input]).map_err(error::inference(stage))?;
        let (shape, prob) = outputs[0].try_extract_tensor::<f32>().map_err(error::inference(stage))?;
        let [_, _, ph, pw] = shape.as_ref() else { return Err(error::output_shape(stage, shape)) };
        let (pw, ph) = (*pw as u32, *ph as u32);
        let (sx, sy) = (raster.width as f32 / pw as f32, raster.height as f32 / ph as f32);
        Ok(find_boxes(prob, pw, ph)
            .into_iter()
            .map(|b| PixelBox {
                x0: (b.x0 as f32 * sx) as u32,
                y0: (b.y0 as f32 * sy) as u32,
                x1: ((b.x1 as f32 * sx).ceil() as u32).min(raster.width),
                y1: ((b.y1 as f32 * sy).ceil() as u32).min(raster.height),
            })
            .filter(|b| b.x1 >= b.x0 + MIN_SIDE && b.y1 >= b.y0 + MIN_SIDE)
            .collect())
    }
}
