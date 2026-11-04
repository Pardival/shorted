//! Pictures brought in by the user (T-0704): size limits checked before decoding, animated
//! GIF / WebP / APNG decoded to frames, and small previews.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, Frames, ImageReader};

/// Limits of docs/08: crafted files must not exhaust memory.
pub const MAX_SIDE: u32 = 16_384;
pub const MAX_PIXELS: u64 = 100_000_000;
pub const MAX_FRAMES: usize = 500;
/// All frames of one animation, decoded.
const MAX_ANIMATION_BYTES: u64 = 512 * 1024 * 1024;
/// Browsers treat shorter delays as 100 ms; so do we, or some GIFs would race.
const MIN_DELAY_US: i64 = 20_000;
const DEFAULT_DELAY_US: i64 = 100_000;

#[derive(Debug, PartialEq, Eq)]
pub enum PictureError {
    Unsupported,
    TooLarge,
}

/// Quality of a cover picture: sharp enough for a thumbnail, still small.
const COVER_JPEG_QUALITY: u8 = 90;

/// A JPEG of BGRA pixels, for a cover picture (T-0903).
pub fn jpeg_from_bgra(bgra: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let rgb: Vec<u8> = bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[b, g, r, _]| [r, g, b])
        .collect();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, COVER_JPEG_QUALITY)
        .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some(out)
}

/// A decoded animation: every frame at full size, each with the time it starts.
pub struct Animation {
    pub width: u32,
    pub height: u32,
    /// (start in microseconds from the first frame, RGBA pixels).
    pub frames: Vec<(i64, Vec<u8>)>,
    pub length_us: i64,
}

impl Animation {
    /// The frame shown `local_us` after the element appears; animations loop.
    pub fn frame_at(&self, local_us: i64) -> usize {
        if self.frames.len() < 2 || self.length_us <= 0 {
            return 0;
        }
        let at = local_us.rem_euclid(self.length_us);
        self.frames
            .partition_point(|(start, _)| *start <= at)
            .saturating_sub(1)
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Width and height from the header, refused when beyond the limits.
pub fn checked_size(path: &Path) -> Result<(u32, u32), PictureError> {
    let (width, height) = if extension(path) == "svg" {
        let data = std::fs::read(path).map_err(|_| PictureError::Unsupported)?;
        crate::library::svg_size(&data).ok_or(PictureError::Unsupported)?
    } else {
        ImageReader::open(path)
            .and_then(ImageReader::with_guessed_format)
            .map_err(|_| PictureError::Unsupported)?
            .into_dimensions()
            .map_err(|_| PictureError::Unsupported)?
    };
    within_limits(width, height)?;
    Ok((width, height))
}

fn within_limits(width: u32, height: u32) -> Result<(), PictureError> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 {
        return Err(PictureError::Unsupported);
    }
    if width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS {
        return Err(PictureError::TooLarge);
    }
    Ok(())
}

fn reader(path: &Path) -> Result<BufReader<File>, PictureError> {
    File::open(path)
        .map(BufReader::new)
        .map_err(|_| PictureError::Unsupported)
}

/// The frames of an animated file, or `None` when it holds one picture.
fn frames(path: &Path) -> Result<Option<Frames<'static>>, PictureError> {
    let bad = |_| PictureError::Unsupported;
    Ok(match extension(path).as_str() {
        "gif" => Some(GifDecoder::new(reader(path)?).map_err(bad)?.into_frames()),
        "webp" => {
            let decoder = WebPDecoder::new(reader(path)?).map_err(bad)?;
            decoder.has_animation().then(|| decoder.into_frames())
        }
        "png" | "apng" => {
            let decoder = PngDecoder::new(reader(path)?).map_err(bad)?;
            if decoder.is_apng().map_err(bad)? {
                Some(decoder.apng().map_err(bad)?.into_frames())
            } else {
                None
            }
        }
        _ => None,
    })
}

/// Whether the file moves. A file that cannot be read is not.
pub fn is_animated(path: &Path) -> bool {
    matches!(frames(path), Ok(Some(_)))
}

