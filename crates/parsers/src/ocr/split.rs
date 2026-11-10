use super::boxes::PixelBox;
use super::components::{Component, components};
use super::raster::{Crop, Raster};

const INK_LEVEL: u8 = 128;
const MIN_INK_PIXELS: u32 = 4;
const BODY_GLYPH: f32 = 0.75;
const TALL_QUANTILE: f32 = 0.8;
const TOP_QUANTILE: f32 = 0.2;
const BASE_QUANTILE: f32 = 0.8;
const MIN_BODY_GLYPHS: usize = 3;
const BAND_ABOVE: f32 = 0.5;
const BAND_BELOW: f32 = 0.25;
const SUP_BOTTOM: f32 = 0.6;
const SUP_HEIGHT: f32 = 0.65;
const MAX_GAP: f32 = 0.5;
const MIN_RUN_WIDTH: f32 = 1.5;
const STACK_OVERLAP: f32 = 0.3;
const NEST_OVERLAP: f32 = 0.7;
const STACK_GAP: f32 = 0.2;
const MIN_SEGMENT: u32 = 3;
const SUPERSCRIPT_STRETCH: f32 = 1.5;
const SUPERSCRIPT_PAD: f32 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Body,
    Superscript,
}

impl Kind {
    pub fn stretch(self) -> f32 {
        match self {
            Kind::Body => 1.0,
            Kind::Superscript => SUPERSCRIPT_STRETCH,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Segment {
    pub crop: Crop,
    pub kind: Kind,
}

impl Segment {
    pub fn whole(line: &PixelBox) -> Self {
        Self { crop: Crop::plain(*line), kind: Kind::Body }
    }

    fn superscript(area: PixelBox) -> Self {
        let pad = (area.height() as f32 * SUPERSCRIPT_PAD) as u32;
        Self { crop: Crop { area, pad }, kind: Kind::Superscript }
    }
}

#[derive(Debug, Clone, Copy)]
struct Line {
    at: f32,
    slope: f32,
}

impl Line {
    fn fit(points: &[(f32, f32)]) -> Option<Self> {
        let n = points.len() as f32;
        let mx = points.iter().map(|p| p.0).sum::<f32>() / n;
        let my = points.iter().map(|p| p.1).sum::<f32>() / n;
        let sxx: f32 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
        let sxy: f32 = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
        let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
        my.is_finite().then_some(Self { at: my - slope * mx, slope })
    }

    fn y(&self, x: f32) -> f32 {
        self.at + self.slope * x
    }

    fn shifted_to(self, points: &[(f32, f32)], q: f32) -> Self {
        let mut residuals: Vec<f32> = points.iter().map(|p| p.1 - self.y(p.0)).collect();
        residuals.sort_by(f32::total_cmp);
        let index = ((residuals.len().saturating_sub(1)) as f32 * q).round() as usize;
        let shift = residuals.get(index).copied().unwrap_or(0.0);
        Self { at: self.at + shift, slope: self.slope }
    }
}

struct Guides {
    top: Line,
    base: Line,
}

impl Guides {
    fn from_glyphs(glyphs: &[Component]) -> Option<Self> {
        let mut heights: Vec<u32> = glyphs.iter().map(Component::height).collect();
        heights.sort_unstable();
        let tall_index = ((heights.len().saturating_sub(1)) as f32 * TALL_QUANTILE).round() as usize;
        let tall = *heights.get(tall_index)? as f32;
        let body: Vec<&Component> = glyphs.iter().filter(|c| c.height() as f32 >= BODY_GLYPH * tall).collect();
        if body.len() < MIN_BODY_GLYPHS {
            return None;
        }
        let centre = |c: &Component| (c.x0 + c.x1) as f32 / 2.0;
        let tops: Vec<(f32, f32)> = body.iter().map(|c| (centre(c), c.y0 as f32)).collect();
        let bases: Vec<(f32, f32)> = body.iter().map(|c| (centre(c), c.y1 as f32)).collect();
        Some(Self {
            top: Line::fit(&tops)?.shifted_to(&tops, TOP_QUANTILE),
            base: Line::fit(&bases)?.shifted_to(&bases, BASE_QUANTILE),
        })
    }

    fn body_height(&self, x: f32) -> f32 {
        (self.base.y(x) - self.top.y(x)).max(1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Empty,
    Body,
    Raised { top: u32, bottom: u32 },
}

fn ink(raster: &Raster, line: &PixelBox) -> Vec<Component> {
    let w = line.width();
    components(w, line.height(), |i| {
        let (x, y) = (line.x0 + i as u32 % w, line.y0 + i as u32 / w);
        (raster.luma(x, y) < INK_LEVEL).then_some(1.0)
    })
    .into_iter()
    .filter(|c| c.pixels >= MIN_INK_PIXELS)
    .filter(|c| c.y0 > 0 && c.y1 < line.height())
    .collect()
}

fn same_glyph(a: &Component, b: &Component) -> bool {
    let narrow = a.width().min(b.width()) as f32;
    let shared_x = a.x1.min(b.x1).saturating_sub(a.x0.max(b.x0)) as f32;
    let shared_y = a.y1.min(b.y1).saturating_sub(a.y0.max(b.y0)) as f32;
    let stacked = shared_y <= STACK_GAP * a.height().min(b.height()) as f32;
    shared_x > 0.0 && (shared_x >= NEST_OVERLAP * narrow || (stacked && shared_x >= STACK_OVERLAP * narrow))
}

fn glyph_columns(mut pieces: Vec<Component>) -> Vec<Component> {
    pieces.sort_by_key(|c| c.x0);
    let mut out: Vec<Component> = Vec::with_capacity(pieces.len());
    for p in pieces {
        match out.last_mut() {
            Some(g) if same_glyph(g, &p) => {
                g.x0 = g.x0.min(p.x0);
                g.y0 = g.y0.min(p.y0);
                g.x1 = g.x1.max(p.x1);
                g.y1 = g.y1.max(p.y1);
                g.pixels += p.pixels;
                g.weight += p.weight;
            }
            _ => out.push(p),
        }
    }
    out
}

fn column(raster: &Raster, line: &PixelBox, guides: &Guides, x: u32) -> Column {
    let fx = x as f32;
    let (top, body) = (guides.top.y(fx), guides.body_height(fx));
    let from = (top - BAND_ABOVE * body).max(0.0) as u32;
    let to = ((guides.base.y(fx) + BAND_BELOW * body).max(0.0) as u32).min(line.height());
    let inked: Vec<u32> = (from..to).filter(|&y| raster.luma(line.x0 + x, line.y0 + y) < INK_LEVEL).collect();
    let (Some(&first), Some(&last)) = (inked.first(), inked.last()) else {
        return Column::Empty;
    };
    let bottom = last + 1;
    let raised = bottom as f32 <= top + SUP_BOTTOM * body && (bottom - first) as f32 <= SUP_HEIGHT * body;
    if raised { Column::Raised { top: first, bottom } } else { Column::Body }
}

struct Run {
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
}

impl Run {
    fn area(&self) -> PixelBox {
        PixelBox { x0: self.x0, y0: self.y0, x1: self.x1, y1: self.y1 }
    }
}

fn superscript_runs(raster: &Raster, line: &PixelBox) -> Vec<PixelBox> {
    let Some(guides) = Guides::from_glyphs(&glyph_columns(ink(raster, line))) else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    let mut current: Option<Run> = None;
    let mut gap = 0u32;
    let close = |run: Option<Run>, runs: &mut Vec<PixelBox>| {
        if let Some(r) = run {
            let mid = (r.x0 + r.x1) as f32 / 2.0;
            if (r.x1 - r.x0) as f32 >= MIN_RUN_WIDTH * guides.body_height(mid) {
                runs.push(r.area());
            }
        }
    };
    for x in 0..line.width() {
        match column(raster, line, &guides, x) {
            Column::Raised { top, bottom } => {
                gap = 0;
                match current.as_mut() {
                    Some(r) => {
                        r.x1 = x + 1;
                        r.y0 = r.y0.min(top);
                        r.y1 = r.y1.max(bottom);
                    }
                    None => current = Some(Run { x0: x, x1: x + 1, y0: top, y1: bottom }),
                }
            }
            Column::Empty => {
                gap += 1;
                if current.is_some() && gap as f32 > MAX_GAP * guides.body_height(x as f32) {
                    close(current.take(), &mut runs);
                }
            }
            Column::Body => {
                gap = 0;
                close(current.take(), &mut runs);
            }
        }
    }
    close(current.take(), &mut runs);
    runs
}

fn body(line: &PixelBox, x0: u32, x1: u32) -> Option<Segment> {
    (x1 >= x0 + MIN_SEGMENT).then(|| Segment::whole(&PixelBox { x0, y0: line.y0, x1, y1: line.y1 }))
}

fn to_page(line: &PixelBox, b: &PixelBox) -> PixelBox {
    PixelBox { x0: line.x0 + b.x0, y0: line.y0 + b.y0, x1: line.x0 + b.x1, y1: line.y0 + b.y1 }
}

pub(crate) fn segments(raster: &Raster, line: &PixelBox) -> Vec<Segment> {
    let runs = superscript_runs(raster, line);
    if runs.is_empty() {
        return vec![Segment::whole(line)];
    }
    let mut out = Vec::new();
    let mut cursor = line.x0;
    for run in runs {
        let sup = Segment::superscript(to_page(line, &run));
        out.extend(body(line, cursor, sup.crop.area.x0));
        cursor = sup.crop.area.x1;
        out.push(sup);
    }
    out.extend(body(line, cursor, line.x1));
    out
}

#[cfg(test)]
mod tests {
    use super::{Kind, segments};
    use crate::ocr::boxes::PixelBox;
    use crate::ocr::raster::Raster;

    fn page(width: u32, height: u32, marks: &[(u32, u32, u32, u32)]) -> Raster {
        let mut rgb = vec![255u8; (width * height * 3) as usize];
        for &(x0, y0, x1, y1) in marks {
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * width + x) * 3) as usize;
                    rgb[i..i + 3].copy_from_slice(&[0, 0, 0]);
                }
            }
        }
        Raster::from_rgb(width, height, rgb)
    }

    fn whole(width: u32, height: u32) -> PixelBox {
        PixelBox { x0: 0, y0: 0, x1: width, y1: height }
    }

    fn kinds(raster: &Raster, width: u32, height: u32) -> Vec<Kind> {
        segments(raster, &whole(width, height)).iter().map(|s| s.kind).collect()
    }

    fn body_glyphs(n: u32) -> Vec<(u32, u32, u32, u32)> {
        (0..n).map(|i| (2 + i * 12, 10, 12 + i * 12, 30)).collect()
    }

    #[test]
    fn a_wide_raised_run_becomes_a_superscript_segment_between_body_parts() {
        let mut marks = body_glyphs(6);
        marks.extend((0..6).map(|i| (80 + i * 6, 10, 84 + i * 6, 16)));
        marks.extend((0..2).map(|i| (122 + i * 12, 10, 132 + i * 12, 30)));
        let raster = page(150, 40, &marks);
        let segs = segments(&raster, &whole(150, 40));
        assert_eq!(segs.iter().map(|s| s.kind).collect::<Vec<_>>(), vec![Kind::Body, Kind::Superscript, Kind::Body]);
        let sup = &segs[1].crop;
        assert!(sup.area.x0 <= 80 && sup.area.x1 >= 114 && sup.area.y1 <= 16 && sup.pad > 0, "{sup:?}");
    }

    #[test]
    fn hangul_syllables_with_a_small_top_piece_are_not_superscripts() {
        let marks: Vec<(u32, u32, u32, u32)> =
            (0..8).flat_map(|i| [(2 + i * 12, 10, 10 + i * 12, 16), (2 + i * 12, 20, 12 + i * 12, 30)]).collect();
        assert_eq!(kinds(&page(110, 40, &marks), 110, 40), vec![Kind::Body]);
    }

    #[test]
    fn pieces_of_neighbouring_lines_cut_by_the_box_edges_are_ignored() {
        let mut marks: Vec<(u32, u32, u32, u32)> = (0..8).map(|i| (2 + i * 12, 20, 12 + i * 12, 40)).collect();
        marks.extend((0..8).map(|i| (2 + i * 12, 0, 12 + i * 12, 6)));
        assert_eq!(kinds(&page(110, 50, &marks), 110, 50), vec![Kind::Body]);
    }

    #[test]
    fn a_narrow_raised_run_one_glyph_wide_is_not_a_superscript() {
        let mut marks = body_glyphs(6);
        marks.extend((0..3).map(|i| (80 + i * 4, 10, 83 + i * 4, 16)));
        assert_eq!(kinds(&page(100, 40, &marks), 100, 40), vec![Kind::Body]);
    }

    #[test]
    fn a_superscript_joined_to_the_previous_glyph_by_ink_still_starts_after_its_last_full_column() {
        let mut marks = body_glyphs(6);
        marks.push((70, 12, 74, 14));
        marks.extend((0..6).map(|i| (73 + i * 12, 10, 83 + i * 12, 16)));
        let raster = page(160, 40, &marks);
        let segs = segments(&raster, &whole(160, 40));
        let sup = segs.iter().find(|s| s.kind == Kind::Superscript).map(|s| s.crop.area.x0);
        assert!(sup.is_some_and(|x| x <= 73), "{segs:?}");
    }

    #[test]
    fn single_raised_mark_like_a_quote_stays_in_the_body() {
        let mut marks = body_glyphs(6);
        marks.push((80, 10, 84, 16));
        assert_eq!(kinds(&page(100, 40, &marks), 100, 40), vec![Kind::Body]);
    }

    #[test]
    fn small_glyphs_at_the_high_end_of_a_skewed_line_are_not_superscripts() {
        let marks: Vec<(u32, u32, u32, u32)> = (0..12)
            .map(|i| {
                let small = if (8..11).contains(&i) { 8 } else { 0 };
                (2 + i * 12, 20 - i + small, 12 + i * 12, 40 - i)
            })
            .collect();
        assert_eq!(kinds(&page(150, 48, &marks), 150, 48), vec![Kind::Body]);
    }
}
