use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::error::AppError;

/// Character reference conditioning specifying identity, appearance, and crop mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CharacterReference {
    pub image_paths: Vec<PathBuf>,
    pub identity_weight: f32,
    pub appearance_weight: f32,
    pub crop_mode: String,
}

impl Default for CharacterReference {
    fn default() -> Self {
        Self {
            image_paths: Vec::new(),
            identity_weight: 0.85,
            appearance_weight: 0.75,
            crop_mode: "FACE_AND_UPPER_BODY".to_string(),
        }
    }
}

/// Environment and scene conditioning specifying prompts and cinematic style presets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCondition {
    pub positive_prompt: String,
    pub negative_prompt: String,
    pub style_preset: String,
}

impl Default for EnvironmentCondition {
    fn default() -> Self {
        Self {
            positive_prompt: "Cinematic, photorealistic 8k, natural lighting, high detail"
                .to_string(),
            negative_prompt: "blurry, low quality, distorted, bad anatomy, deformed hands"
                .to_string(),
            style_preset: "CINEMATIC".to_string(),
        }
    }
}

/// Fine-grained generation hyperparameters for diffusion sampling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenerationParams {
    pub steps: u32,
    pub cfg_scale: f32,
    pub denoise_strength: f32,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub control_weights: HashMap<String, f32>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        let mut control_weights = HashMap::new();
        control_weights.insert("pose".to_string(), 0.85);
        control_weights.insert("depth".to_string(), 0.75);
        control_weights.insert("mask".to_string(), 0.50);

        Self {
            steps: 25,
            cfg_scale: 7.0,
            denoise_strength: 0.85,
            seed: 42,
            width: 512,
            height: 768,
            control_weights,
        }
    }
}

/// Request structure for generating a single AI keyframe preview.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyframeGenerationRequest {
    pub job_id: String,
    pub source_frame_path: PathBuf,
    pub pose_artifact_path: Option<PathBuf>,
    pub depth_artifact_path: Option<PathBuf>,
    pub mask_artifact_path: Option<PathBuf>,
    pub character_reference: CharacterReference,
    pub environment: EnvironmentCondition,
    pub params: GenerationParams,
    pub output_path: PathBuf,
}

/// Comprehensive result payload returned after generative keyframe inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyframeGenerationResult {
    pub job_id: String,
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub load_duration_ms: f64,
    pub inference_duration_ms: f64,
    pub total_duration_ms: f64,
    pub vram_peak_bytes: Option<u64>,
    pub model_id: String,
    pub model_version: Option<String>,
    pub model_hash: Option<String>,
    pub backend: String,
    pub provider: String,
    pub is_production: bool,
    pub parameters: GenerationParams,
}

/// Real-time generation progress telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GenerativeProgress {
    pub job_id: String,
    pub active_step: u32,
    pub total_steps: u32,
    pub percent: f32,
    pub status: String,
}

/// Health and runtime diagnostics for the generative backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackendHealthStatus {
    pub healthy: bool,
    pub backend_name: String,
    pub version: String,
    pub cuda_available: bool,
    pub gpu_name: Option<String>,
    pub vram_total_mb: Option<u64>,
    pub vram_free_mb: Option<u64>,
    pub error: Option<String>,
}

/// Feature capabilities supported by the active generative backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackendCapabilities {
    pub supported_resolutions: Vec<[u32; 2]>,
    pub supports_character_reference: bool,
    pub supports_depth_control: bool,
    pub supports_pose_control: bool,
    pub supports_mask_control: bool,
    pub supports_fp8: bool,
    pub supports_lora: bool,
    pub backend_name: String,
}

/// Request structure for generating a sliding-window batch of frames.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoBatchGenerationRequest {
    pub job_id: String,
    pub window_index: usize,
    pub start_frame: usize,
    pub frame_count: usize,
    pub source_frame_paths: Vec<PathBuf>,
    pub pose_artifact_paths: Vec<PathBuf>,
    pub depth_artifact_paths: Vec<PathBuf>,
    pub mask_artifact_paths: Vec<PathBuf>,
    pub character_reference: CharacterReference,
    pub environment: EnvironmentCondition,
    pub params: GenerationParams,
    pub output_dir: PathBuf,
    pub latent_context_path: Option<PathBuf>,
}

/// Result payload returned after generating a temporal sliding-window batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoBatchGenerationResult {
    pub job_id: String,
    pub window_index: usize,
    pub output_frame_paths: Vec<PathBuf>,
    pub frame_count: usize,
    pub width: u32,
    pub height: u32,
    pub inference_duration_ms: f64,
    pub total_duration_ms: f64,
    pub latent_tail_path: Option<PathBuf>,
    pub is_production: bool,
}

/// Request for full video generation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoGenerationRequest {
    pub job_id: String,
    pub control_package_path: PathBuf,
    pub character_reference: CharacterReference,
    pub environment: EnvironmentCondition,
    pub params: GenerationParams,
    pub output_dir: PathBuf,
}

/// Result for full video generation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoGenerationResult {
    pub job_id: String,
    pub output_frames_dir: PathBuf,
    pub total_frames: usize,
    pub duration_seconds: f64,
    pub effective_fps: f64,
}

/// Stable abstraction decoupling Rust/Tauri from generative diffusion engine backends.
pub trait GenerativeBackend: Send + Sync {
    /// Generates a single transformed keyframe.
    fn generate_keyframe(
        &self,
        request: &KeyframeGenerationRequest,
        cancel_token: Option<Arc<AtomicBool>>,
    ) -> Result<KeyframeGenerationResult, AppError>;

    /// Generates a single temporal sliding-window multi-frame batch.
    fn generate_video_batch(
        &self,
        request: &VideoBatchGenerationRequest,
        cancel_token: Option<Arc<AtomicBool>>,
    ) -> Result<VideoBatchGenerationResult, AppError>;

    /// Generates full multi-frame video sequence.
    fn generate_video(
        &self,
        request: &VideoGenerationRequest,
        cancel_token: Option<Arc<AtomicBool>>,
    ) -> Result<VideoGenerationResult, AppError>;

    /// Cancels an in-flight generative job.
    fn cancel_job(&self, job_id: &str) -> Result<(), AppError>;

    /// Queries live progress of a generative job.
    fn get_progress(&self, job_id: &str) -> Result<GenerativeProgress, AppError>;

    /// Performs preflight health check on the backend runtime and CUDA status.
    fn health_check(&self) -> Result<BackendHealthStatus, AppError>;

    /// Reports hardware and algorithmic capabilities of the backend.
    fn get_capabilities(&self) -> Result<BackendCapabilities, AppError>;
}
