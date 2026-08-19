use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::error::AppError;

/// Authoritative end-to-end production execution report for an AI video processing job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiProductionExecutionReport {
    pub job_id: String,

    // Model Pinned Metadata
    pub model_id: String,
    pub model_version: Option<String>,
    pub model_hash: Option<String>,
    pub profile_hash: Option<String>,
    #[serde(default)]
    pub is_production: bool,
    pub provider: String,

    // Source Specifications
    pub source_duration_ms: u64,
    pub source_width: u32,
    pub source_height: u32,
    pub source_fps: f64,
    pub source_total_frames: usize,

    // Execution Statistics
    pub selected_frames: usize,
    pub processed_frames: usize,
    pub reused_frames: usize,
    pub passthrough_frames: usize,
    pub failed_frames: usize,

    // Timing Breakdown (milliseconds)
    pub preprocessing_ms: f64,
    pub inference_ms: f64,
    pub postprocessing_ms: f64,
    pub reconstruction_ms: f64,
    pub validation_ms: f64,
    pub total_ms: f64,

    // Storage Breakdown
    pub artifacts_written: usize,
    pub bytes_written: u64,

    // Quality Results
    pub valid_frames: usize,
    pub invalid_frames: usize,
    pub quality_warnings: usize,

    // Result Specifications
    pub output_path: Option<String>,
    pub output_size_bytes: Option<u64>,
    pub output_duration_ms: Option<u64>,
    pub output_fps: Option<f64>,
    pub output_width: Option<u32>,
    pub output_height: Option<u32>,
    pub audio_preserved: bool,
    pub validation_status: String,

    // Overall Status
    pub status: String,
    pub created_at: String,
}

impl AiProductionExecutionReport {
    /// Creates a report initialized with execution start details.
    pub fn new(
        job_id: &str,
        model_id: &str,
        model_version: Option<&str>,
        model_hash: Option<&str>,
        profile_hash: Option<&str>,
        provider: &str,
        source_width: u32,
        source_height: u32,
        source_fps: f64,
        source_duration_ms: u64,
        source_total_frames: usize,
    ) -> Self {
        Self {
            job_id: job_id.to_string(),
            model_id: model_id.to_string(),
            model_version: model_version.map(|v| v.to_string()),
            model_hash: model_hash.map(|h| h.to_string()),
            profile_hash: profile_hash.map(|h| h.to_string()),
            is_production: false,
            provider: provider.to_string(),
            source_duration_ms,
            source_width,
            source_height,
            source_fps,
            source_total_frames,
            selected_frames: 0,
            processed_frames: 0,
            reused_frames: 0,
            passthrough_frames: 0,
            failed_frames: 0,
            preprocessing_ms: 0.0,
            inference_ms: 0.0,
            postprocessing_ms: 0.0,
            reconstruction_ms: 0.0,
            validation_ms: 0.0,
            total_ms: 0.0,
            artifacts_written: 0,
            bytes_written: 0,
            valid_frames: 0,
            invalid_frames: 0,
            quality_warnings: 0,
            output_path: None,
            output_size_bytes: None,
            output_duration_ms: None,
            output_fps: None,
            output_width: None,
            output_height: None,
            audio_preserved: false,
            validation_status: "PENDING".to_string(),
            status: "RUNNING".to_string(),
            created_at: Utc::now().to_rfc3339(),
        }
    }

    pub fn with_production(mut self, is_production: bool) -> Self {
        self.is_production = is_production;
        self
    }

    /// Persists report to disk as JSON.
    pub fn save_to_file(&self, path: &Path) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            AppError::storage_error("Failed to serialize execution report", e.to_string())
        })?;
        fs::write(path, json).map_err(|e| {
            AppError::storage_error("Failed to write execution report file", e.to_string())
        })?;
        Ok(())
    }

    /// Loads report from disk JSON.
    pub fn load_from_file(path: &Path) -> Result<Self, AppError> {
        if !path.exists() {
            return Err(AppError::file_not_found(path.display().to_string()));
        }
        let content = fs::read_to_string(path).map_err(|e| {
            AppError::storage_error("Failed to read execution report file", e.to_string())
        })?;
        let report = serde_json::from_str(&content).map_err(|e| {
            AppError::storage_error("Failed to parse execution report file", e.to_string())
        })?;
        Ok(report)
    }
}
