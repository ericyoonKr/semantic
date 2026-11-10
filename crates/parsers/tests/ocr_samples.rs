use std::path::{Path, PathBuf};
use std::time::Instant;

use docmodel::Block;
use parsers::{Ocr, PdfReader, file_id};
use pdfium_render::prelude::*;

const SCAN_DPI: f32 = 200.0;

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
}

fn squash(text: &str) -> Vec<char> {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn bag_recall(truth: &[char], got: &[char]) -> f64 {
    let mut pool = std::collections::HashMap::<char, i64>::new();
    for c in got {
        *pool.entry(*c).or_default() += 1;
    }
    let hit = truth
        .iter()
        .filter(|c| match pool.get_mut(c) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        })
        .count();
    hit as f64 / truth.len().max(1) as f64
}

fn doc_text(blocks: &[Block]) -> String {
    serde_json::to_value(blocks)
        .ok()
        .map(|v| collect_text(&v))
        .unwrap_or_default()
}

fn collect_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(m) => m
            .iter()
            .map(|(k, x)| if k == "text" { x.as_str().unwrap_or_default().to_string() + "\n" } else { collect_text(x) })
            .collect(),
        serde_json::Value::Array(a) => a.iter().map(collect_text).collect(),
        _ => String::new(),
    }
}

#[test]
#[ignore]
fn make_scanned_samples() -> Result<(), Box<dyn std::error::Error>> {
    let (Some(lib), Some(list), Some(out)) = (env_path("PDFIUM_DIR"), env_path("OCR_SAMPLE_LIST"), env_path("OCR_OUT")) else {
        return Ok(());
    };
    let pages_per_file: usize = std::env::var("OCR_PAGES").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    std::fs::create_dir_all(&out)?;
    let pdfium = Pdfium::new(Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&lib))?);
    let mut made = 0;
    for (fi, line) in std::fs::read_to_string(&list)?.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let src = pdfium.load_pdf_from_file(line.trim(), None)?;
        let total = src.pages().len() as usize;
        let picks: Vec<usize> = (1..total).take(pages_per_file).collect();
        for p in picks {
            let page = src.pages().get(p as u16)?;
            let truth = page.text()?.all();
            if squash(&truth).len() < 50 {
                continue;
            }
            let (w, h) = (page.width().value, page.height().value);
            let cfg = PdfRenderConfig::new().set_target_width((w * SCAN_DPI / 72.0) as i32);
            let image = page.render_with_config(&cfg)?.as_image().to_luma8();
            let mut doc = pdfium.create_new_pdf()?;
            let mut target = doc.pages_mut().create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(w), PdfPoints::new(h)))?;
            let mut obj = PdfPageImageObject::new_with_width(&doc, &image::DynamicImage::ImageLuma8(image), PdfPoints::new(w))?;
            obj.translate(PdfPoints::ZERO, PdfPoints::ZERO)?;
            target.objects_mut().add_image_object(obj)?;
            let name = format!("s{fi:02}_p{:03}", p + 1);
            doc.save_to_file(&out.join(format!("{name}.pdf")))?;
            std::fs::write(out.join(format!("{name}.txt")), truth)?;
            made += 1;
        }
    }
    println!("made {made}");
    Ok(())
}

#[test]
#[ignore]
fn make_synthetic_samples() -> Result<(), Box<dyn std::error::Error>> {
    let (Some(lib), Some(text), Some(fonts), Some(out)) =
        (env_path("PDFIUM_DIR"), env_path("OCR_TEXT_FILE"), std::env::var("OCR_FONTS").ok(), env_path("OCR_OUT"))
    else {
        return Ok(());
    };
    std::fs::create_dir_all(&out)?;
    let pdfium = Pdfium::new(Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&lib))?);
    let lines: Vec<String> = std::fs::read_to_string(&text)?.lines().map(str::to_string).collect();
    let fonts: Vec<&str> = fonts.split(';').filter(|f| !f.is_empty()).collect();
    let (w, h, margin, size, step) = (595.0f32, 842.0f32, 60.0f32, 10.5f32, 18.0f32);
    let per_page = ((h - 2.0 * margin) / step) as usize;
    for (pi, chunk) in lines.chunks(per_page).enumerate() {
        let font_path = fonts[pi % fonts.len()];
        let mut doc = pdfium.create_new_pdf()?;
        let font = doc.fonts_mut().load_true_type_from_file(font_path, true)?;
        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(w), PdfPoints::new(h)))?;
        for (li, line) in chunk.iter().enumerate() {
            let mut obj = PdfPageTextObject::new(&doc, line, font, PdfPoints::new(size))?;
            obj.translate(PdfPoints::new(margin), PdfPoints::new(h - margin - step * (li as f32 + 1.0)))?;
            page.objects_mut().add_text_object(obj)?;
        }
        let cfg = PdfRenderConfig::new().set_target_width((w * SCAN_DPI / 72.0) as i32);
        let image = page.render_with_config(&cfg)?.as_image().to_luma8();
        let mut scan = pdfium.create_new_pdf()?;
        let mut target = scan.pages_mut().create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(w), PdfPoints::new(h)))?;
        let obj = PdfPageImageObject::new_with_width(&scan, &image::DynamicImage::ImageLuma8(image), PdfPoints::new(w))?;
        target.objects_mut().add_image_object(obj)?;
        let name = format!("t{pi:02}");
        scan.save_to_file(&out.join(format!("{name}.pdf")))?;
        std::fs::write(out.join(format!("{name}.txt")), chunk.join("\n"))?;
    }
    println!("pages {}", lines.len().div_ceil(per_page));
    Ok(())
}

