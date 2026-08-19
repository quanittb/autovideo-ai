use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::ai::frame_pipeline::config::AiJobConfig;
use crate::ai::pipeline::postprocess::PostprocessConfig;
use crate::ai::pipeline::preprocess::PreprocessConfig;
use crate::error::AppError;

/// Computes SHA-256 checksum over byte slice.
pub fn calculate_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AiFrameStatus {
    #[default]
    Completed,
    Passthrough,
    Reused,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiFrameMetadata {
    #[serde(default)]
    pub job_id: Option<String>,
    pub frame_index: usize,
    #[serde(default)]
    pub source_frame_index: usize,
    pub status: AiFrameStatus,
    pub model_id: String,
    #[serde(default)]
    pub model_version: Option<String>,
    #[serde(default)]
    pub model_hash: Option<String>,
    #[serde(default)]
    pub profile_hash: Option<String>,
    pub provider: String,
    pub decode_duration_ms: f64,
    pub preprocess_duration_ms: f64,
    pub inference_duration_ms: f64,
    pub postprocess_duration_ms: f64,
    pub total_duration_ms: f64,
    pub input_width: u32,
    pub input_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub output_artifact_path: String,
    pub config_hash: String,
    #[serde(default)]
    pub artifact_hash: Option<String>,
    #[serde(default)]
    pub artifact_size_bytes: Option<u64>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiJobMetrics {
    pub frames_total: usize,
    pub frames_selected: usize,
    pub frames_processed: usize,
    pub frames_reused: usize,
    pub frames_passthrough: usize,
    pub frames_failed: usize,
    pub total_inference_duration_ms: f64,
    pub average_inference_duration_ms: f64,
    pub min_inference_duration_ms: f64,
    pub max_inference_duration_ms: f64,
    pub total_pipeline_duration_ms: f64,
    #[serde(default)]
    pub artifact_bytes_written: u64,
    #[serde(default)]
    pub eta_ms: Option<f64>,
}

impl Default for AiJobMetrics {
    fn default() -> Self {
        Self {
            frames_total: 0,
            frames_selected: 0,
            frames_processed: 0,
            frames_reused: 0,
            frames_passthrough: 0,
            frames_failed: 0,
            total_inference_duration_ms: 0.0,
            average_inference_duration_ms: 0.0,
            min_inference_duration_ms: 0.0,
            max_inference_duration_ms: 0.0,
            total_pipeline_duration_ms: 0.0,
            artifact_bytes_written: 0,
            eta_ms: None,
        }
    }
}

/// Computes a deterministic configuration hash for model + preprocessing + postprocessing.
pub fn compute_ai_config_hash(
    model_id: &str,
    preprocessing: &PreprocessConfig,
    postprocessing: Option<&PostprocessConfig>,
) -> String {
    let mut hasher = DefaultHasher::new();
    model_id.hash(&mut hasher);
    if let Ok(prep_json) = serde_json::to_string(preprocessing) {
        prep_json.hash(&mut hasher);
    }
    if let Some(post) = postprocessing {
        if let Ok(post_json) = serde_json::to_string(post) {
            post_json.hash(&mut hasher);
        }
    }
    format!("{:016x}", hasher.finish())
}

/// Computes a comprehensive, deterministic configuration hash for an entire AiJobConfig (pinning version & hashes).
pub fn compute_ai_job_config_hash(config: &AiJobConfig) -> String {
    let mut hasher = DefaultHasher::new();
    config.model_id.hash(&mut hasher);
    if let Some(ref ver) = config.model_version {
        ver.hash(&mut hasher);
    }
    if let Some(ref mhash) = config.model_hash {
        mhash.hash(&mut hasher);
    }
    if let Some(ref phash) = config.profile_hash {
        phash.hash(&mut hasher);
    }
    if let Ok(prep_json) = serde_json::to_string(&config.preprocessing) {
        prep_json.hash(&mut hasher);
    }
    if let Some(ref post) = config.postprocessing {
        if let Ok(post_json) = serde_json::to_string(post) {
            post_json.hash(&mut hasher);
        }
    }
    format!("{:016x}", hasher.finish())
}

#[derive(Debug, Clone)]
pub struct AiArtifactManager {
    pub base_dir: PathBuf,
}

impl AiArtifactManager {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    pub fn frame_dir(&self, frame_index: usize) -> PathBuf {
        self.base_dir.join(format!("{:06}", frame_index))
    }

    pub fn frame_output_png_path(&self, frame_index: usize) -> PathBuf {
        self.frame_dir(frame_index).join("output.png")
    }

    pub fn output_png_path(&self, frame_index: usize) -> PathBuf {
        self.frame_output_png_path(frame_index)
    }

    pub fn frame_result_json_path(&self, frame_index: usize) -> PathBuf {
        self.frame_dir(frame_index).join("result.json")
    }

    pub fn result_json_path(&self, frame_index: usize) -> PathBuf {
        self.frame_result_json_path(frame_index)
    }

    pub fn reconstruction_frames_dir(&self) -> PathBuf {
        self.base_dir.join("reconstruction_frames")
    }

    pub fn reconstruction_frame_path(&self, frame_index: usize) -> PathBuf {
        self.reconstruction_frames_dir()
            .join(format!("{:06}.png", frame_index))
    }

    pub fn ensure_dirs(&self) -> Result<(), AppError> {
        fs::create_dir_all(&self.base_dir).map_err(|e| {
            AppError::storage_error("Failed to create AI artifacts directory", e.to_string())
        })?;
        fs::create_dir_all(self.reconstruction_frames_dir()).map_err(|e| {
            AppError::storage_error(
                "Failed to create reconstruction frames directory",
                e.to_string(),
            )
        })?;
        Ok(())
    }

    /// Atomically persists an AI output frame artifact (PNG + JSON) and mirrors to reconstruction folder.
    pub fn write_frame_artifact(
        &self,
        meta: &AiFrameMetadata,
        image_png_bytes: &[u8],
    ) -> Result<u64, AppError> {
        let f_dir = self.frame_dir(meta.frame_index);
        fs::create_dir_all(&f_dir).map_err(|e| {
            AppError::storage_error("Failed to create frame artifact directory", e.to_string())
        })?;

        // 1. Write output PNG with atomic rename
        let out_png = self.frame_output_png_path(meta.frame_index);
        let tmp_png = f_dir.join(format!(".output.png.tmp-{}", Uuid::new_v4()));
        {
            let mut file = File::create(&tmp_png).map_err(|e| {
                AppError::storage_error("Failed to create temporary output frame", e.to_string())
            })?;
            file.write_all(image_png_bytes).map_err(|e| {
                AppError::storage_error("Failed to write temporary output frame", e.to_string())
            })?;
            file.sync_all().map_err(|e| {
                AppError::storage_error("Failed to sync temporary output frame", e.to_string())
            })?;
        }

        #[cfg(target_os = "windows")]
        {
            if out_png.exists() {
                let _ = fs::remove_file(&out_png);
            }
        }
        fs::rename(&tmp_png, &out_png).map_err(|e| {
            let _ = fs::remove_file(&tmp_png);
            AppError::storage_error("Failed to persist frame artifact PNG", e.to_string())
        })?;

        // 2. Mirror to reconstruction frames directory for video muxing
        let recon_dir = self.reconstruction_frames_dir();
        fs::create_dir_all(&recon_dir).map_err(|e| {
            AppError::storage_error(
                "Failed to create reconstruction frames directory",
                e.to_string(),
            )
        })?;

        let recon_png = recon_dir.join(format!("{:06}.png", meta.frame_index));
        let tmp_recon_png = recon_dir.join(format!(".recon.tmp-{}", Uuid::new_v4()));
        {
            let mut file = File::create(&tmp_recon_png).map_err(|e| {
                AppError::storage_error("Failed to create temporary recon frame", e.to_string())
            })?;
            file.write_all(image_png_bytes).map_err(|e| {
                AppError::storage_error("Failed to write temporary recon frame", e.to_string())
            })?;
            file.sync_all().map_err(|e| {
                AppError::storage_error("Failed to sync temporary recon frame", e.to_string())
            })?;
        }

        #[cfg(target_os = "windows")]
        {
            if recon_png.exists() {
                let _ = fs::remove_file(&recon_png);
            }
        }
        fs::rename(&tmp_recon_png, &recon_png).map_err(|e| {
            let _ = fs::remove_file(&tmp_recon_png);
            AppError::storage_error("Failed to persist reconstruction frame PNG", e.to_string())
        })?;

        // 3. Write metadata JSON with real SHA-256 and byte sizes
        let mut final_meta = meta.clone();
        let sha256 = calculate_sha256(image_png_bytes);
        final_meta.artifact_hash = Some(sha256);
        final_meta.artifact_size_bytes = Some(image_png_bytes.len() as u64);
        if final_meta.created_at.is_none() {
            final_meta.created_at = Some(Utc::now().to_rfc3339());
        }

        let out_json = self.frame_result_json_path(meta.frame_index);
        let tmp_json = f_dir.join(format!(".result.json.tmp-{}", Uuid::new_v4()));
        let json_str = serde_json::to_string_pretty(&final_meta).map_err(|e| {
            AppError::storage_error("Failed to serialize frame metadata", e.to_string())
        })?;
        {
            let mut file = File::create(&tmp_json).map_err(|e| {
                AppError::storage_error("Failed to create temporary frame metadata", e.to_string())
            })?;
            file.write_all(json_str.as_bytes()).map_err(|e| {
                AppError::storage_error("Failed to write temporary frame metadata", e.to_string())
            })?;
            file.sync_all().map_err(|e| {
                AppError::storage_error("Failed to sync temporary frame metadata", e.to_string())
            })?;
        }

        #[cfg(target_os = "windows")]
        {
            if out_json.exists() {
                let _ = fs::remove_file(&out_json);
            }
        }
        fs::rename(&tmp_json, &out_json).map_err(|e| {
            let _ = fs::remove_file(&tmp_json);
            AppError::storage_error("Failed to persist frame metadata JSON", e.to_string())
        })?;

        let total_bytes = (image_png_bytes.len() * 2 + json_str.len()) as u64;
        Ok(total_bytes)
    }

    /// Links or copies an original source frame to the reconstruction frames directory as passthrough.
    pub fn write_passthrough_frame(
        &self,
        frame_index: usize,
        source_frame_path: &Path,
    ) -> Result<u64, AppError> {
        let recon_dir = self.reconstruction_frames_dir();
        fs::create_dir_all(&recon_dir).map_err(|e| {
            AppError::storage_error(
                "Failed to create reconstruction frames directory",
                e.to_string(),
            )
        })?;
        let recon_png = recon_dir.join(format!("{:06}.png", frame_index));

        if !source_frame_path.exists() {
            return Err(AppError::media_file_not_found(
                source_frame_path.display().to_string(),
            ));
        }

        let copied_bytes = fs::copy(source_frame_path, &recon_png).map_err(|e| {
            AppError::storage_error("Failed to copy passthrough frame", e.to_string())
        })?;

        Ok(copied_bytes)
    }

    /// Loads frame metadata JSON if present on disk.
    pub fn load_frame_metadata(
        &self,
        frame_index: usize,
    ) -> Result<Option<AiFrameMetadata>, AppError> {
        let json_path = self.frame_result_json_path(frame_index);
        if !json_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&json_path).map_err(|e| {
            AppError::storage_error("Failed to read frame metadata JSON", e.to_string())
        })?;
        let meta: AiFrameMetadata = serde_json::from_str(&content).map_err(|e| {
            AppError::storage_error("Failed to parse frame metadata JSON", e.to_string())
        })?;
        Ok(Some(meta))
    }

    /// Safely purges corrupted or mismatched artifacts for a specific frame index.
    pub fn clean_frame_artifacts(&self, frame_index: usize) {
        let _ = fs::remove_file(self.frame_output_png_path(frame_index));
        let _ = fs::remove_file(self.frame_result_json_path(frame_index));
        let _ = fs::remove_file(
            self.reconstruction_frames_dir()
                .join(format!("{:06}.png", frame_index)),
        );
        let _ = fs::remove_dir(self.frame_dir(frame_index));
    }

    /// Validates whether a cached frame artifact exists and is structurally valid for reuse.
    pub fn validate_frame_artifact(
        &self,
        frame_index: usize,
        expected_model_id: &str,
        expected_config_hash: &str,
    ) -> Option<AiFrameMetadata> {
        self.validate_frame_artifact_deep(
            frame_index,
            expected_model_id,
            expected_config_hash,
            None,
            None,
        )
    }

    /// Deep validation for artifact reuse ensuring SHA-256 file integrity and pinned model hashes match.
    pub fn validate_frame_artifact_deep(
        &self,
        frame_index: usize,
        expected_model_id: &str,
        expected_config_hash: &str,
        expected_model_hash: Option<&str>,
        expected_profile_hash: Option<&str>,
    ) -> Option<AiFrameMetadata> {
        let json_path = self.frame_result_json_path(frame_index);
        let png_path = self.frame_output_png_path(frame_index);
        let recon_png = self
            .reconstruction_frames_dir()
            .join(format!("{:06}.png", frame_index));

        if !json_path.exists() || !png_path.exists() || !recon_png.exists() {
            return None;
        }

        // Check PNG file size > 0
        if let Ok(meta) = fs::metadata(&png_path) {
            if meta.len() == 0 {
                self.clean_frame_artifacts(frame_index);
                return None;
            }
        } else {
            return None;
        }

        if let Ok(meta) = fs::metadata(&recon_png) {
            if meta.len() == 0 {
                self.clean_frame_artifacts(frame_index);
                return None;
            }
        } else {
            return None;
        }

        // Read and validate JSON
        let content = fs::read_to_string(&json_path).ok()?;
        let frame_meta = serde_json::from_str::<AiFrameMetadata>(&content).ok()?;

        if frame_meta.frame_index != frame_index {
            self.clean_frame_artifacts(frame_index);
            return None;
        }
        if frame_meta.model_id != expected_model_id {
            self.clean_frame_artifacts(frame_index);
            return None;
        }
        if frame_meta.config_hash != expected_config_hash {
            self.clean_frame_artifacts(frame_index);
            return None;
        }

        if let Some(exp_mhash) = expected_model_hash {
            if let Some(ref mhash) = frame_meta.model_hash {
                if mhash != exp_mhash {
                    self.clean_frame_artifacts(frame_index);
                    return None;
                }
            }
        }

        if let Some(exp_phash) = expected_profile_hash {
            if let Some(ref phash) = frame_meta.profile_hash {
                if phash != exp_phash {
                    self.clean_frame_artifacts(frame_index);
                    return None;
                }
            }
        }

        // Real SHA-256 byte check
        if let Some(ref expected_sha) = frame_meta.artifact_hash {
            if let Ok(bytes) = fs::read(&png_path) {
                let actual_sha = calculate_sha256(&bytes);
                if &actual_sha != expected_sha {
                    self.clean_frame_artifacts(frame_index);
                    return None;
                }
            } else {
                self.clean_frame_artifacts(frame_index);
                return None;
            }
        }

        Some(frame_meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_compute_config_hash_deterministic() {
        let prep = PreprocessConfig::default();
        let hash1 = compute_ai_config_hash("test-model", &prep, None);
        let hash2 = compute_ai_config_hash("test-model", &prep, None);
        assert_eq!(hash1, hash2);

        let hash3 = compute_ai_config_hash("other-model", &prep, None);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_write_and_validate_frame_artifact() {
        let tmp = tempdir().unwrap();
        let manager = AiArtifactManager::new(tmp.path());
        manager.ensure_dirs().unwrap();

        let meta = AiFrameMetadata {
            job_id: Some("job-123".to_string()),
            frame_index: 0,
            source_frame_index: 0,
            status: AiFrameStatus::Completed,
            model_id: "test-model".to_string(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some("mhash123".to_string()),
            profile_hash: Some("phash123".to_string()),
            provider: "CPU".to_string(),
            decode_duration_ms: 1.0,
            preprocess_duration_ms: 1.0,
            inference_duration_ms: 2.0,
            postprocess_duration_ms: 1.0,
            total_duration_ms: 5.0,
            input_width: 2,
            input_height: 2,
            output_width: 2,
            output_height: 2,
            output_artifact_path: "output.png".to_string(),
            config_hash: "hash123".to_string(),
            artifact_hash: None,
            artifact_size_bytes: None,
            created_at: None,
        };

        let dummy_png = b"dummy png content";
        manager.write_frame_artifact(&meta, dummy_png).unwrap();

        // Valid artifact retrieval
        let validated = manager
            .validate_frame_artifact_deep(
                0,
                "test-model",
                "hash123",
                Some("mhash123"),
                Some("phash123"),
            )
            .expect("Artifact should be valid");
        assert_eq!(validated.frame_index, 0);
        assert_eq!(validated.status, AiFrameStatus::Completed);
        assert!(validated.artifact_hash.is_some());

        // Invalid model ID
        assert!(manager
            .validate_frame_artifact(0, "wrong-model", "hash123")
            .is_none());

        // Invalid config hash
        assert!(manager
            .validate_frame_artifact(0, "test-model", "wrong-hash")
            .is_none());

        // Missing frame index
        assert!(manager
            .validate_frame_artifact(1, "test-model", "hash123")
            .is_none());
    }
}
