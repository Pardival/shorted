//! MediaBackend trait and per-platform decode, encode, mux, probe and proxies.

#![deny(unsafe_op_in_unsafe_fn)]

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_media";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
