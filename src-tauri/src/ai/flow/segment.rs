use super::capability::FlowCapabilityPolicy;
use super::manifest::{
    FlowChildSegmentRecord, FlowChildSubmissionState, FlowJobState, FlowSegmentPlan,
};
use crate::ai::cloud::job::JobTimestamps;
use crate::ai::cloud::manifest::SegmentBoundary;
use crate::ai::cloud::spec::SourceMediaFacts;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;

pub struct FlowVideoSegmenter;

impl FlowVideoSegmenter {
    pub fn plan_segments(
        facts: &SourceMediaFacts,
        policy: &FlowCapabilityPolicy,
    ) -> Result<FlowSegmentPlan, String> {
        let fps = facts.fps;
        if fps <= 0.0 || !fps.is_finite() {
            return Err("INVALID_FPS: Source video FPS must be positive and finite".to_string());
        }

        let total_duration_sec = facts.duration_sec;
        if total_duration_sec <= 0.0 {
            return Err("INVALID_DURATION: Source video duration must be positive".to_string());
        }

        let max_sec = policy.max_edit_segment_duration_sec;
        if max_sec <= 0.0 {
            return Err(
                "INVALID_CAPABILITY: Capability duration limit must be positive".to_string(),
            );
        }

        let total_frames = ((total_duration_sec * fps).round() as u64).max(1);
        let max_frames_per_segment = ((max_sec * fps).floor() as u64).max(1);

        let mut segments = Vec::new();
        let mut start_frame: u64 = 0;
        let mut index: usize = 0;

        while start_frame < total_frames {
            // Pick largest legal frame count up to max_frames_per_segment
            let frames_left = total_frames - start_frame;
            let current_segment_frames = frames_left.min(max_frames_per_segment);
            let end_frame = start_frame + current_segment_frames;

            let start_sec = start_frame as f64 / fps;
            let end_sec = end_frame as f64 / fps;

            let start_pts = (start_sec * 1000.0).round() as u64;
            let end_pts = (end_sec * 1000.0).round() as u64;
            let start_ms = start_pts;
            let end_ms = end_pts;
            let expected_duration_sec = (end_sec - start_sec).max(0.001);

            segments.push(SegmentBoundary {
                index,
                start_frame,
                end_frame,
                start_pts,
                end_pts,
                start_ms,
                end_ms,
                expected_duration_sec,
                start_sec,
                end_sec,
            });

            start_frame = end_frame;
            index += 1;
        }

        Ok(FlowSegmentPlan {
            segments,
            total_frames,
            total_duration_sec,
            target_fps: fps,
            capability_limit_sec: max_sec,
        })
    }

    pub fn split_and_prepare_segments(
        source_path: &Path,
        facts: &SourceMediaFacts,
        plan: &FlowSegmentPlan,
        output_dir: &Path,
    ) -> Result<Vec<FlowChildSegmentRecord>, String> {
        fs::create_dir_all(output_dir)
            .map_err(|e| format!("Failed to create segments directory: {}", e))?;

        let mut child_records = Vec::new();
        let now = Utc::now().to_rfc3339();

        for seg in &plan.segments {
            let seg_filename = format!("flow_segment_{:03}.mp4", seg.index);
            let seg_path = output_dir.join(&seg_filename);

            let start_sec = seg.start_frame as f64 / plan.target_fps;
            let duration_sec = seg.expected_duration_sec;

            // Audio preservation policy: Include source AAC audio if source has audio
            let mut cmd = Command::new("ffmpeg");
            cmd.arg("-y")
                .arg("-ss")
                .arg(format!("{:.6}", start_sec))
                .arg("-i")
                .arg(source_path)
                .arg("-t")
                .arg(format!("{:.6}", duration_sec))
                .arg("-c:v")
                .arg("libx264")
                .arg("-preset")
                .arg("fast")
                .arg("-crf")
                .arg("18")
                .arg("-pix_fmt")
                .arg("yuv420p")
                .arg("-r")
                .arg(format!("{:.4}", plan.target_fps));

            if facts.has_audio {
                cmd.arg("-c:a").arg("aac").arg("-b:a").arg("192k");
            } else {
                cmd.arg("-an");
            }

            cmd.arg(&seg_path);

            let output = cmd
                .output()
                .map_err(|e| format!("FFmpeg segment split execution failed: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!(
                    "FFmpeg failed splitting segment {}: {}",
                    seg.index, stderr
                ));
            }

            if !seg_path.exists() {
                return Err(format!(
                    "Segment file missing after FFmpeg split: {:?}",
                    seg_path
                ));
            }

            let seg_bytes =
                fs::read(&seg_path).map_err(|e| format!("Failed to read split segment: {}", e))?;
            let mut hasher = Sha256::new();
            hasher.update(&seg_bytes);
            let seg_sha256 = format!("{:x}", hasher.finalize());

            child_records.push(FlowChildSegmentRecord {
                segment_index: seg.index,
                segment_file_name: seg_filename,
                segment_sha256: seg_sha256,
                start_frame: seg.start_frame,
                end_frame: seg.end_frame,
                start_pts: seg.start_pts,
                end_pts: seg.end_pts,
                duration_sec: seg.expected_duration_sec,
                state: FlowJobState::ReadyToSubmit,
                submission_state: FlowChildSubmissionState::NeverAttempted,
                local_submission_attempt_id: None,
                submission_evidence: None,
                download_artifact_path: None,
                download_artifact_sha: None,
                timestamps: JobTimestamps {
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    submitted_at: None,
                    completed_at: None,
                },
            });
        }

        Ok(child_records)
    }
}
