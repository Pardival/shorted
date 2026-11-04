//! AiProvider trait, providers and validated EditPlan parsing.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod plan;
pub mod prompt;
pub mod providers;
pub mod speech;

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_ai";

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
}
