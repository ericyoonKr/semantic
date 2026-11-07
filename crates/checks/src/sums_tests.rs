use docmodel::{Anchor, Cell, FactSource, FileId, Table};

use super::{check_ratios, check_table_sums};
use crate::finding::FindingKind;

fn anchor() -> Anchor {
    Anchor { source: FactSource::File(FileId("f".into())), page: Some(1), bbox: None, path: None, span: None }
}

fn table(rows: &[&[&str]]) -> Table {
    let mut cells = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, text) in row.iter().enumerate() {
            cells.push(Cell { row: r as u32, col: c as u32, rowspan: 1, colspan: 1, text: (*text).to_string(), anchor: anchor() });
        }
    }
    Table { cells, anchor: anchor(), degraded: false }
}

#[test]
fn correct_total_passes() {
    let t = table(&[&["항목", "금액"], &["가", "1,000,000원"], &["나", "금 이백만원"], &["합계", "3,000,000원"]]);
    assert!(check_table_sums(&t).is_empty());
}

#[test]
fn wrong_total_is_flagged() {
    let t = table(&[&["항목", "금액", "비고"], &["가", "1,000,000", "-"], &["나", "2,500,000", "-"], &["계", "3,000,000", "-"]]);
    let f = check_table_sums(&t);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].kind, FindingKind::SumMismatch);
}

#[test]
fn degraded_or_merged_table_is_skipped() {
    let mut t = table(&[&["항목", "금액"], &["가", "1"], &["나", "2"], &["합계", "9"]]);
    t.degraded = true;
    assert!(check_table_sums(&t).is_empty());
    t.degraded = false;
    t.cells[0].colspan = 2;
    assert!(check_table_sums(&t).is_empty());
}

#[test]
fn unreadable_part_skips_column() {
    let t = table(&[&["항목", "금액"], &["가", "1,000"], &["나", "별도 협의"], &["합계", "5,000"]]);
    assert!(check_table_sums(&t).is_empty());
}

#[test]
fn ratios_must_reach_hundred() {
    let ok = table(&[&["주주", "지분율"], &["갑", "60%"], &["을", "40%"]]);
    assert!(check_ratios(&ok).is_empty());
    let bad = table(&[&["주주", "지분율"], &["갑", "60%"], &["을", "30%"]]);
    assert_eq!(check_ratios(&bad).len(), 1);
}
