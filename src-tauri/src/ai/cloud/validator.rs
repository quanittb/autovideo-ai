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
        let policy = super::job::ValidationPolicy {
            expected_duration_sec,
            expected_width: None,
            expected_height: None,
            expected_fps: None,
            require_audio,
            require_alpha: false,
            expected_container: None,
            expected_video_codec: None,
        };
        self.validate_artifact_with_policy(partial_path, &policy)
    }

    pub fn validate_artifact_with_policy(
        &self,
        partial_path: &Path,
        policy: &super::job::ValidationPolicy,
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
        let file_name = partial_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("artifact.mp4");
        let container = if let Some(exp) = &policy.expected_container {
            exp.as_str()
        } else {
            let ext = partial_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("mp4");
            if ext == "partial" {
                "mp4"
            } else {
                ext
            }
        };

        let probe = self
            .media_service
            .probe_with_ffprobe(partial_path, file_name, container, metadata.len())
            .map_err(|e| {
                CloudProviderError::ProviderUnavailable(format!(
                    "FFprobe validation failed on artifact {}: {}",
                    partial_path.display(),
                    e
                ))
            })?;

        if probe.width == 0 || probe.height == 0 {
            return Err(CloudProviderError::OutputInvalid(format!(
                "Invalid video dimensions: {}x{}",
                probe.width, probe.height
            )));
        }

        if let Some(exp_w) = policy.expected_width {
            if probe.width != exp_w {
                return Err(CloudProviderError::OutputInvalid(format!(
                    "CLOUD_OUTPUT_INVALID: Video width {} does not match expected {}",
                    probe.width, exp_w
                )));
            }
        }

        if let Some(exp_h) = policy.expected_height {
            if probe.height != exp_h {
                return Err(CloudProviderError::OutputInvalid(format!(
                    "CLOUD_OUTPUT_INVALID: Video height {} does not match expected {}",
                    probe.height, exp_h
                )));
            }
        }

        let duration_sec = probe.duration_ms as f64 / 1000.0;
        if duration_sec <= 0.0 || !duration_sec.is_finite() {
            return Err(CloudProviderError::OutputInvalid(format!(
                "Invalid non-finite or non-positive duration: {}",
                duration_sec
            )));
        }

        // Check duration tolerance if expected duration is provided
        if let Some(exp_dur) = policy.expected_duration_sec {
            if exp_dur > 0.0 {
                let min_acceptable = (exp_dur * 0.8).max(0.1);
                let max_acceptable = exp_dur * 1.2;
                if duration_sec < min_acceptable || duration_sec > max_acceptable {
                    return Err(CloudProviderError::OutputInvalid(format!(
                        "Artifact duration {:.2}s exceeds tolerance bounds [{:.2}s, {:.2}s] for requested duration {:.2}s",
                        duration_sec, min_acceptable, max_acceptable, exp_dur
                    )));
                }
            }
        }

        if let Some(exp_fps) = policy.expected_fps {
            if (probe.fps - exp_fps).abs() >= 0.5 {
                return Err(CloudProviderError::OutputInvalid(format!(
                    "CLOUD_OUTPUT_INVALID: Video fps {:.2} does not match expected {:.2}",
                    probe.fps, exp_fps
                )));
            }
        }

        if let Some(ref exp_container) = policy.expected_container {
            let probe_cont = probe.container.to_lowercase();
            let exp_cont = exp_container.to_lowercase();
            if !probe_cont.contains(&exp_cont) && !exp_cont.contains(&probe_cont) {
                return Err(CloudProviderError::OutputInvalid(format!(
                    "CLOUD_OUTPUT_INVALID: Output container '{}' does not match expected '{}'",
                    probe.container, exp_container
                )));
            }
        }

        if let Some(ref exp_codec) = policy.expected_video_codec {
            let probe_codec = probe.video_codec.to_lowercase();
            let exp_codec_lower = exp_codec.to_lowercase();
            if !probe_codec.contains(&exp_codec_lower) && !exp_codec_lower.contains(&probe_codec) {
                return Err(CloudProviderError::OutputInvalid(format!(
                    "CLOUD_OUTPUT_INVALID: Output video codec '{}' does not match expected '{}'",
                    probe.video_codec, exp_codec
                )));
            }
        }

        // Audio requirement check
        if policy.require_audio && !probe.has_audio {
            return Err(CloudProviderError::OutputInvalid(
                "Audio preservation requested but output artifact has no audio stream".to_string(),
            ));
        }

        // 3. Two-Step Alpha Transparency Check (if required)
        if policy.require_alpha {
            // Stage A: Stream format probe check via ffprobe
            let ffprobe_cmd = std::process::Command::new("ffprobe")
                .arg("-v")
                .arg("error")
                .arg("-select_streams")
                .arg("v:0")
                .arg("-show_entries")
                .arg("stream=pix_fmt:stream_tags=alpha_mode")
                .arg("-of")
                .arg("default=noprint_wrappers=1")
                .arg(partial_path)
                .output();

            let probe_output = match ffprobe_cmd {
                Ok(out) if out.status.success() => {
                    String::from_utf8_lossy(&out.stdout).to_lowercase()
                }
                _ => String::new(),
            };

            let has_alpha_flag = probe_output.contains("alpha_mode=1")
                || probe_output.contains("yuva")
                || probe_output.contains("rgba")
                || probe_output.contains("bgra")
                || probe_output.contains("gbra")
                || probe_output.contains("ya8")
                || probe_output.contains("ya16");

            // Stage B: Deterministic alpha plane extraction decode check via ffmpeg
            let ffmpeg_alpha_test = std::process::Command::new("ffmpeg")
                .arg("-v")
                .arg("error")
                .arg("-c:v")
                .arg("libvpx-vp9")
                .arg("-i")
                .arg(partial_path)
                .arg("-vframes")
                .arg("1")
                .arg("-filter_complex")
                .arg("[0:v]alphaextract[a]")
                .arg("-map")
                .arg("[a]")
                .arg("-f")
                .arg("null")
                .arg("-")
                .output();

            let stage_b_ok = match ffmpeg_alpha_test {
                Ok(out) => out.status.success() && out.stderr.is_empty(),
                Err(_) => false,
            };

            if !has_alpha_flag || !stage_b_ok {
                return Err(CloudProviderError::OutputInvalid(format!(
                    "CLOUD_OUTPUT_INVALID: Output lacks decodable alpha transparency (probe: '{}', alphaextract_ok: {})",
                    probe_output.trim(), stage_b_ok
                )));
            }
        }

        // 4. Compute SHA256 hash
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
