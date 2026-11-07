#![allow(clippy::needless_range_loop, clippy::type_complexity)]

use docmodel::{FactSource, FileId, Rect, Table};

use super::{Found, Glyph, Segment, TableError, TableSettings, find_tables};

fn src() -> FactSource {
    FactSource::File(FileId("f".into()))
}

fn h(y: f32, x0: f32, x1: f32) -> Segment {
    Segment { x0, y0: y, x1, y1: y }
}

fn v(x: f32, y0: f32, y1: f32) -> Segment {
    Segment { x0: x, y0, x1: x, y1 }
}

fn word(text: &str, cx: f32, cy: f32) -> Glyph {
    Glyph { text: text.into(), rect: Rect { x0: cx - 4.0, y0: cy - 3.0, x1: cx + 4.0, y1: cy + 3.0 } }
}

fn run(segments: &[Segment], glyphs: &[Glyph]) -> Vec<Found> {
    find_tables(segments, glyphs, &src(), 1, &TableSettings::default())
}

fn table(found: &[Found]) -> Option<&Table> {
    found.first().and_then(|f| f.result.as_ref().ok())
}

fn grid(xs: &[f32], ys: &[f32]) -> Vec<Segment> {
    let (x0, x1) = (xs[0], xs[xs.len() - 1]);
    let (y0, y1) = (ys[ys.len() - 1], ys[0]);
    let mut out: Vec<Segment> = ys.iter().map(|y| h(*y, x0, x1)).collect();
    out.extend(xs.iter().map(|x| v(*x, y0, y1)));
    out
}

#[test]
fn full_grid_assigns_each_word_to_its_cell() {
    let segs = grid(&[0.0, 100.0, 200.0], &[100.0, 50.0, 0.0]);
    let words = [word("가", 50.0, 75.0), word("나", 150.0, 75.0), word("다", 50.0, 25.0), word("라", 150.0, 25.0)];
    let found = run(&segs, &words);
    let t = table(&found);
    let cells: Vec<(u32, u32, &str)> = t.map(|t| t.cells.iter().map(|c| (c.row, c.col, c.text.as_str())).collect()).unwrap_or_default();
    assert_eq!(cells, vec![(0, 0, "가"), (0, 1, "나"), (1, 0, "다"), (1, 1, "라")]);
}

#[test]
fn missing_inner_line_becomes_merged_cell() {
    let mut segs = vec![h(100.0, 0.0, 300.0), h(50.0, 0.0, 300.0), h(0.0, 0.0, 300.0), v(0.0, 0.0, 100.0), v(300.0, 0.0, 100.0)];
    segs.push(v(100.0, 0.0, 50.0));
    segs.push(v(200.0, 0.0, 100.0));
    let words = [word("머리", 50.0, 75.0), word("머리2", 150.0, 75.0), word("값", 250.0, 75.0)];
    let found = run(&segs, &words);
    let t = table(&found);
    let first = t.and_then(|t| t.cells.first()).map(|c| (c.row, c.col, c.rowspan, c.colspan, c.text.clone()));
    assert_eq!(first, Some((0, 0, 1, 2, "머리 머리2".to_string())));
    assert_eq!(t.map(|t| t.cells.len()), Some(5));
}

#[test]
fn vertical_merge_gives_rowspan() {
    let segs = vec![
        h(100.0, 0.0, 200.0),
        h(50.0, 100.0, 200.0),
        h(0.0, 0.0, 200.0),
        v(0.0, 0.0, 100.0),
        v(100.0, 0.0, 100.0),
        v(200.0, 0.0, 100.0),
    ];
    let found = run(&segs, &[]);
    let spans: Vec<(u32, u32, u32, u32)> =
        table(&found).map(|t| t.cells.iter().map(|c| (c.row, c.col, c.rowspan, c.colspan)).collect()).unwrap_or_default();
    assert_eq!(spans, vec![(0, 0, 2, 1), (0, 1, 1, 1), (1, 1, 1, 1)]);
}

