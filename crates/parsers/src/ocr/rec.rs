use std::fs;
use std::path::Path;
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;

use super::ctc::decode;
use super::error::{self, OcrError, Stage};
use super::raster::{Crop, Norm, Raster};

pub const REC_WIDTH_SCALE: f32 = 1.0;
const HEIGHT: u32 = 48;
const MIN_WIDTH: u32 = 8;
const MAX_WIDTH: u32 = 4800;
const NORM: Norm = Norm { mean: [0.5; 3], std: [0.5; 3] };

pub(crate) struct Recognizer {
    session: Mutex<Session>,
    dict: Vec<String>,
    width_scale: f32,
}

impl Recognizer {
    pub fn new(session: Session, dict_path: &Path) -> Result<Self, OcrError> {
        let text = fs::read_to_string(dict_path).map_err(error::model_file(dict_path))?;
        let mut dict: Vec<String> = text.lines().map(str::to_string).collect();
        dict.push(" ".to_string());
        Ok(Self { session: Mutex::new(session), dict, width_scale: REC_WIDTH_SCALE })
    }

    pub fn with_width_scale(mut self, scale: f32) -> Self {
        self.width_scale = scale.clamp(0.5, 4.0);
        self
    }

    fn input_width(&self, crop: &Crop, stretch: f32) -> u32 {
        let ratio = crop.padded_width() as f32 / crop.padded_height().max(1) as f32;
        ((HEIGHT as f32 * ratio * self.width_scale * stretch).ceil() as u32).clamp(MIN_WIDTH, MAX_WIDTH)
    }

    pub fn read(&self, raster: &Raster, crop: &Crop, stretch: f32) -> Result<Option<(String, f32)>, OcrError> {
        let w = self.input_width(crop, stretch);
        let data = raster.tensor(crop, w, HEIGHT, NORM);
        let stage = Stage::Recognition;
        let input = Tensor::from_array(([1usize, 3, HEIGHT as usize, w as usize], data)).map_err(error::input(stage))?;
        let mut session = self.session.lock().map_err(error::poisoned(stage))?;
        let outputs = session.run(ort::inputs![input]).map_err(error::inference(stage))?;
        let (shape, logits) = outputs[0].try_extract_tensor::<f32>().map_err(error::inference(stage))?;
        let [_, steps, classes] = shape.as_ref() else { return Err(error::output_shape(stage, shape)) };
        Ok(decode(logits, *steps as usize, *classes as usize, &self.dict))
    }
}
