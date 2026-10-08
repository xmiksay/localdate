//! Photo normalisation: decode, honour EXIF orientation, downscale, re-encode as WebP.
//! Re-encoding drops all metadata (EXIF GPS included).

use std::io::Cursor;

use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, Limits};

pub const MAX_EDGE: u32 = 1280;
/// Largest accepted input edge; anything a phone camera produces fits.
pub const MAX_INPUT_EDGE: u32 = 10_000;
/// Largest accepted input size (~33 MP, e.g. 7000×4700). Together with `MAX_DECODE_BYTES` this
/// bounds one decode at 128 MiB even for RGBA8; 48/50 MP full-resolution shots are refused.
pub const MAX_INPUT_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_DECODE_BYTES: u64 = MAX_INPUT_PIXELS * 4;

#[derive(Debug, PartialEq, Eq)]
pub enum Rejected {
    /// Not a decodable jpeg/png/webp.
    Undecodable,
    /// Dimensions or decode memory over the limits; refused before decoding pixel data.
    TooLarge,
}

fn within_limits(width: u32, height: u32) -> bool {
    width <= MAX_INPUT_EDGE
        && height <= MAX_INPUT_EDGE
        && u64::from(width) * u64::from(height) <= MAX_INPUT_PIXELS
}

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

/// Returns WebP bytes, or why the input was refused.
pub fn to_webp(bytes: &[u8]) -> Result<Vec<u8>, Rejected> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Rejected::Undecodable)?;
    // Guards against decompression bombs from a tiny upload.
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_INPUT_EDGE);
    limits.max_image_height = Some(MAX_INPUT_EDGE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);

    let decode_err = |e: image::ImageError| match e {
        image::ImageError::Limits(_) => Rejected::TooLarge,
        _ => Rejected::Undecodable,
    };
    let mut decoder = reader.into_decoder().map_err(decode_err)?;
    let (width, height) = decoder.dimensions();
    if !within_limits(width, height) {
        return Err(Rejected::TooLarge);
    }
    let orientation = decoder.orientation().map_err(decode_err)?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(decode_err)?;
    img.apply_orientation(orientation);

    let (w, h) = fit_dimensions(img.width(), img.height(), MAX_EDGE);
    if (w, h) != (img.width(), img.height()) {
        img = img.resize_exact(w, h, FilterType::Lanczos3);
    }

    let mut out = Vec::new();
    let encoder = WebPEncoder::new_lossless(&mut out);
    let written = if img.color().has_alpha() {
        encoder.write_image(img.to_rgba8().as_raw(), w, h, ExtendedColorType::Rgba8)
    } else {
        encoder.write_image(img.to_rgb8().as_raw(), w, h, ExtendedColorType::Rgb8)
    };
    written.map_err(|_| Rejected::Undecodable)?;
    Ok(out)
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
        assert_eq!(
            to_webp(b"definitely not an image"),
            Err(Rejected::Undecodable)
        );
        assert_eq!(to_webp(&[]), Err(Rejected::Undecodable));
    }

    #[test]
    fn limits_cover_edges_and_pixel_count() {
        assert!(within_limits(MAX_INPUT_EDGE, 1));
        assert!(within_limits(7000, 4700));
        assert!(!within_limits(MAX_INPUT_EDGE + 1, 1));
        assert!(!within_limits(1, MAX_INPUT_EDGE + 1));
        assert!(!within_limits(8000, 6000));
    }

    #[test]
    fn overlong_edge_is_too_large() {
        assert_eq!(
            to_webp(&png(MAX_INPUT_EDGE + 1, 1)),
            Err(Rejected::TooLarge)
        );
    }

    /// A PNG whose header claims `w`×`h` followed by a junk IDAT (the decoder reads up to the first
    /// IDAT before reporting dimensions), so the test needs no pixel memory.
    fn png_header_only(w: u32, h: u32) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = !0u32;
            for &b in data {
                crc ^= u32::from(b);
                for _ in 0..8 {
                    crc = if crc & 1 == 1 {
                        (crc >> 1) ^ 0xEDB8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            let len = u32::try_from(data.len()).expect("small chunk");
            out.extend_from_slice(&len.to_be_bytes());
            let start = out.len();
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            let crc = crc32(&out[start..]);
            out.extend_from_slice(&crc.to_be_bytes());
        }
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &[0; 8]);
        chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn too_many_pixels_is_refused_before_decoding() {
        assert_eq!(
            to_webp(&png_header_only(8000, 6000)),
            Err(Rejected::TooLarge)
        );
        // Same header within the limits gets past the size checks and fails on the missing data.
        assert_eq!(
            to_webp(&png_header_only(64, 64)),
            Err(Rejected::Undecodable)
        );
    }
}
