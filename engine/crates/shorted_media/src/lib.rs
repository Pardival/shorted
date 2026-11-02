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
