use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ai::frame_pipeline::config::FrameSamplingConfig;
use crate::error::AppError;

const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// Quality validation outcome status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FrameQualityStatus {
    Pass,
    Warning,
    Fail,
}

/// Deterministic technical quality metrics computed from real decoded pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalQualityMetrics {
    pub decoded_width: u32,
    pub decoded_height: u32,
    pub file_size_bytes: u64,
    pub has_alpha: bool,
    pub non_zero_pixel_ratio: f32,
    pub min_pixel_value: u8,
    pub max_pixel_value: u8,
    pub mean_pixel_value: f32,
    pub variance: f32,
    pub clipping_ratio: f32,
    pub black_frame_detected: bool,
    pub nan_or_inf_detected: bool,
}

/// Structured quality validation report for an AI frame artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameQualityReport {
    pub frame_index: usize,
    pub status: FrameQualityStatus,
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub metrics: Option<TechnicalQualityMetrics>,
}

/// Structured validation report for an entire sequence of frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSequenceValidationReport {
    pub is_valid: bool,
    pub total_expected: usize,
    pub total_found: usize,
    pub missing_indices: Vec<usize>,
    pub duplicate_indices: Vec<usize>,
    pub passthrough_mismatches: Vec<usize>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct FrameQualityValidator;

impl FrameQualityValidator {
    /// Validates raw PNG byte stream against standard container signatures and expected dimensions.
    pub fn validate_png_bytes(
        frame_index: usize,
        bytes: &[u8],
        expected_width: Option<u32>,
        expected_height: Option<u32>,
        is_mask: bool,
    ) -> Result<FrameQualityReport, AppError> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // 1. File size check
        if bytes.is_empty() {
            errors.push(format!(
                "Frame {:06} byte stream is empty (0 bytes)",
                frame_index
            ));
            return Ok(FrameQualityReport {
                frame_index,
                status: FrameQualityStatus::Fail,
                is_valid: false,
                errors,
                warnings,
                metrics: None,
            });
        }

        // 2. PNG Magic Byte Header check
        if bytes.len() < 8 || bytes[0..8] != PNG_MAGIC {
            errors.push(format!(
                "Frame {:06} has invalid PNG signature header",
                frame_index
            ));
            return Ok(FrameQualityReport {
                frame_index,
                status: FrameQualityStatus::Fail,
                is_valid: false,
                errors,
                warnings,
                metrics: None,
            });
        }

        // 3. Decode frame using image crate
        let img = match image::load_from_memory(bytes) {
            Ok(im) => im,
            Err(e) => {
                errors.push(format!(
                    "Frame {:06} failed image decoding: {}",
                    frame_index, e
                ));
                return Ok(FrameQualityReport {
                    frame_index,
                    status: FrameQualityStatus::Fail,
                    is_valid: false,
                    errors,
                    warnings,
                    metrics: None,
                });
            }
        };

        let width = img.width();
        let height = img.height();

        if width == 0 || height == 0 {
            errors.push(format!(
                "Frame {:06} has zero dimensions ({}x{})",
                frame_index, width, height
            ));
        }

        if let Some(exp_w) = expected_width {
            if width != exp_w {
                errors.push(format!(
                    "Frame {:06} width mismatch: expected {}, got {}",
                    frame_index, exp_w, width
                ));
            }
        }

        if let Some(exp_h) = expected_height {
            if height != exp_h {
                errors.push(format!(
                    "Frame {:06} height mismatch: expected {}, got {}",
                    frame_index, exp_h, height
                ));
            }
        }

        // 4. Extract pixel metrics
        let raw_pixels = img.as_bytes();
        let has_alpha = match img.color() {
            image::ColorType::Rgba8
            | image::ColorType::La8
            | image::ColorType::Rgba16
            | image::ColorType::La16
            | image::ColorType::Rgba32F => true,
            _ => false,
        };

        let mut min_val = 255u8;
        let mut max_val = 0u8;
        let mut sum_val = 0u64;
        let mut non_zero_count = 0u64;
        let mut clipped_count = 0u64;

        for &p in raw_pixels {
            if p < min_val {
                min_val = p;
            }
            if p > max_val {
                max_val = p;
            }
            if p > 0 {
                non_zero_count += 1;
            }
            if p == 0 || p == 255 {
                clipped_count += 1;
            }
            sum_val += p as u64;
        }

        if raw_pixels.is_empty() {
            min_val = 0;
        }

        let total_pixel_bytes = raw_pixels.len() as f32;
        let non_zero_pixel_ratio = if total_pixel_bytes > 0.0 {
            non_zero_count as f32 / total_pixel_bytes
        } else {
            0.0
        };

        let mean_pixel_value = if total_pixel_bytes > 0.0 {
            sum_val as f32 / total_pixel_bytes
        } else {
            0.0
        };

        // Calculate variance
        let mut variance_sum = 0.0f64;
        for &p in raw_pixels {
            let diff = p as f64 - mean_pixel_value as f64;
            variance_sum += diff * diff;
        }
        let variance = if total_pixel_bytes > 0.0 {
            (variance_sum / total_pixel_bytes as f64) as f32
        } else {
            0.0
        };

