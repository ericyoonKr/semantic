use super::components::components;

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

impl PixelBox {
    pub fn width(&self) -> u32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> u32 {
        self.y1 - self.y0
    }
}

pub fn find_boxes(prob: &[f32], width: u32, height: u32) -> Vec<PixelBox> {
    if prob.len() < (width * height) as usize {
        return Vec::new();
    }
    components(width, height, |i| prob.get(i).copied().filter(|p| *p > BIN_THRESH))
        .into_iter()
        .filter(|c| c.width().min(c.height()) >= MIN_SIDE && c.mean_weight() >= BOX_THRESH)
        .map(|c| {
            let (bw, bh) = (c.width() as f32, c.height() as f32);
            let d = bw * bh * UNCLIP_RATIO / (2.0 * (bw + bh));
            PixelBox {
                x0: (c.x0 as f32 - d).max(0.0) as u32,
                y0: (c.y0 as f32 - d).max(0.0) as u32,
                x1: (c.x1 as f32 + d).min(width as f32) as u32,
                y1: (c.y1 as f32 + d).min(height as f32) as u32,
            }
        })
        .collect()
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
