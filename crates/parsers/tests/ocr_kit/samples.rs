use std::path::{Path, PathBuf};

use image::{DynamicImage, GrayImage};
use pdfium_render::prelude::*;

use super::degrade::Degrade;

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

pub const POINTS_PER_INCH: f32 = 72.0;
pub const SCAN_DPI: f32 = 200.0;
const PAGE: (f32, f32) = (595.0, 842.0);
const MARGIN: f32 = 60.0;
const FONT_SIZE: f32 = 10.5;
const LINE_STEP: f32 = 18.0;
const SUP_SCALE: f32 = 0.5;
const SUP_RISE: f32 = 0.45;
const GAP: f32 = 0.1;

pub fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
}

pub fn env_num<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

pub fn bind(lib: &Path) -> Result<Pdfium, PdfiumError> {
    Ok(Pdfium::new(Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(lib))?))
}

pub struct Scan {
    pub image: GrayImage,
    pub width_pt: f32,
    pub height_pt: f32,
    pub truth: String,
}

impl Scan {
    pub fn save(&self, pdfium: &Pdfium, out: &Path, name: &str) -> TestResult {
        let mut doc = pdfium.create_new_pdf()?;
        let size = PdfPagePaperSize::Custom(PdfPoints::new(self.width_pt), PdfPoints::new(self.height_pt));
        let mut page = doc.pages_mut().create_page_at_end(size)?;
        let img = DynamicImage::ImageLuma8(self.image.clone());
        let obj = PdfPageImageObject::new_with_width(&doc, &img, PdfPoints::new(self.width_pt))?;
        page.objects_mut().add_image_object(obj)?;
        doc.save_to_file(&out.join(format!("{name}.pdf")))?;
        std::fs::write(out.join(format!("{name}.txt")), &self.truth)?;
        Ok(())
    }
}

fn render(page: &PdfPage, dpi: f32) -> Result<GrayImage, PdfiumError> {
    let cfg = PdfRenderConfig::new().set_target_width((page.width().value * dpi / POINTS_PER_INCH) as i32);
    Ok(page.render_with_config(&cfg)?.as_image().to_luma8())
}

pub fn scan_pdf_page(page: &PdfPage, dpi: f32, degrade: &Degrade, seed: u64) -> Result<Scan, PdfiumError> {
    Ok(Scan {
        image: degrade.apply(&render(page, dpi)?, seed),
        width_pt: page.width().value,
        height_pt: page.height().value,
        truth: page.text()?.all(),
    })
}

pub fn lines_per_page() -> usize {
    ((PAGE.1 - 2.0 * MARGIN) / LINE_STEP) as usize
}

pub fn markup(line: &str) -> Vec<(&str, bool)> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some((before, after)) = rest.split_once("^{") {
        out.push((before, false));
        let (sup, tail) = after.split_once('}').unwrap_or((after, ""));
        out.push((sup, true));
        rest = tail;
    }
    out.push((rest, false));
    out.retain(|(t, _)| !t.is_empty());
    out
}

pub fn typeset(pdfium: &Pdfium, lines: &[String], font_path: &str, dpi: f32, degrade: &Degrade, seed: u64) -> Result<Scan, PdfiumError> {
    let mut doc = pdfium.create_new_pdf()?;
    let font = doc.fonts_mut().load_true_type_from_file(font_path, true)?;
    let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(PAGE.0), PdfPoints::new(PAGE.1)))?;
    let mut truth = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let base = PAGE.1 - MARGIN - LINE_STEP * (i as f32 + 1.0);
        let mut x = MARGIN;
        let mut plain = String::new();
        for (piece, sup) in markup(line) {
            let (size, rise) = if sup { (FONT_SIZE * SUP_SCALE, FONT_SIZE * SUP_RISE) } else { (FONT_SIZE, 0.0) };
            let mut obj = PdfPageTextObject::new(&doc, piece, font, PdfPoints::new(size))?;
            obj.translate(PdfPoints::new(x), PdfPoints::new(base + rise))?;
            let added = page.objects_mut().add_text_object(obj)?;
            x += added.width()?.value + FONT_SIZE * GAP;
            plain.push_str(piece);
        }
        truth.push(plain);
    }
    Ok(Scan { image: degrade.apply(&render(&page, dpi)?, seed), width_pt: PAGE.0, height_pt: PAGE.1, truth: truth.join("\n") })
}

#[cfg(test)]
mod tests {
    use super::markup;

    #[test]
    fn markup_splits_superscripts_and_keeps_order() {
        assert_eq!(markup("본다.^{2019다1} 다만"), vec![("본다.", false), ("2019다1", true), (" 다만", false)]);
        assert_eq!(markup("없음"), vec![("없음", false)]);
    }
}
