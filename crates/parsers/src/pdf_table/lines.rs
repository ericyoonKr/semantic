use super::{Segment, TableSettings};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HLine {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VLine {
    pub x: f32,
    pub y0: f32,
    pub y1: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Ruling {
    pub h: Vec<HLine>,
    pub v: Vec<VLine>,
}

pub(crate) fn classify(segments: &[Segment], s: &TableSettings) -> Ruling {
    let mut out = Ruling::default();
    for seg in segments {
        let (dx, dy) = ((seg.x1 - seg.x0).abs(), (seg.y1 - seg.y0).abs());
        if !(dx.is_finite() && dy.is_finite()) {
            continue;
        }
        if dy <= s.slope_tol && dx >= s.min_len {
            out.h.push(HLine { y: (seg.y0 + seg.y1) / 2.0, x0: seg.x0.min(seg.x1), x1: seg.x0.max(seg.x1) });
        } else if dx <= s.slope_tol && dy >= s.min_len {
            out.v.push(VLine { x: (seg.x0 + seg.x1) / 2.0, y0: seg.y0.min(seg.y1), y1: seg.y0.max(seg.y1) });
        }
    }
    join(out, s.snap_eps)
}

fn join_axis(mut items: Vec<(f32, f32, f32)>, eps: f32) -> Vec<(f32, f32, f32)> {
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut clusters: Vec<Vec<(f32, f32, f32)>> = Vec::new();
    for item in items {
        match clusters.last_mut() {
            Some(c) if c.last().is_some_and(|l| item.0 - l.0 <= eps) => c.push(item),
            _ => clusters.push(vec![item]),
        }
    }
    let mut out = Vec::new();
    for mut cluster in clusters {
        let at = cluster.iter().map(|c| c.0).sum::<f32>() / cluster.len() as f32;
        cluster.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut run: Option<(f32, f32)> = None;
        for (_, lo, hi) in cluster {
            run = match run {
                Some((a, b)) if lo <= b + eps => Some((a, b.max(hi))),
                Some((a, b)) => {
                    out.push((at, a, b));
                    Some((lo, hi))
                }
                None => Some((lo, hi)),
            };
        }
        if let Some((a, b)) = run {
            out.push((at, a, b));
        }
    }
    out
}

fn join(ruling: Ruling, eps: f32) -> Ruling {
    let h = join_axis(ruling.h.iter().map(|l| (l.y, l.x0, l.x1)).collect(), eps);
    let v = join_axis(ruling.v.iter().map(|l| (l.x, l.y0, l.y1)).collect(), eps);
    Ruling {
        h: h.into_iter().map(|(y, x0, x1)| HLine { y, x0, x1 }).collect(),
        v: v.into_iter().map(|(x, y0, y1)| VLine { x, y0, y1 }).collect(),
    }
}

fn touches(h: &HLine, v: &VLine, eps: f32) -> bool {
    v.x >= h.x0 - eps && v.x <= h.x1 + eps && h.y >= v.y0 - eps && h.y <= v.y1 + eps
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra] = rb;
    }
}

pub(crate) fn groups(ruling: &Ruling, eps: f32) -> Vec<Ruling> {
    let nh = ruling.h.len();
    let mut parent: Vec<usize> = (0..nh + ruling.v.len()).collect();
    for (i, h) in ruling.h.iter().enumerate() {
        for (j, v) in ruling.v.iter().enumerate() {
            if touches(h, v, eps) {
                union(&mut parent, i, nh + j);
            }
        }
    }
    let mut roots: Vec<usize> = Vec::new();
    let mut out: Vec<Ruling> = Vec::new();
    for idx in 0..parent.len() {
        let root = find(&mut parent, idx);
        let slot = match roots.iter().position(|r| *r == root) {
            Some(p) => p,
            None => {
                roots.push(root);
                out.push(Ruling::default());
                out.len() - 1
            }
        };
        if idx < nh {
            out[slot].h.push(ruling.h[idx]);
        } else {
            out[slot].v.push(ruling.v[idx - nh]);
        }
    }
    out.into_iter().filter(|g| !g.h.is_empty() && !g.v.is_empty()).collect()
}

pub(crate) fn snap(values: impl Iterator<Item = f32>, eps: f32) -> Vec<f32> {
    let mut sorted: Vec<f32> = values.filter(|v| v.is_finite()).collect();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut out: Vec<f32> = Vec::new();
    let mut run: Vec<f32> = Vec::new();
    for v in sorted {
        if let Some(last) = run.last()
            && v - *last > eps
        {
            out.push(run.iter().sum::<f32>() / run.len() as f32);
            run.clear();
        }
        run.push(v);
    }
    if !run.is_empty() {
        out.push(run.iter().sum::<f32>() / run.len() as f32);
    }
    out
}

pub(crate) fn has_vertical(group: &Ruling, x: f32, y_lo: f32, y_hi: f32, eps: f32) -> bool {
    group.v.iter().any(|v| (v.x - x).abs() <= eps && v.y0 <= y_lo + eps && v.y1 >= y_hi - eps)
}

pub(crate) fn has_horizontal(group: &Ruling, y: f32, x_lo: f32, x_hi: f32, eps: f32) -> bool {
    group.h.iter().any(|h| (h.y - y).abs() <= eps && h.x0 <= x_lo + eps && h.x1 >= x_hi - eps)
}
