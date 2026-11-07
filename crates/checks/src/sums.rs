use std::collections::BTreeMap;
use std::sync::LazyLock;

use docmodel::{Anchor, Cell, Table};
use regex::Regex;

use crate::amount::{parse_amount, parse_percent};
use crate::finding::{Finding, FindingKind, Severity};

static TOTAL_LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(합\s*계|총\s*계|총\s*액|계)\s*$").expect("TOTAL_LABEL 정규식"));
static RATIO_HEADER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(비율|지분율|구성비)").expect("RATIO_HEADER 정규식"));

fn grid(t: &Table) -> BTreeMap<(u32, u32), &Cell> {
    t.cells.iter().map(|c| ((c.row, c.col), c)).collect()
}

fn single_cells(t: &Table) -> bool {
    t.cells.iter().all(|c| c.rowspan.max(1) == 1 && c.colspan.max(1) == 1)
}

fn rows_cols(t: &Table) -> (u32, u32) {
    let rows = t.cells.iter().map(|c| c.row + 1).max().unwrap_or(0);
    let cols = t.cells.iter().map(|c| c.col + 1).max().unwrap_or(0);
    (rows, cols)
}

fn mismatch(kind: FindingKind, message: String, anchors: Vec<Anchor>) -> Finding {
    Finding { kind, message, anchors, severity: Severity::Error }
}

pub fn check_table_sums(t: &Table) -> Vec<Finding> {
    if t.degraded || !single_cells(t) {
        return Vec::new();
    }
    let g = grid(t);
    let (rows, cols) = rows_cols(t);
    let mut out = Vec::new();
    for r in 1..rows {
        let Some(label) = g.get(&(r, 0)) else { continue };
        if !TOTAL_LABEL.is_match(&label.text) {
            continue;
        }
        for c in 1..cols {
            let Some(total_cell) = g.get(&(r, c)) else { continue };
            let Some(total) = parse_amount(&total_cell.text) else { continue };
            let mut sum = 0i64;
            let mut parts = 0usize;
            let mut unreadable = false;
            for pr in 1..r {
                match g.get(&(pr, c)).map(|cell| cell.text.trim()) {
                    Some("") | None => {}
                    Some(text) => match parse_amount(text) {
                        Some(v) => {
                            sum = sum.saturating_add(v);
                            parts += 1;
                        }
                        None => unreadable = true,
                    },
                }
            }
            if unreadable || parts < 2 {
                continue;
            }
            if sum != total {
                out.push(mismatch(
                    FindingKind::SumMismatch,
                    format!("합계 칸 값 {total}이 위 칸들의 합 {sum}과 다릅니다"),
                    vec![total_cell.anchor.clone(), label.anchor.clone()],
                ));
            }
        }
    }
    out
}

pub fn check_ratios(t: &Table) -> Vec<Finding> {
    if t.degraded || !single_cells(t) {
        return Vec::new();
    }
    let g = grid(t);
    let (rows, cols) = rows_cols(t);
    let mut out = Vec::new();
    for c in 0..cols {
        let Some(head) = g.get(&(0, c)) else { continue };
        if !RATIO_HEADER.is_match(&head.text) {
            continue;
        }
        let mut sum = 0.0f64;
        let mut parts = 0usize;
        let mut has_total_row = false;
        for r in 1..rows {
            if g.get(&(r, 0)).is_some_and(|l| TOTAL_LABEL.is_match(&l.text)) {
                has_total_row = true;
                continue;
            }
            if let Some(v) = g.get(&(r, c)).and_then(|cell| parse_percent(&cell.text)) {
                sum += v;
                parts += 1;
            }
        }
        if parts >= 2 && (sum - 100.0).abs() > 0.01 && !has_total_row {
            out.push(mismatch(
                FindingKind::RatioMismatch,
                format!("비율 칸의 합이 {sum}%로 100%가 아닙니다"),
                vec![head.anchor.clone()],
            ));
        }
    }
    out
}

#[cfg(test)]
#[path = "sums_tests.rs"]
mod tests;