#[test]
fn non_rectangular_merge_is_refused() {
    let segs = vec![
        h(150.0, 0.0, 200.0),
        h(100.0, 0.0, 100.0),
        h(50.0, 0.0, 200.0),
        h(0.0, 0.0, 200.0),
        v(0.0, 0.0, 150.0),
        v(100.0, 0.0, 50.0),
        v(100.0, 100.0, 150.0),
        v(200.0, 0.0, 150.0),
    ];
    let found = run(&segs, &[]);
    assert!(matches!(found.first().map(|f| &f.result), Some(Err(TableError::Ambiguous(_)))));
}

#[test]
fn words_sitting_on_lines_fail_the_table() {
    let segs = grid(&[0.0, 100.0, 200.0], &[100.0, 50.0, 0.0]);
    let words = [word("가", 50.0, 75.0), word("걸침", 100.0, 75.0), word("다", 50.0, 25.0), word("걸침2", 150.0, 50.0)];
    let found = run(&segs, &words);
    assert!(matches!(found.first().map(|f| &f.result), Some(Err(TableError::TooManyUnassigned { .. }))));
}

#[test]
fn two_separate_tables_on_a_page_and_a_lone_box() {
    let mut segs = grid(&[0.0, 100.0, 200.0], &[300.0, 250.0, 200.0]);
    segs.extend(grid(&[0.0, 50.0, 100.0, 150.0], &[100.0, 50.0, 0.0]));
    segs.extend(grid(&[300.0, 400.0], &[100.0, 0.0]));
    let found = run(&segs, &[]);
    let ok: Vec<usize> = found.iter().filter_map(|f| f.result.as_ref().ok().map(|t| t.cells.len())).collect();
    assert_eq!(ok, vec![4, 6]);
    assert_eq!(found.iter().filter(|f| f.result == Err(TableError::NoGrid)).count(), 1);
}

#[test]
fn multi_line_cell_reads_top_down_then_left_right() {
    let segs = grid(&[0.0, 200.0, 300.0], &[100.0, 0.0]);
    let words = [word("둘째", 30.0, 30.0), word("첫째", 30.0, 70.0), word("뒤", 80.0, 70.0), word("값", 250.0, 50.0)];
    let found = run(&segs, &words);
    assert_eq!(table(&found).and_then(|t| t.cells.first()).map(|c| c.text.clone()), Some("첫째 뒤\n둘째".to_string()));
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

fn random_case(rng: &mut Lcg) -> (Vec<Segment>, Vec<Glyph>, Vec<(u32, u32, u32, u32, String)>) {
    let rows = 2 + rng.next(5) as usize;
    let cols = 2 + rng.next(5) as usize;
    let mut owner = vec![vec![usize::MAX; cols]; rows];
    let mut spans: Vec<(usize, usize, usize, usize)> = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            if owner[r][c] != usize::MAX {
                continue;
            }
            let mut rs = 1 + if rng.next(4) == 0 { rng.next(2) as usize } else { 0 };
            let mut cs = 1 + if rng.next(4) == 0 { rng.next(2) as usize } else { 0 };
            rs = rs.min(rows - r);
            cs = cs.min(cols - c);
            while (r..r + rs).any(|rr| (c..c + cs).any(|cc| owner[rr][cc] != usize::MAX)) {
                if cs > 1 { cs -= 1 } else { rs -= 1 }
            }
            for rr in r..r + rs {
                for cc in c..c + cs {
                    owner[rr][cc] = spans.len();
                }
            }
            spans.push((r, c, rs, cs));
        }
    }
    let w = 60.0;
    let hgt = 30.0;
    let jitter = |rng: &mut Lcg| (rng.next(11) as f32 - 5.0) / 10.0;
    let top = rows as f32 * hgt;
    let mut segs = Vec::new();
    for r in 0..=rows {
        for c in 0..cols {
            let above = r.checked_sub(1).map(|rr| owner[rr][c]);
            let below = (r < rows).then(|| owner[r][c]);
            if above.is_none() || below.is_none() || above != below {
                let y = top - r as f32 * hgt + jitter(rng);
                segs.push(h(y, c as f32 * w, (c + 1) as f32 * w));
            }
        }
    }
    for c in 0..=cols {
        for r in 0..rows {
            let left = c.checked_sub(1).map(|cc| owner[r][cc]);
            let right = (c < cols).then(|| owner[r][c]);
            if left.is_none() || right.is_none() || left != right {
                let x = c as f32 * w + jitter(rng);
                segs.push(v(x, top - (r + 1) as f32 * hgt, top - r as f32 * hgt));
            }
        }
    }
    let rank = |edges: Vec<usize>, at: usize| edges.iter().filter(|e| **e < at).count() as u32;
    let mut row_edges: Vec<usize> = spans.iter().flat_map(|(r, _, rs, _)| [*r, r + rs]).collect();
    row_edges.sort_unstable();
    row_edges.dedup();
    let mut col_edges: Vec<usize> = spans.iter().flat_map(|(_, c, _, cs)| [*c, c + cs]).collect();
    col_edges.sort_unstable();
    col_edges.dedup();
    let mut glyphs = Vec::new();
    let mut expected = Vec::new();
    for (i, (r, c, rs, cs)) in spans.iter().enumerate() {
        let text = format!("칸{i}");
        let cx = (*c as f32 + *cs as f32 / 2.0) * w;
        let cy = top - (*r as f32 + *rs as f32 / 2.0) * hgt;
        glyphs.push(word(&text, cx, cy));
        let (r0, r1) = (rank(row_edges.clone(), *r), rank(row_edges.clone(), r + rs));
        let (c0, c1) = (rank(col_edges.clone(), *c), rank(col_edges.clone(), c + cs));
        expected.push((r0, c0, r1 - r0, c1 - c0, text));
    }
    (segs, glyphs, expected)
}

