use pdfium_render::prelude::*;

use super::boxes::PixelBox;
use super::error::{self, OcrError};

const WHITE: [u8; 3] = [255, 255, 255];

pub(crate) struct Raster {
    pub width: u32,
    pub height: u32,
    rgb: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Crop {
    pub area: PixelBox,
    pub pad: u32,
}

impl Crop {
    pub fn plain(area: PixelBox) -> Self {
        Self { area, pad: 0 }
    }

    fn shows(&self, x: f32, y: f32) -> bool {
        let a = &self.area;
        x >= a.x0 as f32 && y >= a.y0 as f32 && x < a.x1 as f32 && y < a.y1 as f32
    }

    pub fn padded_width(&self) -> u32 {
        self.area.width() + 2 * self.pad
    }

    pub fn padded_height(&self) -> u32 {
        self.area.height() + 2 * self.pad
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Norm {
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

impl Raster {
    pub fn render(page: &PdfPage, width: f32, height: f32) -> Result<Self, OcrError> {
        let config = PdfRenderConfig::new()
            .set_target_width(width.round() as i32)
            .set_maximum_height(height.round() as i32);
        let bitmap = page.render_with_config(&config).map_err(error::render)?;
        let image = bitmap.as_image().to_rgb8();
        let (width, height) = image.dimensions();
        Ok(Self { width, height, rgb: image.into_raw() })
    }

    #[cfg(test)]
    pub fn from_rgb(width: u32, height: u32, rgb: Vec<u8>) -> Self {
        Self { width, height, rgb }
    }

    pub fn whole(&self) -> PixelBox {
        PixelBox { x0: 0, y0: 0, x1: self.width, y1: self.height }
    }

    pub fn luma(&self, x: u32, y: u32) -> u8 {
        let [r, g, b] = self.pixel(x, y);
        ((u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000) as u8
    }

    fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let x = x.min(self.width.saturating_sub(1));
        let y = y.min(self.height.saturating_sub(1));
        let i = ((y * self.width + x) * 3) as usize;
        match self.rgb.get(i..i + 3) {
            Some([r, g, b]) => [*r, *g, *b],
            _ => WHITE,
        }
    }

    pub fn tensor(&self, crop: &Crop, width: u32, height: u32, norm: Norm) -> Vec<f32> {
        let plane = (width * height) as usize;
        let mut data = vec![0f32; 3 * plane];
        let pad = crop.pad as f32;
        let (ox, oy) = (crop.area.x0 as f32 - pad, crop.area.y0 as f32 - pad);
        let (rw, rh) = (crop.padded_width() as f32, crop.padded_height() as f32);
        for y in 0..height {
            let sy = oy + (y as f32 + 0.5) * rh / height as f32;
            for x in 0..width {
                let sx = ox + (x as f32 + 0.5) * rw / width as f32;
                let [r, g, b] = if crop.shows(sx, sy) { self.pixel(sx as u32, sy as u32) } else { WHITE };
                let i = (y * width + x) as usize;
                for (c, v) in [b, g, r].into_iter().enumerate() {
                    data[c * plane + i] = (v as f32 / 255.0 - norm.mean[c]) / norm.std[c];
                }
            }
        }
        data
    }
}

#[cfg(test)]
mod tests {
    use super::{Crop, Norm, Raster};
    use crate::ocr::boxes::PixelBox;

    #[test]
    fn tensor_is_bgr_planar_and_normalized() {
        let raster = Raster::from_rgb(1, 1, vec![255, 0, 51]);
        let norm = Norm { mean: [0.0; 3], std: [1.0; 3] };
        let t = raster.tensor(&Crop::plain(PixelBox { x0: 0, y0: 0, x1: 1, y1: 1 }), 1, 1, norm);
        assert_eq!(t, vec![0.2, 0.0, 1.0]);
    }

    #[test]
    fn padding_samples_white_around_the_region_instead_of_neighbours() {
        let raster = Raster::from_rgb(3, 1, vec![0, 0, 0, 10, 10, 10, 0, 0, 0]);
        let norm = Norm { mean: [0.0; 3], std: [1.0; 3] };
        let crop = Crop { area: PixelBox { x0: 1, y0: 0, x1: 2, y1: 1 }, pad: 1 };
        let blue: Vec<f32> = raster.tensor(&crop, 3, 3, norm)[..9].to_vec();
        assert_eq!(blue[3], 1.0);
        assert_eq!(blue[5], 1.0);
        assert!((blue[4] - 10.0 / 255.0).abs() < 1e-6);
    }

}