        let clipping_ratio = if total_pixel_bytes > 0.0 {
            clipped_count as f32 / total_pixel_bytes
        } else {
            0.0
        };

        let black_frame_detected = mean_pixel_value < 2.0 && max_val < 10;
        let nan_or_inf_detected = !mean_pixel_value.is_finite() || !variance.is_finite();

        if nan_or_inf_detected {
            errors.push(format!(
                "Frame {:06} produced non-finite (NaN or Inf) pixel metrics",
                frame_index
            ));
        }

        if is_mask && non_zero_pixel_ratio == 0.0 {
            warnings.push(format!(
                "Frame {:06} mask is completely empty (all black pixels)",
                frame_index
            ));
        } else if black_frame_detected && !is_mask {
            warnings.push(format!(
                "Frame {:06} appears to be entirely black (mean: {:.2}, max: {})",
                frame_index, mean_pixel_value, max_val
            ));
        }

        let metrics = TechnicalQualityMetrics {
            decoded_width: width,
            decoded_height: height,
            file_size_bytes: bytes.len() as u64,
            has_alpha,
            non_zero_pixel_ratio,
            min_pixel_value: min_val,
            max_pixel_value: max_val,
            mean_pixel_value,
            variance,
            clipping_ratio,
            black_frame_detected,
            nan_or_inf_detected,
        };

        let status = if !errors.is_empty() {
            FrameQualityStatus::Fail
        } else if !warnings.is_empty() {
            FrameQualityStatus::Warning
        } else {
            FrameQualityStatus::Pass
        };

        let is_valid = errors.is_empty();

        Ok(FrameQualityReport {
            frame_index,
            status,
            is_valid,
            errors,
            warnings,
            metrics: Some(metrics),
        })
    }

    /// Validates an on-disk frame artifact PNG file.
    pub fn validate_frame_file(
        path: &Path,
        frame_index: usize,
        expected_width: Option<u32>,
        expected_height: Option<u32>,
        is_mask: bool,
    ) -> Result<FrameQualityReport, AppError> {
        if !path.exists() {
            return Ok(FrameQualityReport {
                frame_index,
                status: FrameQualityStatus::Fail,
                is_valid: false,
                errors: vec![format!("Frame file not found at '{}'", path.display())],
                warnings: vec![],
                metrics: None,
            });
        }

        let bytes = fs::read(path).map_err(|e| {
            AppError::storage_error("Failed to read frame artifact file", e.to_string())
        })?;

        Self::validate_png_bytes(
            frame_index,
            &bytes,
            expected_width,
            expected_height,
            is_mask,
        )
    }

    /// Validates an entire sequence of frames for temporal continuity, missing frames, and passthrough integrity.
    pub fn validate_frame_sequence(
        source_frames_dir: &Path,
        artifacts_dir: &Path,
        total_expected: usize,
        sampling: &FrameSamplingConfig,
    ) -> Result<FrameSequenceValidationReport, AppError> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut missing_indices = Vec::new();
        let duplicate_indices = Vec::new();
        let mut passthrough_mismatches = Vec::new();

        let (selected_indices, _) = (
            crate::ai::frame_pipeline::config::select_frames(total_expected, sampling)?,
            total_expected,
        );
        let selected_set: std::collections::HashSet<usize> =
            selected_indices.iter().copied().collect();

        let mut found_count = 0usize;

        for i in 0..total_expected {
            let artifact_path = artifacts_dir.join(format!("{:06}", i)).join("output.png");

            if !artifact_path.exists() {
                missing_indices.push(i);
                errors.push(format!("Missing output frame at index {:06}", i));
            } else {
                found_count += 1;

                // For passthrough frames, verify bitwise identity with source frame if available
                if !selected_set.contains(&i) {
                    let src_frame_png = source_frames_dir.join(format!("frame_{:06}.png", i));
                    let src_frame_alt = source_frames_dir.join(format!("{:06}.png", i));
                    let src_path = if src_frame_png.exists() {
                        Some(src_frame_png)
                    } else if src_frame_alt.exists() {
                        Some(src_frame_alt)
                    } else {
                        None
                    };

                    if let Some(src) = src_path {
                        if let (Ok(src_bytes), Ok(art_bytes)) =
                            (fs::read(&src), fs::read(&artifact_path))
                        {
                            if src_bytes != art_bytes {
                                passthrough_mismatches.push(i);
                                warnings.push(format!(
                                    "Passthrough frame {:06} content differed from source frame",
                                    i
                                ));
                            }
                        }
                    }
                }
            }
        }

        let is_valid = errors.is_empty() && missing_indices.is_empty();

        Ok(FrameSequenceValidationReport {
            is_valid,
            total_expected,
            total_found: found_count,
            missing_indices,
            duplicate_indices,
            passthrough_mismatches,
            errors,
            warnings,
        })
    }
}
