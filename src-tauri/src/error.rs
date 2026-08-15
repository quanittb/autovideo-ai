use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    FileNotFound,
    UnsupportedMedia,
    ModelNotAvailable,
    RuntimeNotAvailable,
    InsufficientResources,
    ProcessFailed,
    Cancelled,
    UnknownError,
}

#[derive(Debug, Error, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Option<String>,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(code: ErrorCode, message: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Some(details.into()),
        }
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, msg)
    }

    pub fn file_not_found(path: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::FileNotFound, "File not found", path)
    }

    pub fn unsupported_media(format: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::UnsupportedMedia, "Unsupported video format", format)
    }

    pub fn model_not_available(model: impl Into<String>, guidance: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::ModelNotAvailable,
            format!("Model '{}' is not available", model.into()),
            guidance,
        )
    }

    pub fn runtime_not_available(runtime: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::RuntimeNotAvailable,
            format!("Inference runtime '{}' is not available", runtime.into()),
            "Check GPU hardware drivers or use cloud adapter",
        )
    }

    pub fn cancelled() -> Self {
        Self::new(ErrorCode::Cancelled, "Operation was cancelled by user")
    }

    pub fn process_failed(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::ProcessFailed, msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_serialization() {
        let err = AppError::model_not_available("FoxDiffusion", "Please download weights");
        assert_eq!(err.code, ErrorCode::ModelNotAvailable);
        let serialized = serde_json::to_string(&err).expect("Failed to serialize");
        assert!(serialized.contains("MODEL_NOT_AVAILABLE"));
        assert!(serialized.contains("Please download weights"));
    }

    #[test]
    fn test_error_display() {
        let err = AppError::invalid_input("Prompt cannot be empty");
        assert_eq!(format!("{}", err), "InvalidInput: Prompt cannot be empty");
    }
}
