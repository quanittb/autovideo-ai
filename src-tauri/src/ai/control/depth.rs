use image::{GrayImage, Luma};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Configuration for depth map extraction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DepthExtractorConfig {
    pub target_width: u32,
    pub target_height: u32,
    pub invert: bool,
    pub model_id: String,
    pub model_version: Option<String>,
}

impl Default for DepthExtractorConfig {
    fn default() -> Self {
        Self {
            target_width: 518,
            target_height: 518,
            invert: false,
            model_id: "depth_anything_v2".to_string(),
            model_version: Some("1.0.0".to_string()),
        }
    }
}

impl DepthExtractorConfig {
    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.model_id.as_bytes());
        if let Some(ref v) = self.model_version {
            hasher.update(v.as_bytes());
        }
        hasher.update(&self.target_width.to_le_bytes());
        hasher.update(&self.target_height.to_le_bytes());
        hasher.update(&[self.invert as u8]);
        format!("{:x}", hasher.finalize())
    }
}

/// Result of extracting depth map from a single frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DepthFrameResult {
    pub frame_index: usize,
    pub min_depth: f32,
    pub max_depth: f32,
    pub mean_depth: f32,
    pub artifact_path: PathBuf,
    pub duration_ms: f64,
    pub is_reused: bool,
}

pub struct DepthExtractor {
    pub config: DepthExtractorConfig,
}

impl DepthExtractor {
    pub fn new(config: DepthExtractorConfig) -> Self {
        Self { config }
    }

    /// Normalizes raw continuous floating-point depth values to an 8-bit grayscale image.
    pub fn normalize_depth_to_image(
        raw_depth: &[f32],
        width: u32,
        height: u32,
        invert: bool,
    ) -> (GrayImage, f32, f32, f32) {
        if raw_depth.is_empty() || width == 0 || height == 0 {
            return (GrayImage::new(width.max(1), height.max(1)), 0.0, 0.0, 0.0);
        }

        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;
        let mut sum_val = 0.0f64;

        for &v in raw_depth {
            if v.is_finite() {
                if v < min_val {
                    min_val = v;
                }
                if v > max_val {
                    max_val = v;
                }
                sum_val += v as f64;
            }
        }

        if min_val == f32::MAX {
            min_val = 0.0;
            max_val = 1.0;
        }

        let range = (max_val - min_val).max(1e-6);
        let mean_val = (sum_val / raw_depth.len() as f64) as f32;

        let mut gray_img = GrayImage::new(width, height);

        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                if idx < raw_depth.len() {
                    let val = raw_depth[idx];
                    let norm = if val.is_finite() {
                        ((val - min_val) / range).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };

                    let byte_val = if invert {
                        ((1.0 - norm) * 255.0).round() as u8
                    } else {
                        (norm * 255.0).round() as u8
                    };

                    gray_img.put_pixel(x, y, Luma([byte_val]));
                }
            }
        }

        (gray_img, min_val, max_val, mean_val)
    }

    /// Extracts and saves depth map artifact to disk.
    pub fn extract_frame(
        &self,
        frame_index: usize,
        raw_depth: &[f32],
        width: u32,
        height: u32,
        output_path: &Path,
    ) -> Result<DepthFrameResult, AppError> {
        let start = std::time::Instant::now();

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::storage_error(
                    format!("Failed to create depth directory: {}", parent.display()),
                    e.to_string(),
                )
            })?;
        }

        let (depth_img, min_depth, max_depth, mean_depth) =
            Self::normalize_depth_to_image(raw_depth, width, height, self.config.invert);

        depth_img.save(output_path).map_err(|e| {
            AppError::storage_error(
                format!("Failed to save depth artifact: {}", output_path.display()),
                e.to_string(),
            )
        })?;

        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok(DepthFrameResult {
            frame_index,
            min_depth,
            max_depth,
            mean_depth,
            artifact_path: output_path.to_path_buf(),
            duration_ms,
            is_reused: false,
        })
    }
}
