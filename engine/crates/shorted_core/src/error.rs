//! Typed errors for loading and validating project documents.

use thiserror::Error;

use crate::model::Id;

#[derive(Debug, Error, PartialEq)]
pub enum ModelError {
    #[error("the document is larger than {limit} bytes")]
    DocumentTooLarge { limit: usize },
    #[error("the document is not a valid project")]
    Malformed(String),
    #[error("schema version {found} is newer than this app supports")]
    NewerSchema { found: u32 },
    #[error("schema version {found} needs a migration")]
    OlderSchema { found: u32 },
    #[error("too many {what}: {found}, the limit is {limit}")]
    TooMany {
        what: &'static str,
        found: usize,
        limit: usize,
    },
    #[error("{what} is not valid")]
    Invalid { what: &'static str },
    #[error("duplicate id {0}")]
    DuplicateId(Id),
    #[error("unknown id {0}")]
    UnknownId(Id),
    #[error("position {index} is out of range")]
    OutOfRange { index: usize },
    #[error("no gesture is in progress")]
    NoGesture,
}