/// Decodes every frame, within the limits.
pub fn decode_animation(path: &Path) -> Result<Animation, PictureError> {
    let (width, height) = checked_size(path)?;
    let frames = frames(path)?.ok_or(PictureError::Unsupported)?;
    let frame_bytes = u64::from(width) * u64::from(height) * 4;
    let mut decoded = Vec::new();
    let mut at = 0;
    for frame in frames {
        if decoded.len() == MAX_FRAMES
            || (decoded.len() as u64 + 1) * frame_bytes > MAX_ANIMATION_BYTES
        {
            return Err(PictureError::TooLarge);
        }
        let frame = frame.map_err(|_| PictureError::Unsupported)?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay_us = if numerator == 0 {
            DEFAULT_DELAY_US
        } else {
            (i64::from(numerator) * 1000 / i64::from(denominator.max(1))).max(MIN_DELAY_US)
        };
        let buffer = frame.into_buffer();
        if buffer.dimensions() != (width, height) {
            return Err(PictureError::Unsupported);
        }
        decoded.push((at, buffer.into_raw()));
        at += delay_us;
    }
    if decoded.is_empty() {
        return Err(PictureError::Unsupported);
    }
    Ok(Animation {
        width,
        height,
        frames: decoded,
        length_us: at,
    })
}

/// A small RGBA picture of a still or animated file, at most `size` on its longer side.
pub fn preview(path: &Path, size: u32) -> Option<(u32, u32, Vec<u8>)> {
    checked_size(path).ok()?;
    let (width, height, rgba) = match decode_animation(path) {
        Ok(animation) => {
            let first = animation.frames.into_iter().next()?.1;
            (animation.width, animation.height, first)
        }
        Err(_) => crate::compositor::load_image(path)?,
    };
    let picture = image::RgbaImage::from_raw(width, height, rgba)?;
    let scale = (size as f32 / width.max(height) as f32).min(1.0);
    let small = image::imageops::thumbnail(
        &picture,
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    );
    Some((small.width(), small.height(), small.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, Rgba, RgbaImage};

    fn gif(dir: &Path, frames: usize, side: u32, delay_ms: u32) -> std::path::PathBuf {
        let path = dir.join(format!("a{frames}-{side}.gif"));
        let file = File::create(&path).unwrap();
        let mut encoder = GifEncoder::new(file);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for index in 0..frames {
            let shade = (index * 40 % 255) as u8;
            let picture = RgbaImage::from_pixel(side, side, Rgba([shade, 0, 0, 255]));
            let delay = Delay::from_numer_denom_ms(delay_ms, 1);
            encoder
                .encode_frame(Frame::from_parts(picture, 0, 0, delay))
                .unwrap();
        }
        drop(encoder);
        path
    }

    #[test]
    fn a_gif_decodes_with_its_timing_and_loops() {
        let dir = tempfile::tempdir().unwrap();
        let path = gif(dir.path(), 3, 8, 100);
        assert!(is_animated(&path));
        let animation = decode_animation(&path).unwrap();
        assert_eq!(animation.frames.len(), 3);
        assert_eq!(animation.length_us, 300_000);
        assert_eq!(animation.frame_at(0), 0);
        assert_eq!(animation.frame_at(150_000), 1);
        assert_eq!(animation.frame_at(299_999), 2);
        assert_eq!(animation.frame_at(310_000), 0);
        assert_eq!(animation.frames[1].1[0], 40);
    }

    #[test]
    fn too_many_frames_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = gif(dir.path(), MAX_FRAMES + 1, 2, 20);
        assert_eq!(decode_animation(&path).err(), Some(PictureError::TooLarge));
    }

    #[test]
    fn sizes_beyond_the_limits_are_refused() {
        assert_eq!(within_limits(MAX_SIDE + 1, 10), Err(PictureError::TooLarge));
        assert_eq!(within_limits(12_000, 12_000), Err(PictureError::TooLarge));
        assert_eq!(within_limits(0, 10), Err(PictureError::Unsupported));
        assert_eq!(within_limits(4000, 3000), Ok(()));
    }

    #[test]
    fn a_still_png_is_not_animated_and_has_a_preview() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("still.png");
        RgbaImage::from_pixel(40, 20, Rgba([0, 0, 255, 255]))
            .save(&path)
            .unwrap();
        assert!(!is_animated(&path));
        assert_eq!(checked_size(&path), Ok((40, 20)));
        let (width, height, rgba) = preview(&path, 10).unwrap();
        assert_eq!((width, height), (10, 5));
        assert_eq!(&rgba[..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn a_cover_jpeg_decodes_back_to_its_size_and_colour() {
        // Two by one, both pure blue in BGRA.
        let bgra = [255, 0, 0, 255, 255, 0, 0, 255];
        let jpeg = jpeg_from_bgra(&bgra, 2, 1).unwrap();
        let back = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        assert_eq!(back.dimensions(), (2, 1));
        let pixel = back.get_pixel(0, 0);
        assert!(pixel[2] > 200 && pixel[0] < 60, "{pixel:?}");
    }
}
