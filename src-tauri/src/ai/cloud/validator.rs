use super::error::CloudProviderError;
use super::job::OutputArtifactRecord;
use crate::media::MediaService;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ValidatedArtifactMetadata {
    pub artifact_hash: String,
    pub width: u32,
    pub height: u32,
    pub duration_sec: f64,
    pub fps: f64,
}

pub struct CloudOutputValidator {
    media_service: MediaService,
}

impl Default for CloudOutputValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudOutputValidator {
    pub fn new() -> Self {
        Self {
            media_service: MediaService::new(),
        }
    }

    pub fn compute_file_sha256(path: &Path) -> Result<String, CloudProviderError> {
        let mut file = File::open(path).map_err(|e| {
            CloudProviderError::RequestInvalid(format!(
                "Failed to open file for hashing {}: {}",
                path.display(),
                e
            ))
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let bytes_read = file.read(&mut buffer).map_err(|e| {
                CloudProviderError::RequestInvalid(format!(
                    "Failed to read file for hashing {}: {}",
                    path.display(),
                    e
                ))
            })?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn validate_artifact(
        &self,
        partial_path: &Path,
        expected_duration_sec: Option<f64>,
        require_audio: bool,
    ) -> Result<ValidatedArtifactMetadata, CloudProviderError> {
        // 1. File existence and size check
        if !partial_path.exists() {
            return Err(CloudProviderError::ProviderUnavailable(format!(
                "Artifact validation failed: partial file does not exist at {}",
                partial_path.display()
            )));
        }

        let metadata = fs::metadata(partial_path).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Artifact validation failed: cannot read metadata of {}: {}",
                partial_path.display(),
                e
            ))
        })?;

        if metadata.len() == 0 {
            return Err(CloudProviderError::ProviderUnavailable(format!(
                "Artifact validation failed: downloaded file is empty (0 bytes) at {}",
                partial_path.display()
            )));
        }

        // 2. Strict FFprobe deep inspection
        let probe = self
            .media_service
            .probe_with_ffprobe(partial_path, "artifact.mp4", "mp4", metadata.len())
            .map_err(|e| {
                CloudProviderError::ProviderUnavailable(format!(
                    "FFprobe validation failed on artifact {}: {}",
                    partial_path.display(),
                    e
                ))
            })?;

        if probe.width == 0 || probe.height == 0 {
            return Err(CloudProviderError::ProviderUnavailable(format!(
                "Invalid video dimensions: {}x{}",
                probe.width, probe.height
            )));
        }

        let duration_sec = probe.duration_ms as f64 / 1000.0;
        if duration_sec <= 0.0 || !duration_sec.is_finite() {
            return Err(CloudProviderError::ProviderUnavailable(format!(
                "Invalid non-finite or non-positive duration: {}",
                duration_sec
            )));
        }

        // Check duration tolerance if expected duration is provided
        if let Some(exp_dur) = expected_duration_sec {
            if exp_dur > 0.0 {
                let min_acceptable = (exp_dur * 0.8).max(0.1);
                let max_acceptable = exp_dur * 1.2;
                if duration_sec < min_acceptable || duration_sec > max_acceptable {
                    return Err(CloudProviderError::ProviderUnavailable(format!(
                        "Artifact duration {:.2}s exceeds tolerance bounds [{:.2}s, {:.2}s] for requested duration {:.2}s",
                        duration_sec, min_acceptable, max_acceptable, exp_dur
                    )));
                }
            }
        }

        // Audio requirement check
        if require_audio && !probe.has_audio {
            return Err(CloudProviderError::ProviderUnavailable(
                "Audio preservation requested but output artifact has no audio stream".to_string(),
            ));
        }

        // 3. Compute SHA256 hash
        let artifact_hash = Self::compute_file_sha256(partial_path)?;

        Ok(ValidatedArtifactMetadata {
            artifact_hash,
            width: probe.width,
            height: probe.height,
            duration_sec,
            fps: probe.fps,
        })
    }

    pub fn promote_artifact(
        partial_path: &Path,
        final_path: &Path,
        metadata: &ValidatedArtifactMetadata,
    ) -> Result<OutputArtifactRecord, CloudProviderError> {
        if let Some(parent) = final_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        super::store::atomic_replace(partial_path, final_path).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to atomically promote {} to final artifact {}: {}",
                partial_path.display(),
                final_path.display(),
                e
            ))
        })?;

        Ok(OutputArtifactRecord {
            temporary_path: Some(partial_path.to_path_buf()),
            final_path: Some(final_path.to_path_buf()),
            artifact_hash: Some(metadata.artifact_hash.clone()),
            width: Some(metadata.width),
            height: Some(metadata.height),
            duration_sec: Some(metadata.duration_sec),
            fps: Some(metadata.fps),
        })
    }

    pub fn validate_and_promote_artifact(
        &self,
        partial_path: &Path,
        final_path: &Path,
        expected_duration_sec: Option<f64>,
        require_audio: bool,
    ) -> Result<OutputArtifactRecord, CloudProviderError> {
        let meta = self.validate_artifact(partial_path, expected_duration_sec, require_audio)?;
        Self::promote_artifact(partial_path, final_path, &meta)
    }
}
