//! wgpu compositor shared by preview and export.

#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_vendor = "apple")]
pub mod apple;
pub mod spike;

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_render";

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
