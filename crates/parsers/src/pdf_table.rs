mod assign;
mod lines;
mod merge;

use docmodel::{Anchor, Cell, FactSource, Rect, Table};

pub(crate) use assign::overlaps;
use assign::{assign, cell_text};
use lines::{classify, groups, snap};
use merge::merged_cells;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Segment {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Glyph {
    pub text: String,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TableError {
    NoGrid,
    TooManyUnassigned { ratio: f32 },
    Ambiguous(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TableSettings {
    pub snap_eps: f32,
    pub slope_tol: f32,
    pub min_len: f32,
    pub max_unassigned: f32,
}

impl Default for TableSettings {
    fn default() -> Self {
        Self { snap_eps: 2.0, slope_tol: 1.0, min_len: 4.0, max_unassigned: 0.0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Found {
    pub bbox: Rect,
    pub result: Result<Table, TableError>,
}

pub(crate) fn find_tables(
    segments: &[Segment],
    glyphs: &[Glyph],
    source: &FactSource,
    page: u32,
    settings: &TableSettings,
) -> Vec<Found> {
    let eps = settings.snap_eps;
    let mut out = Vec::new();
    for group in groups(&classify(segments, settings), eps) {
        let xs = snap(group.v.iter().map(|v| v.x), eps);
        let mut ys = snap(group.h.iter().map(|h| h.y), eps);
        ys.reverse();
        let (Some(&x0), Some(&x1), Some(&y_top), Some(&y_bottom)) = (xs.first(), xs.last(), ys.first(), ys.last()) else {
            continue;
        };
        if xs.len() < 2 || ys.len() < 2 {
            continue;
        }
        let bbox = Rect { x0, y0: y_bottom, x1, y1: y_top };
        let result = merged_cells(&group, &xs, &ys, eps).and_then(|spans| {
            let placed = assign(glyphs, &spans, &xs, &ys, &bbox, eps);
            if placed.inside > 0 {
                let ratio = placed.unassigned as f32 / placed.inside as f32;
                if ratio > settings.max_unassigned {
                    return Err(TableError::TooManyUnassigned { ratio });
                }
            }
            let anchor = |rect: Rect| Anchor { source: source.clone(), page: Some(page), bbox: Some(rect), path: None, span: None };
            let cells = spans
                .iter()
                .zip(placed.per_cell)
                .map(|(s, members)| Cell {
                    row: s.row as u32,
                    col: s.col as u32,
                    rowspan: s.rowspan as u32,
                    colspan: s.colspan as u32,
                    text: cell_text(glyphs, &members),
                    anchor: anchor(Rect { x0: xs[s.col], y0: ys[s.row + s.rowspan], x1: xs[s.col + s.colspan], y1: ys[s.row] }),
                })
                .collect();
            Ok(Table { cells, anchor: anchor(bbox), degraded: false })
        });
        out.push(Found { bbox, result });
    }
    out.sort_by(|a, b| b.bbox.y1.total_cmp(&a.bbox.y1).then(a.bbox.x0.total_cmp(&b.bbox.x0)));
    out
}

#[cfg(test)]
mod tests;
