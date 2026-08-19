use image::{Rgb, RgbImage};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Configuration for pose extraction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PoseExtractorConfig {
    pub target_width: u32,
    pub target_height: u32,
    pub confidence_threshold: f32,
    pub include_hands: bool,
    pub include_face: bool,
    pub model_id: String,
    pub model_version: Option<String>,
}

impl Default for PoseExtractorConfig {
    fn default() -> Self {
        Self {
            target_width: 384,
            target_height: 288,
            confidence_threshold: 0.3,
            include_hands: true,
            include_face: true,
            model_id: "dwpose".to_string(),
            model_version: Some("1.0.0".to_string()),
        }
    }
}

impl PoseExtractorConfig {
    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.model_id.as_bytes());
        if let Some(ref v) = self.model_version {
            hasher.update(v.as_bytes());
        }
        hasher.update(&self.target_width.to_le_bytes());
        hasher.update(&self.target_height.to_le_bytes());
        hasher.update(&self.confidence_threshold.to_le_bytes());
        hasher.update(&[self.include_hands as u8, self.include_face as u8]);
        format!("{:x}", hasher.finalize())
    }
}

/// 2D Keypoint with coordinates and confidence score.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Keypoint2D {
    pub x: f32,
    pub y: f32,
    pub score: f32,
}

/// Result of extracting pose from a single frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PoseFrameResult {
    pub frame_index: usize,
    pub keypoints_detected: usize,
    pub artifact_path: PathBuf,
    pub duration_ms: f64,
    pub is_reused: bool,
}

/// Standard OpenPose limb pairs (indices connecting body keypoints).
pub const BODY_LIMBS: [(usize, usize, [u8; 3]); 17] = [
    (0, 1, [255, 0, 0]),     // nose -> neck
    (1, 2, [255, 85, 0]),    // neck -> r_shoulder
    (2, 3, [255, 170, 0]),   // r_shoulder -> r_elbow
    (3, 4, [255, 255, 0]),   // r_elbow -> r_wrist
    (1, 5, [170, 255, 0]),   // neck -> l_shoulder
    (5, 6, [85, 255, 0]),    // l_shoulder -> l_elbow
    (6, 7, [0, 255, 0]),     // l_elbow -> l_wrist
    (1, 8, [0, 255, 85]),    // neck -> r_hip
    (8, 9, [0, 255, 170]),   // r_hip -> r_knee
    (9, 10, [0, 255, 255]),  // r_knee -> r_ankle
    (1, 11, [0, 170, 255]),  // neck -> l_hip
    (11, 12, [0, 85, 255]),  // l_hip -> l_knee
    (12, 13, [0, 0, 255]),   // l_knee -> l_ankle
    (0, 14, [85, 0, 255]),   // nose -> r_eye
    (14, 16, [170, 0, 255]), // r_eye -> r_ear
    (0, 15, [255, 0, 255]),  // nose -> l_eye
    (15, 17, [255, 0, 170]), // l_eye -> l_ear
];

pub struct PoseExtractor {
    pub config: PoseExtractorConfig,
}

impl PoseExtractor {
    pub fn new(config: PoseExtractorConfig) -> Self {
        Self { config }
    }

    /// Renders detected 2D keypoints and skeletal limbs onto a black RGB canvas.
    pub fn render_skeleton(
        width: u32,
        height: u32,
        keypoints: &[Keypoint2D],
        threshold: f32,
    ) -> RgbImage {
        let mut canvas = RgbImage::new(width, height);

        // Draw limb connections
        for &(p1_idx, p2_idx, color) in &BODY_LIMBS {
            if p1_idx < keypoints.len() && p2_idx < keypoints.len() {
                let p1 = &keypoints[p1_idx];
                let p2 = &keypoints[p2_idx];
                if p1.score >= threshold && p2.score >= threshold {
                    Self::draw_line(
                        &mut canvas,
                        (p1.x * width as f32) as i32,
                        (p1.y * height as f32) as i32,
                        (p2.x * width as f32) as i32,
                        (p2.y * height as f32) as i32,
                        Rgb(color),
                    );
                }
            }
        }

        // Draw keypoint joints
        for (i, p) in keypoints.iter().enumerate() {
            if p.score >= threshold {
                let px = (p.x * width as f32) as i32;
                let py = (p.y * height as f32) as i32;
                let joint_color = if i < BODY_LIMBS.len() {
                    Rgb(BODY_LIMBS[i % BODY_LIMBS.len()].2)
                } else {
                    Rgb([255, 255, 255])
                };
                Self::draw_circle(&mut canvas, px, py, 2, joint_color);
            }
        }

        canvas
    }

    /// Helper: Bresenham line rasterization.
    fn draw_line(img: &mut RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb<u8>) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;

        let mut x = x0;
        let mut y = y0;

        let (w, h) = (img.width() as i32, img.height() as i32);

        loop {
            if x >= 0 && x < w && y >= 0 && y < h {
                img.put_pixel(x as u32, y as u32, color);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Helper: Fill small circle for joint keypoints.
    fn draw_circle(img: &mut RgbImage, cx: i32, cy: i32, radius: i32, color: Rgb<u8>) {
        let (w, h) = (img.width() as i32, img.height() as i32);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && px < w && py >= 0 && py < h {
                        img.put_pixel(px as u32, py as u32, color);
                    }
                }
            }
        }
    }

    /// Extracts and saves pose skeleton frame to disk.
    pub fn extract_frame(
        &self,
        frame_index: usize,
        width: u32,
        height: u32,
        keypoints: &[Keypoint2D],
        output_path: &Path,
    ) -> Result<PoseFrameResult, AppError> {
        let start = std::time::Instant::now();

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::storage_error(
                    format!("Failed to create pose directory: {}", parent.display()),
                    e.to_string(),
                )
            })?;
        }

        let skeleton =
            Self::render_skeleton(width, height, keypoints, self.config.confidence_threshold);

        skeleton.save(output_path).map_err(|e| {
            AppError::storage_error(
                format!(
                    "Failed to save pose frame artifact: {}",
                    output_path.display()
                ),
                e.to_string(),
            )
        })?;

        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        let detected = keypoints
            .iter()
            .filter(|k| k.score >= self.config.confidence_threshold)
            .count();

        Ok(PoseFrameResult {
            frame_index,
            keypoints_detected: detected,
            artifact_path: output_path.to_path_buf(),
            duration_ms,
            is_reused: false,
        })
    }
}
