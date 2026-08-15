use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize, Clone)]
pub enum AiError {
    #[error("MODEL_NOT_AVAILABLE: {model_name} is not loaded or missing. Guidance: {guidance}")]
    ModelNotAvailable { model_name: String, guidance: String },

    #[error("MODEL_BLOCKED: {reason}")]
    ModelBlocked { reason: String },

    #[error("RUNTIME_NOT_AVAILABLE: {runtime_type} runtime unavailable on this platform/hardware.")]
    RuntimeNotAvailable { runtime_type: String },

    #[error("Execution failed: {message}")]
    ExecutionFailed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "type")]
pub enum AiAvailabilityStatus {
    Available,
    ModelNotAvailable { model_name: String, guidance: String },
    ModelBlocked { reason: String },
    RuntimeNotAvailable { runtime_type: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub is_downloaded: bool,
    pub is_loaded_in_vram: bool,
    pub size_bytes: u64,
}

// System Abstraction Traits

pub trait AnalysisEngine: Send + Sync {
    fn analyze_video(&self, video_path: &Path) -> Result<String, AiError>;
}

pub trait TransformationEngine: Send + Sync {
    fn plan_transformation(&self, prompt: &str) -> Result<String, AiError>;
}

pub trait CharacterTransformationEngine: Send + Sync {
    fn replace_character(&self, frames: &[PathBuf], prompt: &str) -> Result<Vec<PathBuf>, AiError>;
}

pub trait BackgroundTransformationEngine: Send + Sync {
    fn transform_background(&self, frames: &[PathBuf], prompt: &str) -> Result<Vec<PathBuf>, AiError>;
}

pub trait TemporalConsistencyEngine: Send + Sync {
    fn align_frames(&self, frames: &[PathBuf]) -> Result<Vec<PathBuf>, AiError>;
}

pub trait AudioEngine: Send + Sync {
    fn process_audio(&self, input_video: &Path, output_video: &Path) -> Result<(), AiError>;
}

pub trait InferenceRuntime: Send + Sync {
    fn runtime_name(&self) -> &'static str;
    fn is_available(&self) -> bool;
}

pub trait ModelProvider: Send + Sync {
    fn get_model_path(&self, model_name: &str) -> Result<PathBuf, AiError>;
}

pub trait ModelManager: Send + Sync {
    fn check_status(&self, model_name: &str) -> AiAvailabilityStatus;
    fn list_models(&self) -> Vec<ModelInfo>;
}
