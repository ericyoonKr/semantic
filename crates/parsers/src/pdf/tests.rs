use docmodel::{Block, ExtractWarning, FactSource, FileId, Rect};

use super::layout::{PageInput, lay_out};
use super::ranges;
use super::words::{CharBox, words};
use crate::pdf_table::{Segment, TableSettings};

fn chars_at(text: &str, x: f32, y: f32) -> Vec<CharBox> {
    let mut out = Vec::new();
    let mut cx = x;
    for ch in text.chars() {
        out.push(CharBox { ch, rect: Rect { x0: cx, y0: y, x1: cx + 8.0, y1: y + 10.0 } });
        cx += if ch == ' ' { 4.0 } else { 9.0 };
    }
    out
}

#[test]
fn chars_become_words_split_by_space_and_gap() {
    let mut cs = chars_at("과징금 산정", 0.0, 0.0);
    cs.extend(chars_at("내역", 200.0, 0.0));
    let got: Vec<String> = words(&cs).into_iter().map(|w| w.text).collect();
    assert_eq!(got, vec!["과징금", "산정", "내역"]);
}

#[test]
fn empty_pages_are_reported_as_ranges() {
    assert_eq!(ranges(&[2, 3, 4, 7, 9, 10]), vec![(2, 4), (7, 7), (9, 10)]);
}

fn h(y: f32, x0: f32, x1: f32) -> Segment {
    Segment { x0, y0: y, x1, y1: y }
}

fn v(x: f32, y0: f32, y1: f32) -> Segment {
    Segment { x0: x, y0, x1: x, y1 }
}

#[test]
fn page_with_paragraph_above_a_ruled_table() {
    let mut chars = chars_at("제1조 목적", 0.0, 300.0);
    chars.extend(chars_at("항목", 20.0, 170.0));
    chars.extend(chars_at("금액", 120.0, 170.0));
    chars.extend(chars_at("기본", 20.0, 120.0));
    chars.extend(chars_at("1000", 120.0, 120.0));
    let segments = vec![
        h(200.0, 0.0, 200.0),
        h(150.0, 0.0, 200.0),
        h(100.0, 0.0, 200.0),
        v(0.0, 100.0, 200.0),
        v(100.0, 100.0, 200.0),
        v(200.0, 100.0, 200.0),
    ];
    let source = FactSource::File(FileId("f".into()));
    let out = lay_out(PageInput { number: 3, chars, segments }, &source, &TableSettings::default());
    assert!(out.warnings.is_empty());
    match out.blocks.as_slice() {
        [(_, Block::Para { text, anchor }), (_, Block::Table(t))] => {
            assert_eq!(text, "제1조 목적");
            assert_eq!(anchor.page, Some(3));
            let cells: Vec<&str> = t.cells.iter().map(|c| c.text.as_str()).collect();
            assert_eq!(cells, vec!["항목", "금액", "기본", "1000"]);
            assert!(!t.degraded);
        }
        other => panic_free_fail(other.len()),
    }
}

fn panic_free_fail(n: usize) {
    assert_eq!(n, usize::MAX);
}

#[test]
fn failed_table_falls_back_to_text_with_warning() {
    let mut chars = chars_at("걸친글자", 82.5, 170.0);
    chars.extend(chars_at("값", 20.0, 120.0));
    let segments = vec![
        h(200.0, 0.0, 200.0),
        h(150.0, 0.0, 200.0),
        h(100.0, 0.0, 200.0),
        v(0.0, 100.0, 200.0),
        v(100.0, 100.0, 200.0),
        v(200.0, 100.0, 200.0),
    ];
    let source = FactSource::File(FileId("f".into()));
    let out = lay_out(PageInput { number: 1, chars, segments }, &source, &TableSettings::default());
    assert!(matches!(out.warnings.as_slice(), [ExtractWarning::TableDegraded { .. }]));
    assert!(out.blocks.iter().all(|(_, b)| !matches!(b, Block::Table(_))));
    let text: Vec<&str> = out.blocks.iter().filter_map(|(_, b)| if let Block::Para { text, .. } = b { Some(text.as_str()) } else { None }).collect();
    assert_eq!(text, vec!["걸친글자\n값"]);
}
