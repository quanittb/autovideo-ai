use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::ai::frame_pipeline::reconstruct::RationalFps;
use crate::error::AppError;

/// Paths to extracted control artifacts on disk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ControlArtifactPaths {
    pub pose_frames_dir: Option<PathBuf>,
    pub depth_frames_dir: Option<PathBuf>,
    pub mask_frames_dir: Option<PathBuf>,
    pub audio_file_path: Option<PathBuf>,
}

/// Immutable, versioned Control Package representing extracted conditioning signals for a source video.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoControlPackage {
    pub job_id: String,
    pub source_video_path: String,
    pub source_video_hash: String,
    pub width: u32,
    pub height: u32,
    pub fps: RationalFps,
    pub total_frames: usize,
    pub duration_ms: u64,
    pub artifacts: ControlArtifactPaths,
    pub pose_hash: Option<String>,
    pub depth_hash: Option<String>,
    pub mask_hash: Option<String>,
    pub audio_hash: Option<String>,
    pub package_hash: String,
    pub is_valid: bool,
    pub created_at: String,
    pub schema_version: u32,
}

/// Execution telemetry and timing metrics for control extraction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ControlExtractionReport {
    pub job_id: String,
    pub total_frames: usize,
    pub pose_extracted_count: usize,
    pub depth_extracted_count: usize,
    pub mask_extracted_count: usize,
    pub pose_duration_ms: f64,
    pub depth_duration_ms: f64,
    pub mask_duration_ms: f64,
    pub total_duration_ms: f64,
    pub cache_hits_count: usize,
    pub package_hash: String,
    pub is_valid: bool,
    pub errors: Vec<String>,
}

impl VideoControlPackage {
    /// Computes deterministic composite SHA-256 package hash over all constituent signals.
    pub fn compute_package_hash(
        source_video_hash: &str,
        pose_hash: Option<&str>,
        depth_hash: Option<&str>,
        mask_hash: Option<&str>,
        audio_hash: Option<&str>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(source_video_hash.as_bytes());
        if let Some(h) = pose_hash {
            hasher.update(b":pose:");
            hasher.update(h.as_bytes());
        }
        if let Some(h) = depth_hash {
            hasher.update(b":depth:");
            hasher.update(h.as_bytes());
        }
        if let Some(h) = mask_hash {
            hasher.update(b":mask:");
            hasher.update(h.as_bytes());
        }
        if let Some(h) = audio_hash {
            hasher.update(b":audio:");
            hasher.update(h.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    /// Creates a new VideoControlPackage instance.
    pub fn new(
        job_id: impl Into<String>,
        source_video_path: impl Into<String>,
        source_video_hash: impl Into<String>,
        width: u32,
        height: u32,
        fps: RationalFps,
        total_frames: usize,
        duration_ms: u64,
        artifacts: ControlArtifactPaths,
        pose_hash: Option<String>,
        depth_hash: Option<String>,
        mask_hash: Option<String>,
        audio_hash: Option<String>,
    ) -> Self {
        let src_hash = source_video_hash.into();
        let package_hash = Self::compute_package_hash(
            &src_hash,
            pose_hash.as_deref(),
            depth_hash.as_deref(),
            mask_hash.as_deref(),
            audio_hash.as_deref(),
        );

        Self {
            job_id: job_id.into(),
            source_video_path: source_video_path.into(),
            source_video_hash: src_hash,
            width,
            height,
            fps,
            total_frames,
            duration_ms,
            artifacts,
            pose_hash,
            depth_hash,
            mask_hash,
            audio_hash,
            package_hash,
            is_valid: true,
            created_at: Utc::now().to_rfc3339(),
            schema_version: 1,
        }
    }

    /// Saves package manifest JSON to disk.
    pub fn save_to_file(&self, path: &Path) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::storage_error(
                    format!("Failed to create directory: {}", parent.display()),
                    e.to_string(),
                )
            })?;
        }

        let json = serde_json::to_string_pretty(self).map_err(|e| {
            AppError::storage_error("Failed to serialize VideoControlPackage", e.to_string())
        })?;

        let mut file = File::create(path).map_err(|e| {
            AppError::storage_error(
                format!("Failed to create manifest file: {}", path.display()),
                e.to_string(),
            )
        })?;

        file.write_all(json.as_bytes()).map_err(|e| {
            AppError::storage_error(
                format!("Failed to write manifest file: {}", path.display()),
                e.to_string(),
            )
        })?;

        Ok(())
    }

    /// Loads package manifest JSON from disk.
    pub fn load_from_file(path: &Path) -> Result<Self, AppError> {
        if !path.exists() {
            return Err(AppError::file_not_found(path.display().to_string()));
        }

        let json = fs::read_to_string(path).map_err(|e| {
            AppError::storage_error(
                format!("Failed to read manifest file: {}", path.display()),
                e.to_string(),
            )
        })?;

        let package: Self = serde_json::from_str(&json).map_err(|e| {
            AppError::storage_error("Failed to deserialize VideoControlPackage", e.to_string())
        })?;

        Ok(package)
    }

    /// Validates that all declared artifact files exist and are non-empty.
    pub fn validate_artifacts(&self) -> Result<(), AppError> {
        if let Some(ref pose_dir) = self.artifacts.pose_frames_dir {
            if !pose_dir.exists() {
                return Err(AppError::file_not_found(format!(
                    "Pose artifact directory not found: {}",
                    pose_dir.display()
                )));
            }
        }

        if let Some(ref depth_dir) = self.artifacts.depth_frames_dir {
            if !depth_dir.exists() {
                return Err(AppError::file_not_found(format!(
                    "Depth artifact directory not found: {}",
                    depth_dir.display()
                )));
            }
        }

        if let Some(ref mask_dir) = self.artifacts.mask_frames_dir {
            if !mask_dir.exists() {
                return Err(AppError::file_not_found(format!(
                    "Mask artifact directory not found: {}",
                    mask_dir.display()
                )));
            }
        }

        if let Some(ref audio_path) = self.artifacts.audio_file_path {
            if !audio_path.exists() {
                return Err(AppError::file_not_found(format!(
                    "Audio artifact file not found: {}",
                    audio_path.display()
                )));
            }
        }

        Ok(())
    }
}
