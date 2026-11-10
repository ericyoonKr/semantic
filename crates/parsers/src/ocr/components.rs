#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Component {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
    pub pixels: u32,
    pub weight: f32,
}

impl Component {
    pub fn width(&self) -> u32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> u32 {
        self.y1 - self.y0
    }

    pub fn mean_weight(&self) -> f32 {
        self.weight / self.pixels.max(1) as f32
    }
}

pub(crate) fn components(width: u32, height: u32, value: impl Fn(usize) -> Option<f32>) -> Vec<Component> {
    let (w, h) = (width as usize, height as usize);
    let mut seen = vec![false; w * h];
    let mut stack = Vec::new();
    let mut out = Vec::new();
    for start in 0..w * h {
        if seen[start] || value(start).is_none() {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut c = Component { x0: u32::MAX, y0: u32::MAX, x1: 0, y1: 0, pixels: 0, weight: 0.0 };
        while let Some(i) = stack.pop() {
            let (x, y) = ((i % w) as u32, (i / w) as u32);
            c.x0 = c.x0.min(x);
            c.y0 = c.y0.min(y);
            c.x1 = c.x1.max(x + 1);
            c.y1 = c.y1.max(y + 1);
            c.pixels += 1;
            c.weight += value(i).unwrap_or(0.0);
            let (x, y) = (x as usize, y as usize);
            let neighbours = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for n in neighbours.into_iter().flatten() {
                if !seen[n] && value(n).is_some() {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::components;

    #[test]
    fn finds_separate_blobs_with_bounds_counts_and_weights() {
        let grid = [1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 1];
        let found = components(4, 3, |i| (grid[i] == 1).then_some(0.5));
        assert_eq!(found.len(), 2);
        let first = found[0];
        assert_eq!((first.x0, first.y0, first.x1, first.y1, first.pixels), (0, 0, 2, 2, 4));
        assert!((first.mean_weight() - 0.5).abs() < 1e-6);
        assert_eq!((found[1].x0, found[1].y0, found[1].pixels), (3, 2, 1));
    }
}
