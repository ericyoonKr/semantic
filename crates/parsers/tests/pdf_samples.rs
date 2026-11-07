use std::path::PathBuf;
use std::time::Instant;

use docmodel::{Block, ExtractWarning};
use parsers::{ParseError, PdfReader, file_id};

#[test]
#[ignore]
fn local_pdf_samples_report_counts_only() -> Result<(), Box<dyn std::error::Error>> {
    let (Some(lib), Some(dir)) = (std::env::var_os("PDFIUM_DIR"), std::env::var_os("PDF_SAMPLE_DIR")) else {
        return Ok(());
    };
    let reader = PdfReader::load(&PathBuf::from(lib))?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(PathBuf::from(dir))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
        .collect();
    files.sort();
    let (mut ok, mut empty, mut encrypted, mut broken) = (0, 0, 0, 0);
    let (mut tables, mut degraded, mut paras, mut partly) = (0, 0, 0, 0);
    let started = Instant::now();
    for (i, path) in files.iter().enumerate() {
        let bytes = std::fs::read(path)?;
        let t = Instant::now();
        match reader.read(&bytes, file_id(&bytes)) {
            Ok(doc) => {
                ok += 1;
                let t_ok = doc.blocks.iter().filter(|b| matches!(b, Block::Table(_))).count();
                let t_bad = doc.warnings.iter().filter(|w| matches!(w, ExtractWarning::TableDegraded { .. })).count();
                let p = doc.blocks.iter().filter(|b| matches!(b, Block::Para { .. })).count();
                let missing = doc.warnings.iter().any(|w| matches!(w, ExtractWarning::PagesUnextracted { .. }));
                tables += t_ok;
                degraded += t_bad;
                paras += p;
                partly += usize::from(missing);
                println!("file {i}: ok tables {t_ok} degraded {t_bad} paras {p} pages_missing {missing} ms {}", t.elapsed().as_millis());
            }
            Err(ParseError::EmptyText) => {
                empty += 1;
                println!("file {i}: no text");
            }
            Err(ParseError::Encrypted) => {
                encrypted += 1;
                println!("file {i}: encrypted");
            }
            Err(e) => {
                broken += 1;
                println!("file {i}: error {e}");
            }
        }
    }
    println!(
        "pdf samples: files {} ok {ok} no_text {empty} encrypted {encrypted} errors {broken} tables {tables} degraded_tables {degraded} paras {paras} files_with_missing_pages {partly} total_ms {}",
        files.len(),
        started.elapsed().as_millis()
    );
    Ok(())
}

#[test]
#[ignore]
fn local_pdf_tables_for_review() -> Result<(), Box<dyn std::error::Error>> {
    use pdfium_render::prelude::*;
    let (Some(lib), Some(dir), Some(out)) =
        (std::env::var_os("PDFIUM_DIR"), std::env::var_os("PDF_SAMPLE_DIR"), std::env::var_os("PDF_REVIEW_DIR"))
    else {
        return Ok(());
    };
    let picks: usize = std::env::var("PDF_REVIEW_COUNT").ok().and_then(|v| v.parse().ok()).unwrap_or(12);
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out)?;
    let reader = PdfReader::load(&PathBuf::from(&lib))?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(PathBuf::from(dir))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")))
        .collect();
    files.sort();
    let mut all = Vec::new();
    for (fi, path) in files.iter().enumerate() {
        let bytes = std::fs::read(path)?;
        if let Ok(doc) = reader.read(&bytes, file_id(&bytes)) {
            for b in doc.blocks {
                if let Block::Table(t) = b {
                    all.push((fi, t));
                }
            }
        }
    }
    if std::env::var_os("PDF_REVIEW_LIST").is_some() {
        for (fi, t) in &all {
            let rows = t.cells.iter().map(|c| c.row + c.rowspan).max().unwrap_or(0);
            let cols = t.cells.iter().map(|c| c.col + c.colspan).max().unwrap_or(0);
            println!("table at file {fi} page {} rows {rows} cols {cols}", t.anchor.page.unwrap_or(0));
        }
    }
    drop(reader);
    let pdfium = Pdfium::new(Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&PathBuf::from(&lib)))?);
    let step = (all.len() / picks.max(1)).max(1);
    for (n, (fi, table)) in all.iter().step_by(step).take(picks).enumerate() {
        let (Some(page_no), Some(bbox)) = (table.anchor.page, table.anchor.bbox) else { continue };
        let bytes = std::fs::read(&files[*fi])?;
        let doc = pdfium.load_pdf_from_byte_slice(&bytes, None)?;
        let page = doc.pages().get((page_no - 1) as u16)?;
        let width_pt = page.width().value;
        let height_pt = page.height().value;
        let px = 1600.0;
        let scale = px / width_pt;
        let image = page.render_with_config(&PdfRenderConfig::new().set_target_width(px as i32))?.as_image();
        let x = ((bbox.x0 - 6.0) * scale).max(0.0) as u32;
        let y = ((height_pt - bbox.y1 - 6.0) * scale).max(0.0) as u32;
        let w = (((bbox.x1 - bbox.x0) + 12.0) * scale) as u32;
        let h = (((bbox.y1 - bbox.y0) + 12.0) * scale) as u32;
        image.crop_imm(x, y, w.min(image.width() - x), h.min(image.height() - y)).save(out.join(format!("table_{n:02}.png")))?;
        let mut cells = String::new();
        for c in &table.cells {
            cells.push_str(&format!("r{} c{} rs{} cs{} | {}\n", c.row, c.col, c.rowspan, c.colspan, c.text.replace('\n', " / ")));
        }
        std::fs::write(out.join(format!("table_{n:02}.txt")), format!("file {fi} page {page_no}\n{}\n{cells}", table.to_markdown()))?;
    }
    println!("review tables written: {} of {}", picks.min(all.len()), all.len());
    Ok(())
}
