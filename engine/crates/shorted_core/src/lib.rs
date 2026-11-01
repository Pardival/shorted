//! Project model, commands, undo stack and timeline evaluation. Pure: no I/O, no threads, no GPU.

#![deny(unsafe_op_in_unsafe_fn)]

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_core";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CRATE_NAME, env!("CARGO_PKG_NAME"));
    }
}
