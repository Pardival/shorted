//! The only crate exposed to Dart: coarse-grained API, async functions, event streams.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod api;
pub mod error;
#[allow(clippy::unwrap_used, clippy::expect_used)] // Generated code.
mod frb_generated;
pub mod logging;
#[cfg(target_vendor = "apple")]
pub mod preview_session;
pub mod runtime;
pub mod spike;
mod transport;

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_ffi";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }

    #[test]
    fn links_core() {
        assert_eq!(shorted_core::CRATE_NAME, "shorted_core");
    }

    #[test]
    fn links_store() {
        assert_eq!(shorted_store::CRATE_NAME, "shorted_store");
    }

    #[test]
    fn links_media() {
        assert_eq!(shorted_media::CRATE_NAME, "shorted_media");
    }

    #[test]
    fn links_render() {
        assert_eq!(shorted_render::CRATE_NAME, "shorted_render");
    }

    #[test]
    fn links_audio() {
        assert_eq!(shorted_audio::CRATE_NAME, "shorted_audio");
    }

    #[test]
    fn links_playback() {
        assert_eq!(shorted_playback::CRATE_NAME, "shorted_playback");
    }

    #[test]
    fn links_transcribe() {
        assert_eq!(shorted_transcribe::CRATE_NAME, "shorted_transcribe");
    }

    #[test]
    fn links_analysis() {
        assert_eq!(shorted_analysis::CRATE_NAME, "shorted_analysis");
    }

    #[test]
    fn links_ai() {
        assert_eq!(shorted_ai::CRATE_NAME, "shorted_ai");
    }
}
