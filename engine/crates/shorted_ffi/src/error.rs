//! Engine errors. Dart only ever receives a stable [`ErrorCode`], never a message: messages may
//! hold paths or user content.

use shorted_ai::providers::AiError;
use shorted_store::bundle::BundleError;
use shorted_store::session::StoreError;
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
    #[error("the file cannot be used")]
    UnsupportedMedia,
    #[error("the chosen file is not the missing one")]
    DifferentMedia,
    #[error("the video could not be written")]
    ExportFailed,
    #[error("the file is beyond the size limits")]
    TooLarge,
    #[error("no AI provider is set up")]
    AiNotConfigured,
    #[error("the key could not be stored safely")]
    KeyStorage,
    #[error("the AI provider failed")]
    Ai(shorted_ai::providers::AiError),
    #[error("the bundle could not be used")]
    Bundle(#[from] shorted_store::bundle::BundleError),
    #[error("project storage failed")]
    Store(#[from] shorted_store::session::StoreError),
}

/// User-safe error code. The app maps each code to a localised message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
    Cancelled,
    Storage,
    Internal,
    /// Saved by a newer version of the app.
    NewerProject,
    /// The project file cannot be read.
    DamagedProject,
    /// An edit was refused: invalid values, or nothing is open.
    InvalidEdit,
    /// The file type is not supported, or the file cannot be decoded.
    UnsupportedMedia,
    /// A relink pointed at a different file.
    DifferentMedia,
    /// The video could not be written.
    ExportFailed,
    /// The file is larger than the app accepts (docs/08).
    TooLarge,
    /// No AI provider, or no key for it.
    AiNotConfigured,
    /// The system has no safe place for the key here.
    KeyStorage,
    /// The provider refused the key (401, 403).
    AiKeyRejected,
    /// Too many requests, or no credit left (429).
    AiRateLimited,
    /// The provider is having trouble (5xx).
    AiUnavailable,
    /// No answer: offline, wrong address, or too slow.
    AiUnreachable,
    /// The provider answered something unusable.
    AiBadResponse,
    /// A `.shorted` file that is damaged, unsafe, or not a project.
    BadBundle,
    /// A media file of the project cannot be found.
    MissingMedia,
}

impl EngineError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Cancelled => ErrorCode::Cancelled,
            Self::Io(_) => ErrorCode::Storage,
            Self::Logging(_) | Self::Internal(_) => ErrorCode::Internal,
            Self::UnsupportedMedia => ErrorCode::UnsupportedMedia,
            Self::DifferentMedia => ErrorCode::DifferentMedia,
            Self::ExportFailed => ErrorCode::ExportFailed,
            Self::TooLarge => ErrorCode::TooLarge,
            Self::AiNotConfigured => ErrorCode::AiNotConfigured,
            Self::Bundle(error) => match error {
                BundleError::Io(_) => ErrorCode::Storage,
                BundleError::TooLarge => ErrorCode::TooLarge,
                BundleError::MissingMedia => ErrorCode::MissingMedia,
                BundleError::Unsafe(_) | BundleError::Mismatch | BundleError::Malformed => {
                    ErrorCode::BadBundle
                }
            },
            Self::KeyStorage => ErrorCode::KeyStorage,
            Self::Ai(error) => match error {
                AiError::KeyRejected => ErrorCode::AiKeyRejected,
                AiError::RateLimited => ErrorCode::AiRateLimited,
                AiError::Unavailable => ErrorCode::AiUnavailable,
                AiError::Network => ErrorCode::AiUnreachable,
                AiError::BadResponse(_) => ErrorCode::AiBadResponse,
            },
            Self::Store(error) => match error {
                StoreError::Io(_) | StoreError::WriterStopped => ErrorCode::Storage,
                StoreError::NewerVersion { .. } => ErrorCode::NewerProject,
                StoreError::Malformed => ErrorCode::DamagedProject,
                StoreError::Model(_) => ErrorCode::InvalidEdit,
            },
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
