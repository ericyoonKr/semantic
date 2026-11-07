use super::TableError;
use super::lines::{Ruling, has_horizontal, has_vertical};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub row: usize,
    pub col: usize,
    pub rowspan: usize,
    pub colspan: usize,
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

pub(crate) fn merged_cells(group: &Ruling, xs: &[f32], ys_top_down: &[f32], eps: f32) -> Result<Vec<Span>, TableError> {
    let rows = ys_top_down.len().saturating_sub(1);
    let cols = xs.len().saturating_sub(1);
    if rows == 0 || cols == 0 {
        return Err(TableError::NoGrid);
    }
    let id = |r: usize, c: usize| r * cols + c;
    let mut parent: Vec<usize> = (0..rows * cols).collect();
    for r in 0..rows {
        let (top, bottom) = (ys_top_down[r], ys_top_down[r + 1]);
        for c in 0..cols {
            if c + 1 < cols && !has_vertical(group, xs[c + 1], bottom, top, eps) {
                let (a, b) = (find(&mut parent, id(r, c)), find(&mut parent, id(r, c + 1)));
                parent[a] = b;
            }
            if r + 1 < rows && !has_horizontal(group, bottom, xs[c], xs[c + 1], eps) {
                let (a, b) = (find(&mut parent, id(r, c)), find(&mut parent, id(r + 1, c)));
                parent[a] = b;
            }
        }
    }
    let mut members: Vec<(usize, Vec<(usize, usize)>)> = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let root = find(&mut parent, id(r, c));
            match members.iter_mut().find(|(k, _)| *k == root) {
                Some((_, list)) => list.push((r, c)),
                None => members.push((root, vec![(r, c)])),
            }
        }
    }
    let mut out = Vec::with_capacity(members.len());
    for (_, list) in members {
        let r0 = list.iter().map(|p| p.0).min().unwrap_or(0);
        let r1 = list.iter().map(|p| p.0).max().unwrap_or(0);
        let c0 = list.iter().map(|p| p.1).min().unwrap_or(0);
        let c1 = list.iter().map(|p| p.1).max().unwrap_or(0);
        if list.len() != (r1 - r0 + 1) * (c1 - c0 + 1) {
            return Err(TableError::Ambiguous("병합된 칸이 사각형이 아님".into()));
        }
        out.push(Span { row: r0, col: c0, rowspan: r1 - r0 + 1, colspan: c1 - c0 + 1 });
    }
    if out.len() < 2 {
        return Err(TableError::NoGrid);
    }
    out.sort_by_key(|s| (s.row, s.col));
    Ok(out)
}