#[test]
#[ignore]
fn measure_ocr_on_scanned_samples() -> Result<(), Box<dyn std::error::Error>> {
    let (Some(lib), Some(models), Some(out)) = (env_path("PDFIUM_DIR"), env_path("OCR_DIR"), env_path("OCR_OUT")) else {
        return Ok(());
    };
    let threads: usize = std::env::var("OCR_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    let side: u32 = std::env::var("OCR_DET_SIDE").ok().and_then(|v| v.parse().ok()).unwrap_or(parsers::DET_MAX_SIDE);
    let reader = PdfReader::load(&lib)?.with_ocr(Ocr::load(&models, threads)?.with_det_max_side(side));
    let mut files: Vec<PathBuf> = std::fs::read_dir(&out)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    files.sort();
    let (mut chars, mut errors, mut recall_sum, mut secs) = (0usize, 0usize, 0f64, 0f64);
    for path in &files {
        let truth = squash(&std::fs::read_to_string(path.with_extension("txt"))?);
        let bytes = std::fs::read(path)?;
        let t = Instant::now();
        let got = match reader.read(&bytes, file_id(&bytes)) {
            Ok(doc) => doc_text(&doc.blocks),
            Err(e) => format!("ERROR {e}"),
        };
        let took = t.elapsed().as_secs_f64();
        std::fs::write(path.with_extension("ocr.txt"), &got)?;
        let got = squash(&got);
        let d = edit_distance(&truth, &got);
        let recall = bag_recall(&truth, &got);
        println!(
            "{}\tchars {}\tCER {:.3}\tcharbag recall {:.3}\t{:.1}s",
            name(path),
            truth.len(),
            d as f64 / truth.len().max(1) as f64,
            recall,
            took
        );
        chars += truth.len();
        errors += d;
        recall_sum += recall * truth.len() as f64;
        secs += took;
    }
    println!(
        "TOTAL pages {}\tchars {}\tCER {:.3}\tcharbag recall {:.3}\tavg {:.1}s/page",
        files.len(),
        chars,
        errors as f64 / chars.max(1) as f64,
        recall_sum / chars.max(1) as f64,
        secs / files.len().max(1) as f64
    );
    Ok(())
}

fn name(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

#[test]
#[ignore]
fn dump_page_top_as_png() -> Result<(), Box<dyn std::error::Error>> {
    let (Some(lib), Some(pdf), Some(png)) = (env_path("PDFIUM_DIR"), env_path("OCR_DUMP_PDF"), env_path("OCR_DUMP_PNG")) else {
        return Ok(());
    };
    let frac: f32 = std::env::var("OCR_DUMP_FRAC").ok().and_then(|v| v.parse().ok()).unwrap_or(0.25);
    let pdfium = Pdfium::new(Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&lib))?);
    let doc = pdfium.load_pdf_from_file(&pdf, None)?;
    let page = doc.pages().get(0)?;
    let cfg = PdfRenderConfig::new().set_target_width((page.width().value * SCAN_DPI / 72.0) as i32);
    let img = page.render_with_config(&cfg)?.as_image().to_luma8();
    let (w, h) = img.dimensions();
    let top = image::imageops::crop_imm(&img, 0, 0, w, (h as f32 * frac) as u32).to_image();
    top.save(&png)?;
    Ok(())
}
