//! Stable, user-friendly error codes for the office suite engine.

use serde::{Deserialize, Serialize};

pub type OfficeResult<T> = Result<T, OfficeError>;

/// Error codes are part of the IPC contract and must stay stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidPath,
    IoError,
    NotFound,
    PermissionDenied,
    UnsupportedFormat,
    CorruptDocument,
    UnsupportedFeature,
    EncodingError,
    ZipBomb,
    TooLarge,
    Cancelled,
    InvalidArgument,
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidPath => "invalid_path",
            ErrorCode::IoError => "io_error",
            ErrorCode::NotFound => "not_found",
            ErrorCode::PermissionDenied => "permission_denied",
            ErrorCode::UnsupportedFormat => "unsupported_format",
            ErrorCode::CorruptDocument => "corrupt_document",
            ErrorCode::UnsupportedFeature => "unsupported_feature",
            ErrorCode::EncodingError => "encoding_error",
            ErrorCode::ZipBomb => "zip_bomb",
            ErrorCode::TooLarge => "too_large",
            ErrorCode::Cancelled => "cancelled",
            ErrorCode::InvalidArgument => "invalid_argument",
            ErrorCode::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficeError {
    pub code: String,
    pub message: String,
}

impl OfficeError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code: code.as_str().to_string(), message: message.into() }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::UnsupportedFormat, message)
    }

    pub fn corrupt(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::CorruptDocument, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorCode::Cancelled, "The operation was cancelled.")
    }

    pub fn from_io(error: std::io::Error, path: &std::path::Path) -> Self {
        use std::io::ErrorKind;
        let message = format!("{}: {}", path.display(), error);
        match error.kind() {
            ErrorKind::NotFound => Self::new(ErrorCode::NotFound, message),
            ErrorKind::PermissionDenied => Self::new(ErrorCode::PermissionDenied, message),
            _ => Self::new(ErrorCode::IoError, message),
        }
    }
}

impl std::fmt::Display for OfficeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for OfficeError {}

impl From<OfficeError> for String {
    fn from(value: OfficeError) -> Self {
        serde_json::to_string(&value).unwrap_or(value.message)
    }
}
