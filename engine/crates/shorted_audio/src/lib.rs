//! Audio decode, mix, ducking, waveform peaks, output and microphone capture.

#![deny(unsafe_op_in_unsafe_fn)]

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_audio";

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
    fn links_media() {
        assert_eq!(shorted_media::CRATE_NAME, "shorted_media");
    }
}
