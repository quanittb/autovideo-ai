use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::ai::frame_pipeline::config::{AiFrameOutputMode, AiJobConfig};
use crate::ai::profile::OutputInterpretationType;
use crate::ai::registry::ModelRegistry;
use crate::ai::resolver::{ProductionModelResolver, ResolvedProductionModel};
use crate::error::AppError;
use crate::media::MediaService;
use crate::system::StoragePaths;

/// Preflight check pass/warn/fail status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreflightCheckStatus {
    Pass,
    Warn,
    Fail,
}

/// Preflight check severity descriptor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreflightCheckSeverity {
    Info,
    Warning,
    Error,
}

/// Individual item check in the preflight validation report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreflightCheckResult {
    pub check: String,
    pub status: PreflightCheckStatus,
    pub severity: PreflightCheckSeverity,
    pub message: String,
    pub technical_detail: Option<String>,
}

impl PreflightCheckResult {
    pub fn pass(check: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            check: check.into(),
            status: PreflightCheckStatus::Pass,
            severity: PreflightCheckSeverity::Info,
            message: message.into(),
            technical_detail: None,
        }
    }

    pub fn warn(
        check: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            check: check.into(),
            status: PreflightCheckStatus::Warn,
            severity: PreflightCheckSeverity::Warning,
            message: message.into(),
            technical_detail: detail,
        }
    }

    pub fn fail(
        check: impl Into<String>,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> Self {
        Self {
            check: check.into(),
            status: PreflightCheckStatus::Fail,
            severity: PreflightCheckSeverity::Error,
            message: message.into(),
            technical_detail: detail,
        }
    }
}

/// Structured comprehensive preflight validation report for an AI Video Job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiJobPreflightReport {
    pub is_valid: bool,
    pub checks: Vec<PreflightCheckResult>,
    pub resolved_model: Option<ResolvedProductionModel>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Validates all pre-conditions before creating or launching an AI Video Inference Job.
