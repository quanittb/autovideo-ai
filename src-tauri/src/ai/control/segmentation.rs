use image::{GrayImage, Luma};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Configuration for subject/background segmentation extraction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SegmentationExtractorConfig {
    pub target_width: u32,
    pub target_height: u32,
    pub threshold: f32,
    pub binary_mask: bool,
    pub model_id: String,
    pub model_version: Option<String>,
}

impl Default for SegmentationExtractorConfig {
    fn default() -> Self {
        Self {
            target_width: 1024,
            target_height: 1024,
            threshold: 0.5,
            binary_mask: false,
            model_id: "birefnet".to_string(),
            model_version: Some("1.0.0".to_string()),
        }
    }
}

impl SegmentationExtractorConfig {
    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.model_id.as_bytes());
        if let Some(ref v) = self.model_version {
            hasher.update(v.as_bytes());
        }
        hasher.update(&self.target_width.to_le_bytes());
        hasher.update(&self.target_height.to_le_bytes());
        hasher.update(&self.threshold.to_le_bytes());
        hasher.update(&[self.binary_mask as u8]);
        format!("{:x}", hasher.finalize())
    }
}

/// Result of extracting subject segmentation mask from a single frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SegmentationFrameResult {
    pub frame_index: usize,
    pub foreground_ratio: f32,
    pub mean_probability: f32,
    pub artifact_path: PathBuf,
    pub duration_ms: f64,
    pub is_reused: bool,
}

pub struct SegmentationExtractor {
    pub config: SegmentationExtractorConfig,
}

impl SegmentationExtractor {
    pub fn new(config: SegmentationExtractorConfig) -> Self {
        Self { config }
    }

    /// Converts raw probability values into an alpha mask image.
    pub fn probabilities_to_mask(
        raw_probs: &[f32],
        width: u32,
        height: u32,
        threshold: f32,
        binary_mask: bool,
    ) -> (GrayImage, f32, f32) {
        if raw_probs.is_empty() || width == 0 || height == 0 {
            return (GrayImage::new(width.max(1), height.max(1)), 0.0, 0.0);
        }

        let mut gray_img = GrayImage::new(width, height);
        let mut fg_count = 0u64;
        let mut sum_prob = 0.0f64;

        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                if idx < raw_probs.len() {
                    let prob = raw_probs[idx].clamp(0.0, 1.0);
                    sum_prob += prob as f64;

                    let byte_val = if binary_mask {
                        if prob >= threshold {
                            fg_count += 1;
                            255u8
                        } else {
                            0u8
                        }
                    } else {
                        if prob >= threshold {
                            fg_count += 1;
                        }
                        (prob * 255.0).round() as u8
                    };

                    gray_img.put_pixel(x, y, Luma([byte_val]));
                }
            }
        }

        let total_pixels = (width * height) as f32;
        let fg_ratio = if total_pixels > 0.0 {
            fg_count as f32 / total_pixels
        } else {
            0.0
        };

        let mean_prob = if total_pixels > 0.0 {
            (sum_prob / total_pixels as f64) as f32
        } else {
            0.0
        };

        (gray_img, fg_ratio, mean_prob)
    }

    /// Extracts and saves segmentation mask artifact to disk.
    pub fn extract_frame(
        &self,
        frame_index: usize,
        raw_probs: &[f32],
        width: u32,
        height: u32,
        output_path: &Path,
    ) -> Result<SegmentationFrameResult, AppError> {
        let start = std::time::Instant::now();

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::storage_error(
                    format!("Failed to create mask directory: {}", parent.display()),
                    e.to_string(),
                )
            })?;
        }

        let (mask_img, fg_ratio, mean_prob) = Self::probabilities_to_mask(
            raw_probs,
            width,
            height,
            self.config.threshold,
            self.config.binary_mask,
        );

        mask_img.save(output_path).map_err(|e| {
            AppError::storage_error(
                format!("Failed to save mask artifact: {}", output_path.display()),
                e.to_string(),
            )
        })?;

        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok(SegmentationFrameResult {
            frame_index,
            foreground_ratio: fg_ratio,
            mean_probability: mean_prob,
            artifact_path: output_path.to_path_buf(),
            duration_ms,
            is_reused: false,
        })
    }
}
