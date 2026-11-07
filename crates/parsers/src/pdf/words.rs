use docmodel::Rect;

use crate::pdf_table::Glyph;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CharBox {
    pub ch: char,
    pub rect: Rect,
}

fn height(r: &Rect) -> f32 {
    (r.y1 - r.y0).abs()
}

fn same_line(a: &Rect, b: &Rect) -> bool {
    let mid_a = (a.y0 + a.y1) / 2.0;
    let mid_b = (b.y0 + b.y1) / 2.0;
    (mid_a - mid_b).abs() <= height(a).max(height(b)) / 2.0
}

fn grow(word: &mut Glyph, c: &CharBox) {
    word.text.push(c.ch);
    word.rect.x0 = word.rect.x0.min(c.rect.x0);
    word.rect.y0 = word.rect.y0.min(c.rect.y0);
    word.rect.x1 = word.rect.x1.max(c.rect.x1);
    word.rect.y1 = word.rect.y1.max(c.rect.y1);
}

pub(crate) fn words(chars: &[CharBox]) -> Vec<Glyph> {
    let mut out: Vec<Glyph> = Vec::new();
    let mut current: Option<Glyph> = None;
    for c in chars {
        if c.ch.is_whitespace() || c.ch.is_control() {
            out.extend(current.take());
            continue;
        }
        let joins = current.as_ref().is_some_and(|w| {
            let gap = c.rect.x0 - w.rect.x1;
            same_line(&w.rect, &c.rect) && gap <= height(&c.rect).max(1.0) * 0.3 && gap >= -height(&c.rect)
        });
        match current.as_mut() {
            Some(w) if joins => grow(w, c),
            _ => {
                out.extend(current.take());
                current = Some(Glyph { text: c.ch.to_string(), rect: c.rect });
            }
        }
    }
    out.extend(current);
    out
}

pub(crate) fn lines(words: &[&Glyph]) -> Vec<(String, Rect)> {
    let mut sorted: Vec<&Glyph> = words.to_vec();
    sorted.sort_by(|a, b| ((b.rect.y0 + b.rect.y1) / 2.0).total_cmp(&((a.rect.y0 + a.rect.y1) / 2.0)));
    let mut rows: Vec<Vec<&Glyph>> = Vec::new();
    for w in sorted {
        match rows.last_mut() {
            Some(row) if row.first().is_some_and(|f| same_line(&f.rect, &w.rect)) => row.push(w),
            _ => rows.push(vec![w]),
        }
    }
    rows.into_iter()
        .filter_map(|mut row| {
            row.sort_by(|a, b| a.rect.x0.total_cmp(&b.rect.x0));
            let first = row.first()?.rect;
            let rect = row.iter().fold(first, |acc, w| Rect {
                x0: acc.x0.min(w.rect.x0),
                y0: acc.y0.min(w.rect.y0),
                x1: acc.x1.max(w.rect.x1),
                y1: acc.y1.max(w.rect.y1),
            });
            Some((row.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "), rect))
        })
        .collect()
}

pub(crate) fn paragraphs(lines: Vec<(String, Rect)>) -> Vec<(String, Rect)> {
    let mut out: Vec<(String, Rect)> = Vec::new();
    for (text, rect) in lines {
        let joins = out.last().is_some_and(|(_, prev)| {
            let gap = prev.y0 - rect.y1;
            gap >= -1.0 && gap <= height(&rect).max(1.0) * 0.8
        });
        match out.last_mut() {
            Some((t, prev)) if joins => {
                t.push('\n');
                t.push_str(&text);
                *prev = Rect { x0: prev.x0.min(rect.x0), y0: prev.y0.min(rect.y0), x1: prev.x1.max(rect.x1), y1: prev.y1.max(rect.y1) };
            }
            _ => out.push((text, rect)),
        }
    }
    out
}
