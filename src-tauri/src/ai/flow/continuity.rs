use super::manifest::{
    FlowContinuityEvidence, FlowFaceContinuityStatus, FlowIdentityContinuityStrategy,
    FlowSeamStatus,
};
use crate::ai::cloud::spec::SourceMediaProbe;
use chrono::Utc;
use std::fs;
use std::path::Path;
use std::process::Command;

pub struct FlowContinuityManager;

impl FlowContinuityManager {
    /// Strategy default for FLOW-P4-A: same prompt baseline (best effort only).
    pub const DEFAULT_STRATEGY: FlowIdentityContinuityStrategy =
        FlowIdentityContinuityStrategy::SamePromptBaseline;

    /// Extracts boundary frames around adjacent segment boundaries (-250ms, -100ms, last, first, +100ms, +250ms),
    /// generates a contact sheet, and computes true decoded-pixel visual seam metrics.
    ///
    /// NOTE: As mandated by Section L & O, face continuity remains UNVERIFIED because no local
    /// face embedding model is bundled in the repository. Seam metrics detect visual scene transitions
    /// on decoded raw pixels and are explicitly categorized as VISUAL_SEAM_METRIC, not identity similarity.
    pub fn extract_boundary_evidence(
        boundary_index: usize,
        prev_child_normalized_path: &Path,
        prev_segment_index: usize,
        next_child_normalized_path: &Path,
        next_segment_index: usize,
        evidence_dir: &Path,
    ) -> Result<FlowContinuityEvidence, String> {
        fs::create_dir_all(evidence_dir)
            .map_err(|e| format!("Failed to create continuity evidence dir: {}", e))?;

        let prev_probe =
            SourceMediaProbe::probe_file(prev_child_normalized_path).map_err(|e| e.to_string())?;
        let _next_probe =
            SourceMediaProbe::probe_file(next_child_normalized_path).map_err(|e| e.to_string())?;

        let mut prev_frames = Vec::new();
        let mut next_frames = Vec::new();

        // 1. Previous segment frames (-250ms, -100ms, last frame)
        let prev_dur = prev_probe.duration_sec;
        let t_prev_m250 = (prev_dur - 0.250).max(0.0);
        let t_prev_m100 = (prev_dur - 0.100).max(0.0);
        let t_prev_last = (prev_dur - 0.050).max(0.0);

        let p_m250 = evidence_dir.join(format!("boundary_{:03}_prev_m250.jpg", boundary_index));
        let p_m100 = evidence_dir.join(format!("boundary_{:03}_prev_m100.jpg", boundary_index));
        let p_last = evidence_dir.join(format!("boundary_{:03}_prev_last.jpg", boundary_index));

        Self::extract_single_frame(prev_child_normalized_path, t_prev_m250, &p_m250)?;
        Self::extract_single_frame(prev_child_normalized_path, t_prev_m100, &p_m100)?;
        Self::extract_single_frame(prev_child_normalized_path, t_prev_last, &p_last)?;

        prev_frames.push(p_m250.clone());
        prev_frames.push(p_m100.clone());
        prev_frames.push(p_last.clone());

        // 2. Next segment frames (first frame, +100ms, +250ms)
        let next_dur = _next_probe.duration_sec;
        let n_first = evidence_dir.join(format!("boundary_{:03}_next_first.jpg", boundary_index));
        let n_p100 = evidence_dir.join(format!("boundary_{:03}_next_p100.jpg", boundary_index));
        let n_p250 = evidence_dir.join(format!("boundary_{:03}_next_p250.jpg", boundary_index));

        let t_next_p100 = 0.100f64.min((next_dur - 0.050).max(0.0));
        let t_next_p250 = 0.250f64.min((next_dur - 0.050).max(0.0));

        Self::extract_single_frame(next_child_normalized_path, 0.0, &n_first)?;
        Self::extract_single_frame(next_child_normalized_path, t_next_p100, &n_p100)?;
        Self::extract_single_frame(next_child_normalized_path, t_next_p250, &n_p250)?;

        next_frames.push(n_first.clone());
        next_frames.push(n_p100.clone());
        next_frames.push(n_p250.clone());

        // 3. Generate Contact Sheet (Section 6)
        let contact_sheet_path =
            evidence_dir.join(format!("boundary_{:03}_contact_sheet.jpg", boundary_index));
        let _ = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                p_m250.to_str().unwrap_or_default(),
                "-i",
                p_m100.to_str().unwrap_or_default(),
                "-i",
                p_last.to_str().unwrap_or_default(),
                "-i",
                n_first.to_str().unwrap_or_default(),
                "-i",
                n_p100.to_str().unwrap_or_default(),
                "-i",
                n_p250.to_str().unwrap_or_default(),
                "-filter_complex",
                "[0:v][1:v][2:v]hstack=inputs=3[top];[3:v][4:v][5:v]hstack=inputs=3[bottom];[top][bottom]vstack=inputs=2[v]",
                "-map",
                "[v]",
                "-vframes",
                "1",
                contact_sheet_path.to_str().unwrap_or_default(),
            ])
            .output();

        // 4. True decoded pixel difference metric (Section 4 & 5)
        let (seam_status, metric_name, metric_category, metric_value) =
            if p_last.exists() && n_first.exists() {
                match Self::compute_decoded_pixel_delta(&p_last, &n_first) {
                    Ok(delta) => (
                        FlowSeamStatus::Unverified, // Section 5: Uncalibrated threshold removed; status is Unverified
                        Some("mean_pixel_delta".to_string()),
                        Some("VISUAL_SEAM_METRIC".to_string()),
                        Some(delta),
                    ),
                    Err(_) => (FlowSeamStatus::Unverified, None, None, None),
                }
            } else {
                (FlowSeamStatus::Unverified, None, None, None)
            };

        Ok(FlowContinuityEvidence {
            boundary_index,
            previous_segment_index: prev_segment_index,
            next_segment_index,
            previous_end_frame_paths: prev_frames,
            next_start_frame_paths: next_frames,
            contact_sheet_path: if contact_sheet_path.exists() {
                Some(contact_sheet_path)
            } else {
                None
            },
            face_continuity_status: FlowFaceContinuityStatus::Unverified,
            seam_status,
            metric_name,
            metric_category,
            metric_value,
            reviewed_at: Some(Utc::now().to_rfc3339()),
        })
    }

    fn extract_single_frame(
        video_path: &Path,
        timestamp_sec: f64,
        out_path: &Path,
    ) -> Result<(), String> {
        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-ss",
                &format!("{:.3}", timestamp_sec),
                "-i",
                video_path.to_str().unwrap_or_default(),
                "-vframes",
                "1",
                "-pix_fmt",
                "yuvj420p",
                "-strict",
                "unofficial",
                "-q:v",
                "2",
                out_path.to_str().unwrap_or_default(),
            ])
            .output()
            .map_err(|e| format!("FFmpeg failed to extract frame: {}", e))?;

        if !output.status.success() || !out_path.exists() {
            // Fallback: slow precise decode by placing -ss after -i
            let fb = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-i",
                    video_path.to_str().unwrap_or_default(),
                    "-ss",
                    &format!("{:.3}", timestamp_sec),
                    "-vframes",
                    "1",
                    "-pix_fmt",
                    "yuvj420p",
                    "-strict",
                    "unofficial",
                    "-q:v",
                    "2",
                    out_path.to_str().unwrap_or_default(),
                ])
                .output();

            if let Ok(res) = fb {
                if res.status.success() && out_path.exists() {
                    return Ok(());
                }
            }

            return Err(format!(
                "FFmpeg frame extraction failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    /// Decodes image to raw 256x256 grayscale pixel bytes via FFmpeg and calculates
    /// mean absolute decoded pixel difference.
    ///
    /// Metric formula: sum(|pixelA - pixelB|) / (pixelCount * 255.0)
    pub fn compute_decoded_pixel_delta(img_a: &Path, img_b: &Path) -> Result<f64, String> {
        let pixels_a = Self::decode_frame_grayscale(img_a, 256, 256)?;
        let pixels_b = Self::decode_frame_grayscale(img_b, 256, 256)?;

        if pixels_a.is_empty() || pixels_b.is_empty() || pixels_a.len() != pixels_b.len() {
            return Err(
                "PIXEL_DECODE_MISMATCH: Decoded pixel buffers are empty or dimension mismatched"
                    .to_string(),
            );
        }

        let mut diff_sum: u64 = 0;
        for (a, b) in pixels_a.iter().zip(pixels_b.iter()) {
            diff_sum += (*a as i32 - *b as i32).abs() as u64;
        }

        let total_pixels = pixels_a.len() as f64;
        let mean_pixel_delta = (diff_sum as f64) / (total_pixels * 255.0);
        Ok(mean_pixel_delta)
    }

    /// Helper that decodes a frame into raw 8-bit grayscale pixel samples at the specified dimensions.
    pub fn decode_frame_grayscale(
        frame_path: &Path,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, String> {
        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                frame_path.to_str().unwrap_or_default(),
                "-vf",
                &format!("scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black,format=gray", width, height, width, height),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "gray",
                "-",
            ])
            .output()
            .map_err(|e| format!("FFmpeg failed to decode frame: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "FFmpeg frame decode failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(output.stdout)
    }
}
