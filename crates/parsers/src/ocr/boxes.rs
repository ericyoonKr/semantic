const BIN_THRESH: f32 = 0.3;
const BOX_THRESH: f32 = 0.6;
const UNCLIP_RATIO: f32 = 1.5;
const MIN_SIDE: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelBox {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

pub fn find_boxes(prob: &[f32], width: u32, height: u32) -> Vec<PixelBox> {
    let (w, h) = (width as usize, height as usize);
    if prob.len() < w * h {
        return Vec::new();
    }
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if seen[start] || prob[start] <= BIN_THRESH {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        let (mut sum, mut count) = (0f32, 0u32);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            sum += prob[i];
            count += 1;
            let neighbours = [
                (x > 0).then(|| i - 1),
                (x + 1 < w).then(|| i + 1),
                (y > 0).then(|| i - w),
                (y + 1 < h).then(|| i + w),
            ];
            for n in neighbours.into_iter().flatten() {
                if !seen[n] && prob[n] > BIN_THRESH {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        let bw = (x1 - x0 + 1) as f32;
        let bh = (y1 - y0 + 1) as f32;
        if (bw as u32).min(bh as u32) < MIN_SIDE || sum / (count as f32) < BOX_THRESH {
            continue;
        }
        let d = bw * bh * UNCLIP_RATIO / (2.0 * (bw + bh));
        let grow = |v: usize, by: f32| (v as f32 - by).max(0.0) as u32;
        out.push(PixelBox {
            x0: grow(x0, d),
            y0: grow(y0, d),
            x1: ((x1 + 1) as f32 + d).min(width as f32) as u32,
            y1: ((y1 + 1) as f32 + d).min(height as f32) as u32,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::find_boxes;

    #[test]
    fn two_blobs_give_two_grown_boxes_and_faint_blob_is_dropped() {
        let (w, h) = (40u32, 20u32);
        let mut prob = vec![0f32; (w * h) as usize];
        let mut fill = |x0: u32, x1: u32, y0: u32, y1: u32, v: f32| {
            for y in y0..y1 {
                for x in x0..x1 {
                    prob[(y * w + x) as usize] = v;
                }
            }
        };
        fill(2, 12, 5, 9, 0.9);
        fill(20, 34, 5, 9, 0.9);
        fill(2, 12, 14, 18, 0.4);
        let boxes = find_boxes(&prob, w, h);
        assert_eq!(boxes.len(), 2);
        assert!(boxes[0].x0 < 2 && boxes[0].x1 > 12 && boxes[0].y0 < 5 && boxes[0].y1 > 9);
        assert!(boxes.iter().all(|b| b.x1 <= w && b.y1 <= h));
    }
}
