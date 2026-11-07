use docmodel::{Anchor, Block, ExtractWarning, FactSource, Rect};

use super::words::{CharBox, lines, paragraphs, words};
use crate::pdf_table::{Found, Glyph, Segment, TableSettings, find_tables};
use crate::pdf_table::overlaps;

pub(crate) struct PageInput {
    pub number: u32,
    pub chars: Vec<CharBox>,
    pub segments: Vec<Segment>,
}

pub(crate) struct PageOutput {
    pub blocks: Vec<(f32, Block)>,
    pub warnings: Vec<ExtractWarning>,
    pub has_text: bool,
}

fn anchor(source: &FactSource, page: u32, rect: Rect) -> Anchor {
    Anchor { source: source.clone(), page: Some(page), bbox: Some(rect), path: None, span: None }
}

pub(crate) fn lay_out(page: PageInput, source: &FactSource, settings: &TableSettings) -> PageOutput {
    let glyphs = words(&page.chars);
    let tables: Vec<Found> = find_tables(&page.segments, &glyphs, source, page.number, settings);
    let mut blocks: Vec<(f32, Block)> = Vec::new();
    let mut warnings = Vec::new();
    let in_table = |g: &Glyph| tables.iter().any(|t| overlaps(&g.rect, &t.bbox));
    for found in &tables {
        match &found.result {
            Ok(table) => blocks.push((found.bbox.y1, Block::Table(table.clone()))),
            Err(_) => {
                let members: Vec<&Glyph> = glyphs.iter().filter(|g| overlaps(&g.rect, &found.bbox)).collect();
                if members.is_empty() {
                    continue;
                }
                let at = anchor(source, page.number, found.bbox);
                warnings.push(ExtractWarning::TableDegraded { anchor: at.clone() });
                let text = lines(&members).into_iter().map(|(t, _)| t).collect::<Vec<_>>().join("\n");
                blocks.push((found.bbox.y1, Block::Para { text, anchor: at }));
            }
        }
    }
    let free: Vec<&Glyph> = glyphs.iter().filter(|g| !in_table(g)).collect();
    for (text, rect) in paragraphs(lines(&free)) {
        blocks.push((rect.y1, Block::Para { text, anchor: anchor(source, page.number, rect) }));
    }
    blocks.sort_by(|a, b| b.0.total_cmp(&a.0));
    PageOutput { blocks, warnings, has_text: !glyphs.is_empty() }
}
