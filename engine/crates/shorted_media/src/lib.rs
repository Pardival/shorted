//! MediaBackend trait and per-platform decode, encode, mux, probe and proxies.

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_vendor = "apple")]
pub mod apple;
#[cfg(target_vendor = "apple")]
pub mod apple_writer;

use std::path::Path;

use thiserror::Error;

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_media";

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("the file has no video track")]
    NoVideoTrack,
    #[error("the decoder could not be opened")]
    OpenFailed,
    #[error("decoding failed")]
    DecodeFailed,
    #[error("encoding failed")]
    EncodeFailed,
}

/// Facts about a video file. Sizes are as stored; `rotation_degrees` says how to display them.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaInfo {
    pub width: u32,
    pub height: u32,
    pub rotation_degrees: i32,
    pub duration_us: i64,
    pub nominal_fps: f32,
}

impl MediaInfo {
    /// Size once rotation metadata is applied.
    pub fn display_size(&self) -> (u32, u32) {
        if self.rotation_degrees.rem_euclid(180) == 90 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

/// A decoded frame. Timing comes from the file, never from a frame index, so variable frame rate
/// clips stay in sync.
/// Reads an image's size from its header only.
pub fn probe_image(path: &Path) -> Result<(u32, u32), MediaError> {
    let size = imagesize::size(path).map_err(|_| MediaError::OpenFailed)?;
    Ok((size.width as u32, size.height as u32))
}

/// A small RGBA image of a BGRA one, by nearest neighbour, turned as `rotation_degrees` says.
pub fn thumbnail_rgba(
    source: &[u8],
    stride: usize,
    (width, height): (usize, usize),
    rotation_degrees: i32,
    (out_width, out_height): (usize, usize),
) -> Vec<u8> {
    let rotation = rotation_degrees.rem_euclid(360);
    let (shown_width, shown_height) = if rotation % 180 == 90 {
        (height, width)
    } else {
        (width, height)
    };
    let mut out = Vec::with_capacity(out_width * out_height * 4);
    for y in 0..out_height {
        let shown_y = y * shown_height / out_height;
        for x in 0..out_width {
            let shown_x = x * shown_width / out_width;
            let (from_x, from_y) = match rotation {
                90 => (shown_y, height - 1 - shown_x),
                180 => (width - 1 - shown_x, height - 1 - shown_y),
                270 => (width - 1 - shown_y, shown_x),
                _ => (shown_x, shown_y),
            };
            let at = from_y * stride + from_x * 4;
            match source.get(at..at + 4) {
                Some(pixel) => out.extend([pixel[2], pixel[1], pixel[0], 255]),
                None => out.extend([0, 0, 0, 255]),
            }
        }
    }
    out
}

pub trait DecodedFrame {
    fn pts_us(&self) -> i64;
}

pub trait VideoDecoder {
    type Frame: DecodedFrame;

    /// Next frame in presentation order, or `None` at the end.
    fn next_frame(&mut self) -> Result<Option<Self::Frame>, MediaError>;

    /// Moves so that the next frame is the first one at or after `time_us`.
    fn seek(&mut self, time_us: i64) -> Result<(), MediaError>;
}

pub trait MediaBackend {
    type Decoder: VideoDecoder;

    fn probe(&self, path: &Path) -> Result<MediaInfo, MediaError>;
    fn open_decoder(&self, path: &Path) -> Result<Self::Decoder, MediaError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_swap_channels_and_follow_rotation() {
        // Two pixels side by side, blue then red, in BGRA.
        let source = [255, 0, 0, 0, 0, 0, 255, 0];
        let flat = thumbnail_rgba(&source, 8, (2, 1), 0, (2, 1));
        assert_eq!(flat, [0, 0, 255, 255, 255, 0, 0, 255]);
        // Turned clockwise, the left pixel ends on top.
        let turned = thumbnail_rgba(&source, 8, (2, 1), 90, (1, 2));
        assert_eq!(turned, [0, 0, 255, 255, 255, 0, 0, 255]);
        let back = thumbnail_rgba(&source, 8, (2, 1), -90, (1, 2));
        assert_eq!(back, [255, 0, 0, 255, 0, 0, 255, 255]);
        let upside_down = thumbnail_rgba(&source, 8, (2, 1), 180, (2, 1));
        assert_eq!(upside_down, [255, 0, 0, 255, 0, 0, 255, 255]);
    }

    #[test]
    fn display_size_applies_rotation() {
        let mut info = MediaInfo {
            width: 1920,
            height: 1080,
            rotation_degrees: 90,
            duration_us: 0,
            nominal_fps: 60.0,
        };
        assert_eq!(info.display_size(), (1080, 1920));
        info.rotation_degrees = 180;
        assert_eq!(info.display_size(), (1920, 1080));
        info.rotation_degrees = -90;
        assert_eq!(info.display_size(), (1080, 1920));
    }

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
