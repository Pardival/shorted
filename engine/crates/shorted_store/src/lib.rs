//! Project folder, autosave journal, snapshots, atomic writes and schema migrations.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod library;
pub mod media;
pub mod migrations;
pub mod session;

/// Name of this crate, used by the workspace smoke tests.
pub const CRATE_NAME: &str = "shorted_store";

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