#[test]
fn synthetic_tables_are_rebuilt_exactly() {
    let mut rng = Lcg(7);
    let total = 200;
    let mut exact = 0;
    let mut wrong_but_returned = 0;
    let mut refused = 0;
    for _ in 0..total {
        let (segs, glyphs, expected) = random_case(&mut rng);
        let found = run(&segs, &glyphs);
        match found.as_slice() {
            [Found { result: Ok(t), .. }] => {
                let got: Vec<(u32, u32, u32, u32, String)> =
                    t.cells.iter().map(|c| (c.row, c.col, c.rowspan, c.colspan, c.text.clone())).collect();
                if got == expected {
                    exact += 1
                } else {
                    wrong_but_returned += 1;
                    println!("expected {expected:?}
 got {got:?}");
                }
            }
            _ => refused += 1,
        }
    }
    println!("synthetic tables: total {total}, exact {exact}, wrong but returned {wrong_but_returned}, refused {refused}");
    assert_eq!(wrong_but_returned, 0);
    assert_eq!(exact, total);
}

#[test]
fn synthetic_tables_with_words_on_lines_are_refused_not_misread() {
    let mut rng = Lcg(11);
    let total = 200;
    let (mut refused, mut wrong, mut exact) = (0, 0, 0);
    for _ in 0..total {
        let (segs, mut glyphs, expected) = random_case(&mut rng);
        let moved = (glyphs.len() / 4).max(1);
        for g in glyphs.iter_mut().take(moved) {
            let shift = g.rect.x1 - g.rect.x0;
            g.rect.x0 = (g.rect.x0 / 60.0).round() * 60.0 - shift / 2.0;
            g.rect.x1 = g.rect.x0 + shift;
        }
        match run(&segs, &glyphs).as_slice() {
            [Found { result: Ok(t), .. }] => {
                let got: Vec<(u32, u32, u32, u32, String)> =
                    t.cells.iter().map(|c| (c.row, c.col, c.rowspan, c.colspan, c.text.clone())).collect();
                if got == expected {
                    exact += 1
                } else {
                    wrong += 1;
                }
            }
            _ => refused += 1,
        }
    }
    println!("words on lines: total {total}, refused {refused}, wrong but returned {wrong}, exact {exact}");
    assert_eq!(wrong, 0);
}
