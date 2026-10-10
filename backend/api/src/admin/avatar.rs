//! Placeholder avatars for test users: a pale silhouette on a two-tone background whose colour
//! comes from a seed. Encoded as PNG so it goes through the normal photo pipeline like an upload.

use std::io::Cursor;

use anyhow::Context;

use image::{ImageFormat, Rgb, RgbImage};

const SIZE: u32 = 512;

/// HSV (h in 0..360, s and v in 0..=1) → RGB.
fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to(r), to(g), to(b)]
}

fn inside_ellipse(x: f32, y: f32, (cx, cy): (f32, f32), (rx, ry): (f32, f32)) -> bool {
    let (dx, dy) = ((x - cx) / rx, (y - cy) / ry);
    dx * dx + dy * dy <= 1.0
}

/// A `SIZE`² PNG; the same seed always gives the same picture.
pub fn placeholder_png(seed: u64) -> anyhow::Result<Vec<u8>> {
    let hue = (seed % 360) as f32;
    let top = hsv(hue, 0.45, 0.85);
    let bottom = hsv((hue + 40.0) % 360.0, 0.55, 0.6);
    let figure = hsv(hue, 0.12, 0.97);
    let size = SIZE as f32;
    let img = RgbImage::from_fn(SIZE, SIZE, |x, y| {
        let (fx, fy) = (x as f32, y as f32);
        let head = inside_ellipse(fx, fy, (size * 0.5, size * 0.4), (size * 0.17, size * 0.19));
        let body = inside_ellipse(fx, fy, (size * 0.5, size * 1.0), (size * 0.36, size * 0.36));
        if head || body {
            return Rgb(figure);
        }
        let t = fy / size;
        let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - t) + f32::from(b) * t) as u8;
        Rgb([
            mix(top[0], bottom[0]),
            mix(top[1], bottom[1]),
            mix(top[2], bottom[2]),
        ])
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png)
        .context("encoding placeholder avatar")?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_is_a_decodable_square_png_that_varies_with_the_seed() {
        let png = placeholder_png(7).expect("encoded");
        let img = image::load_from_memory_with_format(&png, ImageFormat::Png).expect("png");
        assert_eq!((img.width(), img.height()), (SIZE, SIZE));
        assert_eq!(png, placeholder_png(7).expect("encoded"), "deterministic");
        assert_ne!(png, placeholder_png(200).expect("encoded"));
    }

    #[test]
    fn placeholder_survives_the_upload_pipeline() {
        let webp = crate::me::image_proc::to_webp(&placeholder_png(1).expect("encoded"))
            .expect("accepted");
        assert!(webp.starts_with(b"RIFF"));
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(hsv(0.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv(120.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv(240.0, 1.0, 1.0), [0, 0, 255]);
        assert_eq!(hsv(0.0, 0.0, 1.0), [255, 255, 255]);
    }
}
