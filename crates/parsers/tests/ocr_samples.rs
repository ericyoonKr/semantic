#[path = "ocr_kit/degrade.rs"]
mod degrade;
#[path = "ocr_kit/metrics.rs"]
mod metrics;
#[path = "ocr_kit/samples.rs"]
mod samples;
#[path = "ocr_kit/trace.rs"]
mod trace;

use std::path::PathBuf;
use std::time::Instant;

use docmodel::Block;
use parsers::{DET_MAX_SIDE, OCR_RENDER_DPI, Ocr, OcrOptions, PdfReader, REC_WIDTH_SCALE, file_id};
use pdfium_render::prelude::*;

use degrade::Degrade;
use metrics::{Score, compare};
use samples::{POINTS_PER_INCH, SCAN_DPI, TestResult, bind, env_num, env_path, lines_per_page, scan_pdf_page, typeset};

const MIN_TRUTH_CHARS: usize = 50;

fn block_text(blocks: &[Block]) -> String {
    fn walk(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m {
                    match (k.as_str(), x) {
                        ("text", serde_json::Value::String(s)) => {
                            out.push_str(s);
                            out.push('\n');
                        }
                        _ => walk(x, out),
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = String::new();
    if let Ok(v) = serde_json::to_value(blocks) {
        walk(&v, &mut out);
    }
    out
}

fn ocr_from_env(models: &std::path::Path) -> Result<Ocr, parsers::OcrError> {
    Ok(Ocr::load(models, env_num("OCR_THREADS", 8))?
        .with_det_max_side(env_num("OCR_DET_SIDE", DET_MAX_SIDE))
        .with_rec_width_scale(env_num("OCR_REC_WIDTH", REC_WIDTH_SCALE))
        .with_options(OcrOptions {
            split_superscripts: env_num("OCR_SPLIT_SUP", 1u8) == 1,
            fix_amounts: env_num("OCR_FIX_AMOUNTS", 1u8) == 1,
        }))
}

#[test]
#[ignore]
fn make_samples_from_pdfs() -> TestResult {
    let (Some(lib), Some(list), Some(out)) = (env_path("PDFIUM_DIR"), env_path("OCR_SAMPLE_LIST"), env_path("OCR_OUT")) else {
        return Ok(());
    };
    std::fs::create_dir_all(&out)?;
    let pdfium = bind(&lib)?;
    let per_file: usize = env_num("OCR_PAGES", 3);
    let degrade = Degrade::from_env("OCR_DEGRADE");
    let dpi: f32 = env_num("OCR_SCAN_DPI", SCAN_DPI);
    let mut made = 0;
    for (fi, path) in std::fs::read_to_string(&list)?.lines().map(str::trim).filter(|l| !l.is_empty()).enumerate() {
        let doc = pdfium.load_pdf_from_file(path, None)?;
        for index in (1..doc.pages().len()).take(per_file) {
            let scan = scan_pdf_page(&doc.pages().get(index)?, dpi, &degrade, u64::from(index))?;
            if scan.truth.chars().filter(|c| !c.is_whitespace()).count() < MIN_TRUTH_CHARS {
                continue;
            }
            scan.save(&pdfium, &out, &format!("s{fi:02}_p{:03}", index + 1))?;
            made += 1;
        }
    }
    println!("made {made}");
    Ok(())
}

#[test]
#[ignore]
fn make_samples_from_text() -> TestResult {
    let (Some(lib), Some(text), Some(out)) = (env_path("PDFIUM_DIR"), env_path("OCR_TEXT_FILE"), env_path("OCR_OUT")) else {
        return Ok(());
    };
    let fonts = std::env::var("OCR_FONTS")?;
    let fonts: Vec<&str> = fonts.split(';').filter(|f| !f.is_empty()).collect();
    std::fs::create_dir_all(&out)?;
    let pdfium = bind(&lib)?;
    let degrade = Degrade::from_env("OCR_DEGRADE");
    let dpi: f32 = env_num("OCR_SCAN_DPI", SCAN_DPI);
    let lines: Vec<String> = std::fs::read_to_string(&text)?.lines().map(str::to_string).collect();
    for (i, chunk) in lines.chunks(lines_per_page()).enumerate() {
        let scan = typeset(&pdfium, chunk, fonts[i % fonts.len()], dpi, &degrade, i as u64)?;
        scan.save(&pdfium, &out, &format!("t{i:02}"))?;
    }
    println!("pages {}", lines.len().div_ceil(lines_per_page()));
    Ok(())
}

#[test]
#[ignore]
fn measure_ocr() -> TestResult {
    let (Some(lib), Some(models), Some(out)) = (env_path("PDFIUM_DIR"), env_path("OCR_DIR"), env_path("OCR_OUT")) else {
        return Ok(());
    };
    let reader = PdfReader::load(&lib)?.with_ocr(ocr_from_env(&models)?);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&out)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    files.sort();
    let mut total = Score::default();
    for path in &files {
        let truth = std::fs::read_to_string(path.with_extension("txt"))?;
        let bytes = std::fs::read(path)?;
        let started = Instant::now();
        let got = block_text(&reader.read(&bytes, file_id(&bytes))?.blocks);
        let score = compare(&truth, &got, started.elapsed().as_secs_f64());
        std::fs::write(path.with_extension("ocr.txt"), &got)?;
        println!("{}\t{score}", path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default());
        total += score;
    }
    println!("TOTAL\t{total}");
    Ok(())
}

#[test]
#[ignore]
fn dump_page_top_as_png() -> TestResult {
    let (Some(lib), Some(pdf), Some(png)) = (env_path("PDFIUM_DIR"), env_path("OCR_DUMP_PDF"), env_path("OCR_DUMP_PNG")) else {
        return Ok(());
    };
    let frac: f32 = env_num("OCR_DUMP_FRAC", 0.25);
    let pdfium = bind(&lib)?;
    let doc = pdfium.load_pdf_from_file(&pdf, None)?;
    let page = doc.pages().get(0)?;
    let scan = scan_pdf_page(&page, SCAN_DPI, &Degrade::default(), 0)?;
    let (w, h) = scan.image.dimensions();
    image::imageops::crop_imm(&scan.image, 0, 0, w, (h as f32 * frac) as u32).to_image().save(&png)?;
    Ok(())
}

#[test]
#[ignore]
fn trace_ocr_page() -> TestResult {
    let (Some(lib), Some(models), Some(pdf), Some(png)) =
        (env_path("PDFIUM_DIR"), env_path("OCR_DIR"), env_path("OCR_TRACE_PDF"), env_path("OCR_TRACE_PNG"))
    else {
        return Ok(());
    };
    let filter = std::env::var("OCR_TRACE_FILTER").unwrap_or_default();
    let ocr = ocr_from_env(&models)?;
    let pdfium = bind(&lib)?;
    let doc = pdfium.load_pdf_from_file(&pdf, None)?;
    let page = doc.pages().get(0)?;
    let lines = ocr.trace_page(&page)?;
    for line in lines.iter().filter(|l| filter.is_empty() || l.parts.iter().any(|p| p.text.contains(&filter))) {
        println!("{}", trace::describe(line));
    }
    let width = (page.width().value * OCR_RENDER_DPI / POINTS_PER_INCH).round() as i32;
    let mut img = page.render_with_config(&PdfRenderConfig::new().set_target_width(width))?.as_image().to_rgb8();
    trace::draw(&mut img, &lines);
    match trace::crop_from_env("OCR_TRACE_CROP") {
        Some([x0, y0, x1, y1]) => image::imageops::crop_imm(&img, x0, y0, x1 - x0, y1 - y0).to_image().save(&png)?,
        None => img.save(&png)?,
    }
    Ok(())
}
