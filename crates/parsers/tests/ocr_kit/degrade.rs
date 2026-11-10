use image::{GrayImage, Luma};

#[derive(Debug, Default, Clone, Copy)]
pub struct Degrade {
    pub rotate_deg: f32,
    pub noise: u8,
    pub blur: f32,
}

impl Degrade {
    pub fn from_env(key: &str) -> Self {
        let spec = std::env::var(key).unwrap_or_default();
        let mut it = spec.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(0.0));
        Self {
            rotate_deg: it.next().unwrap_or(0.0),
            noise: it.next().unwrap_or(0.0).clamp(0.0, 255.0) as u8,
            blur: it.next().unwrap_or(0.0),
        }
    }

    pub fn apply(&self, img: &GrayImage, seed: u64) -> GrayImage {
        let mut rng = Lcg(seed);
        let rotated = self.rotate(img, if rng.next().is_multiple_of(2) { 1.0 } else { -1.0 });
        let noisy = self.add_noise(rotated, &mut rng);
        if self.blur > 0.0 { image::imageops::blur(&noisy, self.blur) } else { noisy }
    }

    fn rotate(&self, img: &GrayImage, sign: f32) -> GrayImage {
        if self.rotate_deg == 0.0 {
            return img.clone();
        }
        let (sin, cos) = (sign * self.rotate_deg).to_radians().sin_cos();
        let (w, h) = img.dimensions();
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        GrayImage::from_fn(w, h, |x, y| {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let (sx, sy) = ((cos * dx + sin * dy + cx).round(), (-sin * dx + cos * dy + cy).round());
            if sx < 0.0 || sy < 0.0 || sx >= w as f32 || sy >= h as f32 {
                Luma([255])
            } else {
                *img.get_pixel(sx as u32, sy as u32)
            }
        })
    }

    fn add_noise(&self, mut img: GrayImage, rng: &mut Lcg) -> GrayImage {
        if self.noise == 0 {
            return img;
        }
        let n = i32::from(self.noise);
        for p in img.pixels_mut() {
            let delta = (rng.next() % (2 * n as u32 + 1)) as i32 - n;
            p.0[0] = (i32::from(p.0[0]) + delta).clamp(0, 255) as u8;
        }
        img
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
}
