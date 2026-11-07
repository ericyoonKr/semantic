use docmodel::Rect;

use super::Glyph;
use super::merge::Span;

pub(crate) struct Placed {
    pub inside: usize,
    pub unassigned: usize,
    pub per_cell: Vec<Vec<usize>>,
}

fn center(r: &Rect) -> (f32, f32) {
    ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

pub(crate) fn overlaps(r: &Rect, bbox: &Rect) -> bool {
    r.x0.min(r.x1) < bbox.x1 && r.x0.max(r.x1) > bbox.x0 && r.y0.min(r.y1) < bbox.y1 && r.y0.max(r.y1) > bbox.y0
}

pub(crate) fn assign(glyphs: &[Glyph], spans: &[Span], xs: &[f32], ys_top_down: &[f32], bbox: &Rect, eps: f32) -> Placed {
    let mut placed = Placed { inside: 0, unassigned: 0, per_cell: vec![Vec::new(); spans.len()] };
    for (gi, g) in glyphs.iter().enumerate() {
        if !overlaps(&g.rect, bbox) {
            continue;
        }
        placed.inside += 1;
        let (cx, cy) = center(&g.rect);
        let hit = spans.iter().position(|s| {
            let (x0, x1) = (xs[s.col], xs[s.col + s.colspan]);
            let (top, bottom) = (ys_top_down[s.row], ys_top_down[s.row + s.rowspan]);
            cx > x0 + eps && cx < x1 - eps && cy > bottom + eps && cy < top - eps
        });
        match hit {
            Some(i) => placed.per_cell[i].push(gi),
            None => placed.unassigned += 1,
        }
    }
    placed
}

pub(crate) fn cell_text(glyphs: &[Glyph], members: &[usize]) -> String {
    let mut items: Vec<&Glyph> = members.iter().filter_map(|i| glyphs.get(*i)).collect();
    items.sort_by(|a, b| center(&b.rect).1.total_cmp(&center(&a.rect).1));
    let mut lines: Vec<Vec<&Glyph>> = Vec::new();
    for g in items {
        let (_, cy) = center(&g.rect);
        let half = (g.rect.y1 - g.rect.y0).abs() / 2.0;
        match lines.last_mut() {
            Some(line) if line.first().is_some_and(|f| (center(&f.rect).1 - cy).abs() <= half.max(1.0)) => line.push(g),
            _ => lines.push(vec![g]),
        }
    }
    lines
        .into_iter()
        .map(|mut line| {
            line.sort_by(|a, b| a.rect.x0.total_cmp(&b.rect.x0));
            line.iter().map(|g| g.text.as_str()).collect::<Vec<_>>().join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
