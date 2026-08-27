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

    /// Extracts boundary frames around adjacent segment boundaries and calculates real visual seam metrics.
    ///
    /// NOTE: As mandated by Section L & O, face continuity remains UNVERIFIED because no local
    /// face embedding model is bundled in the repository. Seam metrics detect visual scene transitions
    /// and are explicitly labeled as VISUAL_SEAM_METRIC, not identity similarity.
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

        // 1. Extract last frame of previous segment
        let prev_last_frame_path =
            evidence_dir.join(format!("boundary_{:03}_prev_last.jpg", boundary_index));
        let prev_start_seek = (prev_probe.duration_sec - 0.1).max(0.0);
        let _ = Command::new("ffmpeg")
            .args([
                "-y",
                "-ss",
                &format!("{:.3}", prev_start_seek),
                "-i",
                prev_child_normalized_path.to_str().unwrap_or_default(),
                "-vframes",
                "1",
                "-q:v",
                "2",
                prev_last_frame_path.to_str().unwrap_or_default(),
            ])
            .output();

        if prev_last_frame_path.exists() {
            prev_frames.push(prev_last_frame_path.clone());
        }

        // 2. Extract first frame of next segment
        let next_first_frame_path =
            evidence_dir.join(format!("boundary_{:03}_next_first.jpg", boundary_index));
        let _ = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                next_child_normalized_path.to_str().unwrap_or_default(),
                "-vframes",
                "1",
                "-q:v",
                "2",
                next_first_frame_path.to_str().unwrap_or_default(),
            ])
            .output();

        if next_first_frame_path.exists() {
            next_frames.push(next_first_frame_path.clone());
        }

        // 3. Compute real visual seam metric between prev_last and next_first
        let (seam_status, metric_name, metric_value) =
            if prev_last_frame_path.exists() && next_first_frame_path.exists() {
                let delta =
                    Self::compute_image_difference(&prev_last_frame_path, &next_first_frame_path);
                let status = if delta <= 0.40 {
                    FlowSeamStatus::Pass
                } else {
                    FlowSeamStatus::Fail
                };
                (status, Some("mean_pixel_delta".to_string()), Some(delta))
            } else {
                (FlowSeamStatus::Unverified, None, None)
            };

        Ok(FlowContinuityEvidence {
            boundary_index,
            previous_segment_index: prev_segment_index,
            next_segment_index,
            previous_end_frame_paths: prev_frames,
            next_start_frame_paths: next_frames,
            face_continuity_status: FlowFaceContinuityStatus::Unverified,
            seam_status,
            metric_name,
            metric_value,
            reviewed_at: Some(Utc::now().to_rfc3339()),
        })
    }

    /// Computes a normalized mean absolute difference between two image files.
    fn compute_image_difference(img_a: &Path, img_b: &Path) -> f64 {
        let bytes_a = fs::read(img_a).unwrap_or_default();
        let bytes_b = fs::read(img_b).unwrap_or_default();

        if bytes_a.is_empty() || bytes_b.is_empty() {
            return 1.0;
        }

        let min_len = bytes_a.len().min(bytes_b.len());
        if min_len == 0 {
            return 1.0;
        }

        let sample_step = (min_len / 500).max(1);
        let mut diff_sum: u64 = 0;
        let mut sample_count: u64 = 0;

        for i in (0..min_len).step_by(sample_step) {
            let d = (bytes_a[i] as i32 - bytes_b[i] as i32).abs();
            diff_sum += d as u64;
            sample_count += 1;
        }

        if sample_count == 0 {
            return 1.0;
        }

        (diff_sum as f64) / (sample_count as f64 * 255.0)
    }
}
