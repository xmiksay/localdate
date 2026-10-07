//! Photo normalisation: decode, honour EXIF orientation, downscale, re-encode as WebP.
//! Re-encoding drops all metadata (EXIF GPS included).

use std::io::Cursor;

use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, Limits};

pub const MAX_EDGE: u32 = 1280;

/// Output size with the long edge capped at `max_edge`, aspect kept, never upscaled.
pub fn fit_dimensions(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= max_edge {
        return (width, height);
    }
    let scale =
        |side: u32| ((u64::from(side) * u64::from(max_edge)) / u64::from(long)).max(1) as u32;
    (scale(width), scale(height))
}

/// Returns WebP bytes, or `None` when the input is not a decodable jpeg/png/webp.
pub fn to_webp(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    // Guards against decompression bombs from a tiny upload.
    let mut limits = Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);

    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut img = DynamicImage::from_decoder(decoder).ok()?;
    img.apply_orientation(orientation);

    let (w, h) = fit_dimensions(img.width(), img.height(), MAX_EDGE);
    if (w, h) != (img.width(), img.height()) {
        img = img.resize_exact(w, h, FilterType::Lanczos3);
    }

    let mut out = Vec::new();
    let encoder = WebPEncoder::new_lossless(&mut out);
    if img.color().has_alpha() {
        encoder
            .write_image(img.to_rgba8().as_raw(), w, h, ExtendedColorType::Rgba8)
            .ok()?;
    } else {
        encoder
            .write_image(img.to_rgb8().as_raw(), w, h, ExtendedColorType::Rgb8)
            .ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        RgbImage::new(w, h)
            .write_to(&mut buf, ImageFormat::Png)
            .expect("encode png");
        buf.into_inner()
    }

    #[test]
    fn fit_keeps_small_images() {
        assert_eq!(fit_dimensions(800, 600, 1280), (800, 600));
        assert_eq!(fit_dimensions(1280, 1280, 1280), (1280, 1280));
    }

    #[test]
    fn fit_scales_long_edge_and_keeps_aspect() {
        assert_eq!(fit_dimensions(2560, 1280, 1280), (1280, 640));
        assert_eq!(fit_dimensions(1000, 4000, 1280), (320, 1280));
        assert_eq!(fit_dimensions(10_000, 1, 1280), (1280, 1));
    }

    #[test]
    fn converts_png_to_downscaled_webp() {
        let out = to_webp(&png(2000, 1000)).expect("decodes");
        assert_eq!(
            image::guess_format(&out).expect("format"),
            ImageFormat::WebP
        );
        let img = image::load_from_memory(&out).expect("webp decodes");
        assert_eq!((img.width(), img.height()), (1280, 640));
    }

    #[test]
    fn rejects_garbage() {
        assert!(to_webp(b"definitely not an image").is_none());
        assert!(to_webp(&[]).is_none());
    }
}
