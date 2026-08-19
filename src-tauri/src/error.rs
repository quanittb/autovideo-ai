use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidInput,
    FileNotFound,
    UnsupportedMedia,
    ModelNotAvailable,
    ModelIntegrityMismatch,
    ModelProfileMismatch,
    ModelVersionExists,
    ModelValidationFailed,
    ModelNotFound,
    ModelVersionNotFound,
    ModelNotActive,
    ModelProviderUnsupported,
    ProviderUnavailable,
    ModelGraphInvalid,
    PreflightFailed,
    AiJobConfigurationInvalid,
    ResourceLimitExceeded,
    FrameQualityFailed,
    DiskQuotaExceeded,
    RuntimeNotAvailable,
    InsufficientResources,
    ProcessFailed,
    Cancelled,
    ProjectNotFound,
    ProjectCreateFailed,
    ProjectLoadFailed,
    ProjectSaveFailed,
    ProjectDeleteFailed,
    MediaFileNotFound,
    MediaUnsupportedFormat,
    MediaTooLarge,
    MediaInvalid,
    MediaMetadataFailed,
    MediaImportFailed,
    FfmpegNotAvailable,
    FfprobeNotAvailable,
    MediaProbeFailed,
    FrameExtractionFailed,
    AudioExtractionFailed,
    NoAudioStream,
    MediaCacheFailed,
    MediaProcessCancelled,
    RenderFailed,
    RenderCancelled,
    OutputInvalid,
    OutputNotFound,
    OutputMetadataFailed,
    AudioMuxFailed,
    FrameSequenceInvalid,
    JobNotFound,
    JobFailed,
    JobCancelled,
    JobInterrupted,
    StorageError,
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

    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
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
        Self::with_details(
            ErrorCode::UnsupportedMedia,
            "Unsupported video format",
            format,
        )
    }

    pub fn model_not_available(model: impl Into<String>, guidance: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::ModelNotAvailable,
            format!("Model '{}' is not available", model.into()),
            guidance,
        )
    }

    pub fn model_integrity_mismatch(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::ModelIntegrityMismatch, msg, details)
    }

    pub fn model_profile_mismatch(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::ModelProfileMismatch, msg, details)
    }

    pub fn model_version_exists(model_id: impl Into<String>, version: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::ModelVersionExists,
            format!(
                "Model '{}' version '{}' already exists in registry",
                model_id.into(),
                version.into()
            ),
            "Specify a new version or use update operation",
        )
    }

    pub fn model_validation_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::ModelValidationFailed, msg, details)
    }

    pub fn model_not_found(model_id: impl Into<String>) -> Self {
        let id = model_id.into();
        Self::with_details(
            ErrorCode::ModelNotFound,
            format!("AI model '{}' was not found in registry", id),
            "Verify the model is installed in the local model directory",
        )
    }

    pub fn model_version_not_found(
        model_id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        let id = model_id.into();
        let ver = version.into();
        Self::with_details(
            ErrorCode::ModelVersionNotFound,
            format!("Version '{}' of model '{}' was not found", ver, id),
            "Check available model versions in the model registry",
        )
    }

    pub fn model_not_active(model_id: impl Into<String>) -> Self {
        let id = model_id.into();
        Self::with_details(
            ErrorCode::ModelNotActive,
            format!("Model '{}' does not have an active production version", id),
            "Activate a valid version in the Model Registry before creating jobs",
        )
    }

    pub fn model_provider_unsupported(
        model_id: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        let id = model_id.into();
        let prov = provider.into();
        Self::with_details(
            ErrorCode::ModelProviderUnsupported,
            format!(
                "Execution provider '{}' is not supported by model '{}'",
                prov, id
            ),
            "Choose a provider supported by this model package",
        )
    }

    pub fn provider_unavailable(provider: impl Into<String>, reason: impl Into<String>) -> Self {
        let prov = provider.into();
        Self::with_details(
            ErrorCode::ProviderUnavailable,
            format!(
                "Hardware execution provider '{}' is not available on host",
                prov
            ),
            reason,
        )
    }

    pub fn model_graph_invalid(model_id: impl Into<String>, details: impl Into<String>) -> Self {
        let id = model_id.into();
        Self::with_details(
            ErrorCode::ModelGraphInvalid,
            format!(
                "ONNX computation graph for model '{}' is invalid or corrupt",
                id
            ),
            details,
        )
    }

    pub fn preflight_failed(summary: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::PreflightFailed, summary, details)
    }

    pub fn ai_job_configuration_invalid(
        msg: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self::with_details(ErrorCode::AiJobConfigurationInvalid, msg, details)
    }

    pub fn resource_limit_exceeded(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::ResourceLimitExceeded, msg, details)
    }

    pub fn frame_quality_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::FrameQualityFailed, msg, details)
    }

    pub fn disk_quota_exceeded(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::DiskQuotaExceeded, msg, details)
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

    pub fn project_not_found(id: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::ProjectNotFound, "Project not found", id)
    }

    pub fn project_create_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::ProjectCreateFailed, msg, details)
    }

    pub fn project_load_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::ProjectLoadFailed, msg, details)
    }

    pub fn project_save_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::ProjectSaveFailed, msg, details)
    }

    pub fn project_delete_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::ProjectDeleteFailed, msg, details)
    }

    pub fn media_file_not_found(path: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::MediaFileNotFound,
            "Source media file not found",
            path,
        )
    }

    pub fn media_unsupported_format(format: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::MediaUnsupportedFormat,
            "Unsupported video format (Accepted: MP4, MOV, AVI, MKV)",
            format,
        )
    }

    pub fn media_too_large(size_bytes: u64, max_bytes: u64) -> Self {
        Self::with_details(
            ErrorCode::MediaTooLarge,
            "Video file size exceeds maximum limit of 2 GB",
            format!(
                "File size: {} bytes, Maximum allowed: {} bytes",
                size_bytes, max_bytes
            ),
        )
    }

    pub fn media_invalid(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::MediaInvalid, msg, details)
    }

    pub fn media_metadata_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::MediaMetadataFailed, msg, details)
    }

    pub fn media_import_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::MediaImportFailed, msg, details)
    }

    pub fn ffmpeg_not_available(details: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::FfmpegNotAvailable,
            "FFmpeg executable was not found in system PATH",
            details,
        )
    }

    pub fn ffprobe_not_available(details: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::FfprobeNotAvailable,
            "FFprobe executable was not found in system PATH",
            details,
        )
    }

    pub fn media_probe_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::MediaProbeFailed, msg, details)
    }

    pub fn frame_extraction_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::FrameExtractionFailed, msg, details)
    }

    pub fn audio_extraction_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::AudioExtractionFailed, msg, details)
    }

    pub fn no_audio_stream(details: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::NoAudioStream,
            "Source video contains no audio stream",
            details,
        )
    }

    pub fn media_cache_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::MediaCacheFailed, msg, details)
    }

    pub fn media_process_cancelled(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::MediaProcessCancelled, msg)
    }

    pub fn render_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::RenderFailed, msg, details)
    }

    pub fn render_cancelled() -> Self {
        Self::new(
            ErrorCode::RenderCancelled,
            "Video render operation was cancelled",
        )
    }

    pub fn output_invalid(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::OutputInvalid, msg, details)
    }

    pub fn output_not_found(path: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::OutputNotFound,
            "Render output file was not found on disk",
            path,
        )
    }

    pub fn output_metadata_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::OutputMetadataFailed, msg, details)
    }

    pub fn audio_mux_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::AudioMuxFailed, msg, details)
    }

    pub fn frame_sequence_invalid(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::FrameSequenceInvalid, msg, details)
    }

    pub fn storage_error(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::StorageError, msg, details)
    }

    pub fn job_not_found(id: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::JobNotFound, "Job not found", id)
    }

    pub fn job_failed(msg: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(ErrorCode::JobFailed, msg, details)
    }

    pub fn job_cancelled() -> Self {
        Self::new(ErrorCode::JobCancelled, "Job was cancelled by user")
    }

    pub fn job_interrupted() -> Self {
        Self::new(
            ErrorCode::JobInterrupted,
            "Job was interrupted by system shutdown/restart",
        )
    }

    pub fn storage_write_failed(path: impl Into<String>, details: impl Into<String>) -> Self {
        Self::with_details(
            ErrorCode::StorageError,
            "Failed to write to persistent storage",
            format!("{}: {}", path.into(), details.into()),
        )
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

    #[test]
    fn test_project_errors() {
        let err = AppError::project_not_found("proj-123");
        assert_eq!(err.code, ErrorCode::ProjectNotFound);
        assert_eq!(err.details, Some("proj-123".to_string()));
    }

    #[test]
    fn test_media_and_render_errors() {
        let err = AppError::media_too_large(3_000_000_000, 2_147_483_648);
        assert_eq!(err.code, ErrorCode::MediaTooLarge);
        assert!(err.details.unwrap().contains("Maximum allowed"));

        let render_err = AppError::render_failed("Encoding failed", "Non-zero exit code");
        assert_eq!(render_err.code, ErrorCode::RenderFailed);
    }
}
