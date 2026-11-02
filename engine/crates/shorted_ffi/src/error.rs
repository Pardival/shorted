//! Engine errors. Dart only ever receives a stable [`ErrorCode`], never a message: messages may
//! hold paths or user content.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("the operation was cancelled")]
    Cancelled,
    #[error("I/O failure")]
    Io(#[from] std::io::Error),
    #[error("logging could not be initialised")]
    Logging(String),
    #[error("internal failure")]
    Internal(String),
}

/// User-safe error code. The app maps each code to a localised message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    Cancelled,
    Storage,
    Internal,
}

impl EngineError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Cancelled => ErrorCode::Cancelled,
            Self::Io(_) => ErrorCode::Storage,
            Self::Logging(_) | Self::Internal(_) => ErrorCode::Internal,
        }
    }
}

/// Error type crossing the bridge; becomes a Dart exception holding only the code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineFailure {
    pub code: ErrorCode,
}

impl From<EngineError> for EngineFailure {
    fn from(error: EngineError) -> Self {
        tracing::error!(%error, "engine operation failed");
        Self { code: error.code() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_carries_only_the_code() {
        let error = EngineError::Internal("/private/path/clip.mov".into());
        let failure = EngineFailure::from(error);
        assert_eq!(
            failure,
            EngineFailure {
                code: ErrorCode::Internal
            }
        );
        assert!(!format!("{failure:?}").contains("clip.mov"));
    }

    #[test]
    fn io_errors_map_to_storage() {
        let error = EngineError::from(std::io::Error::other("disk"));
        assert_eq!(error.code(), ErrorCode::Storage);
    }
}
