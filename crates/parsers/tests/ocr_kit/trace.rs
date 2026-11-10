use image::{Rgb, RgbImage};
use parsers::{TraceLine, TracePart};

const BODY: Rgb<u8> = Rgb([0, 160, 0]);
const SUPERSCRIPT: Rgb<u8> = Rgb([220, 0, 0]);
const LINE: Rgb<u8> = Rgb([0, 90, 220]);

fn outline(img: &mut RgbImage, [x0, y0, x1, y1]: [u32; 4], color: Rgb<u8>) {
    let (w, h) = img.dimensions();
    let (x1, y1) = (x1.min(w).saturating_sub(1), y1.min(h).saturating_sub(1));
    for x in x0.min(x1)..=x1 {
        img.put_pixel(x, y0.min(y1), color);
        img.put_pixel(x, y1, color);
    }
    for y in y0.min(y1)..=y1 {
        img.put_pixel(x0.min(x1), y, color);
        img.put_pixel(x1, y, color);
    }
}

pub fn draw(img: &mut RgbImage, lines: &[TraceLine]) {
    for line in lines {
        outline(img, line.area, LINE);
        for part in &line.parts {
            outline(img, part.area, if part.superscript { SUPERSCRIPT } else { BODY });
        }
    }
}

fn part_text(p: &TracePart) -> String {
    format!("{}[{}]", if p.superscript { "위첨자" } else { "본문" }, p.text)
}

pub fn describe(line: &TraceLine) -> String {
    let parts: Vec<String> = line.parts.iter().map(part_text).collect();
    format!("{:?}\t{}", line.area, parts.join(" | "))
}

pub fn crop_from_env(key: &str) -> Option<[u32; 4]> {
    let spec = std::env::var(key).ok()?;
    let v: Vec<u32> = spec.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    match v.as_slice() {
        [x0, y0, x1, y1] if x1 > x0 && y1 > y0 => Some([*x0, *y0, *x1, *y1]),
        _ => None,
    }
}
