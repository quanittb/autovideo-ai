use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{AppError, ErrorCode};

/// Temporal sliding-window configuration for multi-frame diffusion synthesis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemporalConfig {
    pub context_size: usize,
    pub overlap: usize,
    pub enable_seam_blending: bool,
    pub enable_latent_continuity: bool,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            context_size: 16,
            overlap: 4,
            enable_seam_blending: true,
            enable_latent_continuity: true,
        }
    }
}

impl TemporalConfig {
    /// Validates temporal sliding window parameters.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.context_size == 0 {
            return Err(AppError::invalid_input(
                "context_size must be greater than 0",
            ));
        }
        if self.overlap >= self.context_size {
            return Err(AppError::invalid_input(format!(
                "overlap ({}) must be strictly less than context_size ({})",
                self.overlap, self.context_size
            )));
        }
        Ok(())
    }

    /// Calculates window stride (step size between successive sliding windows).
    pub fn stride(&self) -> usize {
        self.context_size.saturating_sub(self.overlap).max(1)
    }

    /// Computes deterministic hash of temporal configuration.
    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!(
            "temporal:ctx={}:ov={}:blend={}:latent={}",
            self.context_size,
            self.overlap,
            self.enable_seam_blending,
            self.enable_latent_continuity
        ));
        format!("{:x}", hasher.finalize())
    }
}

/// Metadata descriptor for a single scheduled temporal diffusion window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TemporalWindow {
    pub window_index: usize,
    pub start_frame: usize,
    pub end_frame: usize, // Exclusive upper bound
    pub is_first: bool,
    pub is_last: bool,
    pub overlap_with_previous: usize,
    pub overlap_with_next: usize,
}

impl TemporalWindow {
    /// Number of total frames in this window.
    pub fn frame_count(&self) -> usize {
        self.end_frame.saturating_sub(self.start_frame)
    }

    /// List of frame indices covered by this window.
    pub fn frame_indices(&self) -> Vec<usize> {
        (self.start_frame..self.end_frame).collect()
    }
}

/// Manifest for a completed temporal window artifact folder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowArtifactManifest {
    pub window_index: usize,
    pub start_frame: usize,
    pub end_frame: usize,
    pub frame_count: usize,
    pub frame_paths: Vec<PathBuf>,
    pub window_hash: String,
    pub is_completed: bool,
    pub generation_duration_ms: f64,
}

