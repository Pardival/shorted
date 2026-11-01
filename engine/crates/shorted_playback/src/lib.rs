//! Audio-driven master clock, frame scheduling, prefetch and scrubbing.

#![deny(unsafe_op_in_unsafe_fn)]

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_playback";

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
    fn links_render() {
        assert_eq!(shorted_render::CRATE_NAME, "shorted_render");
    }

    #[test]
    fn links_audio() {
        assert_eq!(shorted_audio::CRATE_NAME, "shorted_audio");
    }
}