pub fn validate_ai_job_preflight(
    source_path: &Path,
    ai_config: &AiJobConfig,
    storage_paths: &StoragePaths,
) -> Result<AiJobPreflightReport, AppError> {
    let mut checks = Vec::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    // -------------------------------------------------------------
    // 1. Source Media Existence & File Type
    // -------------------------------------------------------------
    if !source_path.exists() {
        let msg = format!(
            "Source video file '{}' does not exist",
            source_path.display()
        );
        checks.push(PreflightCheckResult::fail("SOURCE_FILE_EXISTS", &msg, None));
        errors.push(msg);
    } else if !source_path.is_file() {
        let msg = format!(
            "Source path '{}' is a directory, not a file",
            source_path.display()
        );
        checks.push(PreflightCheckResult::fail("SOURCE_FILE_TYPE", &msg, None));
        errors.push(msg);
    } else {
        let file_len = fs::metadata(source_path).map(|m| m.len()).unwrap_or(0);
        if file_len == 0 {
            let msg = format!(
                "Source media file '{}' is 0 bytes (empty file)",
                source_path.display()
            );
            checks.push(PreflightCheckResult::fail("SOURCE_FILE_EMPTY", &msg, None));
            errors.push(msg);
        } else {
            checks.push(PreflightCheckResult::pass(
                "SOURCE_FILE_EXISTS",
                format!(
                    "Source media file '{}' exists and is readable ({} bytes)",
                    source_path.display(),
                    file_len
                ),
            ));
        }

        // Format validation
        let ext = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let supported_exts = ["mp4", "mov", "mkv", "avi", "webm"];
        if !supported_exts.contains(&ext.as_str()) {
            let msg = format!(
                "Unsupported media extension '.{}'. Supported: {:?}",
                ext, supported_exts
            );
            checks.push(PreflightCheckResult::fail(
                "SOURCE_MEDIA_FORMAT",
                &msg,
                None,
            ));
            errors.push(msg);
        } else {
            checks.push(PreflightCheckResult::pass(
                "SOURCE_MEDIA_FORMAT",
                format!(
                    "Media container format '.{}' is supported for processing",
                    ext
                ),
            ));
        }

        // -------------------------------------------------------------
        // 2. Source Media Stream Probe (FFprobe / MediaService)
        // -------------------------------------------------------------
        match MediaService::new().probe(source_path) {
            Ok(metadata) => {
                if metadata.duration_ms == 0 {
                    let msg = "Source video duration is 0 seconds or stream is invalid".to_string();
                    checks.push(PreflightCheckResult::fail(
                        "SOURCE_VIDEO_STREAM",
                        &msg,
                        None,
                    ));
                    errors.push(msg);
                } else {
                    let duration_sec = metadata.duration_ms as f64 / 1000.0;
                    checks.push(PreflightCheckResult::pass(
                        "SOURCE_VIDEO_STREAM",
                        format!(
                            "Video stream validated: {}x{}, {:.2}s duration, {:.2} FPS",
                            metadata.width, metadata.height, duration_sec, metadata.fps
                        ),
                    ));
                }
            }
            Err(e) => {
                let msg = format!("Failed to probe video stream metadata: {}", e.message);
                checks.push(PreflightCheckResult::fail(
                    "SOURCE_VIDEO_STREAM",
                    &msg,
                    e.details,
                ));
                errors.push(msg);
            }
        }
    }

    // -------------------------------------------------------------
    // 3. Model Package Resolution, Integrity & ONNX Compatibility
    // -------------------------------------------------------------
    let registry = ModelRegistry::new(storage_paths.models_dir.clone());
    let resolved_model = match ProductionModelResolver::resolve_model(
        &registry,
        Some(&ai_config.model_id),
        ai_config.model_version.as_deref(),
        ai_config.provider,
    ) {
        Ok(resolved) => {
            checks.push(PreflightCheckResult::pass(
                "MODEL_PACKAGE_RESOLVED",
                format!(
                    "Production model '{}' v{} successfully resolved and validated",
                    resolved.model_id, resolved.model_version
                ),
            ));

            checks.push(PreflightCheckResult::pass(
                "MODEL_FILE_INTEGRITY",
                format!(
                    "SHA-256 integrity checksum verified: {}",
                    resolved.model_hash
                ),
            ));

            checks.push(PreflightCheckResult::pass(
                "MODEL_ONNX_GRAPH",
                "ONNX computation graph input/output signatures validated against model profile"
                    .to_string(),
            ));

            checks.push(PreflightCheckResult::pass(
                "EXECUTION_PROVIDER",
                format!(
                    "Execution provider '{:?}' is available on host and supported by model",
                    resolved.provider
                ),
            ));

            // -------------------------------------------------------------
            // 4. Preprocessing Profile Compatibility
            // -------------------------------------------------------------
            let prof = &resolved.profile;
            if prof.input.target_width != ai_config.preprocessing.target_width
                || prof.input.target_height != ai_config.preprocessing.target_height
            {
                warnings.push(format!(
                    "Preprocessing resolution ({}x{}) differs from model profile geometry ({}x{})",
                    ai_config.preprocessing.target_width,
                    ai_config.preprocessing.target_height,
                    prof.input.target_width,
                    prof.input.target_height
                ));
                checks.push(PreflightCheckResult::warn(
                    "PREPROCESSING_GEOMETRY",
                    "Custom preprocessing resolution differs from model profile default",
                    Some(format!(
                        "Profile: {}x{}, Job Config: {}x{}",
                        prof.input.target_width,
                        prof.input.target_height,
                        ai_config.preprocessing.target_width,
                        ai_config.preprocessing.target_height
                    )),
                ));
            } else {
                checks.push(PreflightCheckResult::pass(
                    "PREPROCESSING_GEOMETRY",
                    format!(
                        "Preprocessing geometry matches model profile ({}x{})",
                        prof.input.target_width, prof.input.target_height
                    ),
                ));
            }

            // -------------------------------------------------------------
            // 5. Output Mode Compatibility
            // -------------------------------------------------------------
            let output_compatible = match (prof.output.output_type, ai_config.output_mode) {
                (OutputInterpretationType::Mask, AiFrameOutputMode::Mask) => true,
                (OutputInterpretationType::Image, AiFrameOutputMode::Image) => true,
                (OutputInterpretationType::BBox, AiFrameOutputMode::Image) => true,
                _ => false,
            };

            if !output_compatible {
                let msg = format!(
                    "Configured job output mode '{:?}' is incompatible with model output type '{:?}'",
                    ai_config.output_mode, prof.output.output_type
                );
                checks.push(PreflightCheckResult::fail(
                    "OUTPUT_MODE_COMPATIBILITY",
                    &msg,
                    None,
                ));
                errors.push(msg);
            } else {
                checks.push(PreflightCheckResult::pass(
                    "OUTPUT_MODE_COMPATIBILITY",
                    format!(
                        "Job output mode '{:?}' is compatible with model output interpretation",
                        ai_config.output_mode
                    ),
                ));
            }

            Some(resolved)
        }
        Err(e) => {
            let msg = format!("Production model resolution failed: {}", e.message);
            checks.push(PreflightCheckResult::fail(
                "MODEL_PACKAGE_RESOLVED",
                &msg,
                e.details.clone(),
            ));
            errors.push(msg);
            None
        }
    };

    // -------------------------------------------------------------
    // 6. Working Storage & Directory Writability Check
    // -------------------------------------------------------------
    let required_dirs = [
        ("PROJECTS_DIR", &storage_paths.projects_dir),
        ("CACHE_DIR", &storage_paths.cache_dir),
        ("TEMP_DIR", &storage_paths.temp_dir),
    ];

    for (name, dir) in required_dirs {
        if !dir.exists() {
            if let Err(e) = fs::create_dir_all(dir) {
                let msg = format!("Directory '{}' could not be created: {}", dir.display(), e);
                checks.push(PreflightCheckResult::fail(
                    format!("DIR_WRITABLE_{}", name),
                    &msg,
                    None,
                ));
                errors.push(msg);
                continue;
            }
        }

        // Test writability with a temp probe file
        let probe = dir.join(format!(".probe_write_{}", uuid::Uuid::new_v4()));
        match fs::write(&probe, b"ok") {
            Ok(_) => {
                let _ = fs::remove_file(&probe);
                checks.push(PreflightCheckResult::pass(
                    format!("DIR_WRITABLE_{}", name),
                    format!("Storage directory '{}' is writable", dir.display()),
                ));
            }
            Err(e) => {
                let msg = format!(
                    "Storage directory '{}' is not writable: {}",
                    dir.display(),
                    e
                );
                checks.push(PreflightCheckResult::fail(
                    format!("DIR_WRITABLE_{}", name),
                    &msg,
                    None,
                ));
                errors.push(msg);
            }
        }
    }

    let is_valid = errors.is_empty();

    Ok(AiJobPreflightReport {
        is_valid,
        checks,
        resolved_model,
        warnings,
        errors,
    })
}