impl WindowArtifactManifest {
    pub fn save_to_file(&self, path: &Path) -> Result<(), AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::storage_error("Failed to create window artifact directory", e.to_string())
            })?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            AppError::storage_error("Failed to serialize WindowArtifactManifest", e.to_string())
        })?;
        let mut file = File::create(path).map_err(|e| {
            AppError::storage_error(
                format!("Failed to create window manifest file: {}", path.display()),
                e.to_string(),
            )
        })?;
        file.write_all(json.as_bytes()).map_err(|e| {
            AppError::storage_error(
                format!("Failed to write window manifest file: {}", path.display()),
                e.to_string(),
            )
        })?;
        Ok(())
    }

    pub fn load_from_file(path: &Path) -> Result<Self, AppError> {
        if !path.exists() {
            return Err(AppError::file_not_found(path.display().to_string()));
        }
        let json = fs::read_to_string(path).map_err(|e| {
            AppError::storage_error(
                format!("Failed to read window manifest file: {}", path.display()),
                e.to_string(),
            )
        })?;
        let manifest: Self = serde_json::from_str(&json).map_err(|e| {
            AppError::storage_error(
                "Failed to deserialize WindowArtifactManifest",
                e.to_string(),
            )
        })?;
        Ok(manifest)
    }

    /// Verifies that all declared frame PNGs exist and are non-empty.
    pub fn validate_frames_exist(&self) -> bool {
        if !self.is_completed || self.frame_paths.len() != self.frame_count {
            return false;
        }
        for frame_path in &self.frame_paths {
            if !frame_path.exists() {
                return false;
            }
            if let Ok(meta) = fs::metadata(frame_path) {
                if meta.len() == 0 {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
}

/// Schedules temporal sliding windows across arbitrary source frame sequences.
pub struct TemporalWindowSlicer;

impl TemporalWindowSlicer {
    /// Slices total_frames into a deterministic sequence of overlapping TemporalWindows.
    pub fn slice_windows(
        total_frames: usize,
        config: &TemporalConfig,
    ) -> Result<Vec<TemporalWindow>, AppError> {
        config.validate()?;

        if total_frames == 0 {
            return Ok(Vec::new());
        }

        // Case 1: Video is shorter than or equal to context_size
        if total_frames <= config.context_size {
            return Ok(vec![TemporalWindow {
                window_index: 0,
                start_frame: 0,
                end_frame: total_frames,
                is_first: true,
                is_last: true,
                overlap_with_previous: 0,
                overlap_with_next: 0,
            }]);
        }

        // Case 2: Multi-window sliding generation
        let stride = config.stride();
        let mut windows = Vec::new();
        let mut start = 0;
        let mut window_index = 0;

        while start < total_frames {
            let end = (start + config.context_size).min(total_frames);
            let is_first = window_index == 0;
            let is_last = end == total_frames;

            let prev_overlap = if is_first {
                0
            } else {
                config.overlap.min(config.context_size)
            };

            let next_overlap = if is_last {
                0
            } else {
                config.overlap.min(config.context_size)
            };

            windows.push(TemporalWindow {
                window_index,
                start_frame: start,
                end_frame: end,
                is_first,
                is_last,
                overlap_with_previous: prev_overlap,
                overlap_with_next: next_overlap,
            });

            if is_last {
                break;
            }

            // Advance start by stride
            start += stride;
            window_index += 1;

            // Ensure last window always captures trailing frames if step lands before total_frames
            if start < total_frames
                && (start + config.context_size) > total_frames
                && (start + stride) < total_frames
            {
                // If remaining frames would be smaller than overlap, adjust start to fit context_size
                start = total_frames.saturating_sub(config.context_size);
            }
        }

        Ok(windows)
    }

    /// Computes unique deterministic hash for a specific temporal window.
    pub fn compute_window_hash(
        source_hash: &str,
        control_package_hash: &str,
        char_ref_hash: &str,
        prompt_hash: &str,
        model_hash: &str,
        window: &TemporalWindow,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!(
            "w:{}:{}:{}:{}:{}:{}:{}:{}",
            source_hash,
            control_package_hash,
            char_ref_hash,
            prompt_hash,
            model_hash,
            window.window_index,
            window.start_frame,
            window.end_frame,
        ));
        format!("{:x}", hasher.finalize())
    }
}

/// Performs smooth cosine-weighted temporal cross-fading between overlapping generated frames.
pub struct TemporalBlender;

impl TemporalBlender {
    /// Calculates cosine blend weights for a given overlap length N:
    /// returns alpha in range [0.0..1.0] where alpha represents the weight of the NEW window.
    pub fn compute_cosine_weights(overlap_count: usize) -> Vec<f32> {
        if overlap_count == 0 {
            return Vec::new();
        }
        if overlap_count == 1 {
            return vec![0.5];
        }

        let mut weights = Vec::with_capacity(overlap_count);
        for i in 0..overlap_count {
            // Normalized position in [0.0, 1.0]
            let t = (i as f32 + 1.0) / (overlap_count as f32 + 1.0);
            // Smooth cosine transition
            let alpha = 0.5 * (1.0 - (std::f32::consts::PI * t).cos());
            weights.push(alpha.clamp(0.0, 1.0));
        }
        weights
    }

    /// Blends two 8-bit RGB images using alpha factor (0.0 = 100% img1, 1.0 = 100% img2).
    pub fn blend_rgb_images(
        img1: &image::RgbImage,
        img2: &image::RgbImage,
        alpha: f32,
    ) -> Result<image::RgbImage, AppError> {
        let (w1, h1) = img1.dimensions();
        let (w2, h2) = img2.dimensions();

        if w1 != w2 || h1 != h2 {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "Cannot blend images with mismatched dimensions: {}x{} vs {}x{}",
                    w1, h1, w2, h2
                ),
            ));
        }

        let alpha = alpha.clamp(0.0, 1.0);
        let inv_alpha = 1.0 - alpha;

        let mut blended = image::RgbImage::new(w1, h1);

        for y in 0..h1 {
            for x in 0..w1 {
                let p1 = img1.get_pixel(x, y);
                let p2 = img2.get_pixel(x, y);

                let r = ((p1[0] as f32 * inv_alpha) + (p2[0] as f32 * alpha))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                let g = ((p1[1] as f32 * inv_alpha) + (p2[1] as f32 * alpha))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                let b = ((p1[2] as f32 * inv_alpha) + (p2[2] as f32 * alpha))
                    .round()
                    .clamp(0.0, 255.0) as u8;

                blended.put_pixel(x, y, image::Rgb([r, g, b]));
            }
        }

        Ok(blended)
    }

    /// Assembles an array of generated window manifests into a continuous, blended sequence of frames.
    pub fn assemble_and_blend_windows(
        windows: &[TemporalWindow],
        manifests: &[WindowArtifactManifest],
        total_frames: usize,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, AppError> {
        if windows.len() != manifests.len() {
            return Err(AppError::invalid_input(
                "Windows and manifests count mismatch",
            ));
        }

        fs::create_dir_all(output_dir).map_err(|e| {
            AppError::storage_error("Failed to create blended output directory", e.to_string())
        })?;

        let mut master_frames: Vec<Option<PathBuf>> = vec![None; total_frames];

        for (idx, (window, manifest)) in windows.iter().zip(manifests.iter()).enumerate() {
            let frame_paths = &manifest.frame_paths;

            if idx == 0 {
                // First window: copy all non-overlapping or full frames
                for (local_i, &global_i) in window.frame_indices().iter().enumerate() {
                    if local_i < frame_paths.len() && global_i < total_frames {
                        let out_path = output_dir.join(format!("frame_{:06}.png", global_i));
                        let src_frame = &frame_paths[local_i];
                        if src_frame != &out_path {
                            fs::copy(src_frame, &out_path).map_err(|e| {
                                AppError::storage_error(
                                    "Failed to copy master frame",
                                    e.to_string(),
                                )
                            })?;
                        }
                        master_frames[global_i] = Some(out_path);
                    }
                }
            } else {
                let overlap = window.overlap_with_previous;
                let weights = Self::compute_cosine_weights(overlap);

                for (local_i, &global_i) in window.frame_indices().iter().enumerate() {
                    if global_i >= total_frames || local_i >= frame_paths.len() {
                        continue;
                    }

                    let out_path = output_dir.join(format!("frame_{:06}.png", global_i));

                    if local_i < overlap && master_frames[global_i].is_some() {
                        // Blend with previous frame
                        let prev_path = master_frames[global_i].as_ref().unwrap();
                        let prev_img =
                            image::open(prev_path).map(|im| im.to_rgb8()).map_err(|e| {
                                AppError::storage_error(
                                    "Failed to open previous frame for blending",
                                    e.to_string(),
                                )
                            })?;
                        let next_img = image::open(&frame_paths[local_i])
                            .map(|im| im.to_rgb8())
                            .map_err(|e| {
                                AppError::storage_error(
                                    "Failed to open new frame for blending",
                                    e.to_string(),
                                )
                            })?;

                        let alpha = weights.get(local_i).cloned().unwrap_or(0.5);
                        let blended = Self::blend_rgb_images(&prev_img, &next_img, alpha)?;
                        blended.save(&out_path).map_err(|e| {
                            AppError::storage_error("Failed to save blended frame", e.to_string())
                        })?;
                        master_frames[global_i] = Some(out_path);
                    } else {
                        // Non-overlapping frame in this window: copy directly
                        let src_frame = &frame_paths[local_i];
                        if src_frame != &out_path {
                            fs::copy(src_frame, &out_path).map_err(|e| {
                                AppError::storage_error("Failed to copy frame", e.to_string())
                            })?;
                        }
                        master_frames[global_i] = Some(out_path);
                    }
                }
            }
        }

        // Validate that every single frame index is populated
        let mut final_paths = Vec::with_capacity(total_frames);
        for (i, p) in master_frames.into_iter().enumerate() {
            match p {
                Some(path) => final_paths.push(path),
                None => {
                    return Err(AppError::new(
                        ErrorCode::ProcessFailed,
                        format!("Missing blended frame at index {}", i),
                    ));
                }
            }
        }

        Ok(final_paths)
    }
}
