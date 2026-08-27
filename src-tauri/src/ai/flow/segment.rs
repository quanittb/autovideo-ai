use super::capability::FlowCapabilityPolicy;
use super::manifest::{
    FlowChildSegmentRecord, FlowChildSubmissionState, FlowIdentityContinuityStrategy, FlowJobState,
    FlowLongVideoPlan, FlowPlannedSegment, FlowRequestedGenerationConfig, FlowSegmentPlan,
};
use crate::ai::cloud::job::JobTimestamps;
use crate::ai::cloud::manifest::SegmentBoundary;
use crate::ai::cloud::spec::{SourceMediaFacts, SourceMediaProbe};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct FlowVideoSegmenter;

impl FlowVideoSegmenter {
    /// Maximum duration allowed for any Flow edit input segment in seconds.
    pub const MAX_SEGMENT_DURATION_SEC: f64 = 10.0;

    /// Plans long video segments with rational CFR frame alignment and full timeline coverage.
    ///
    /// Respects the hard invariant: EVERY FLOW INPUT SEGMENT <= 10.000 seconds.
    /// Uses rational fps (r_frame_rate) to derive max frames per segment:
    /// Example 30fps (30/1): floor(10.0 * 30 / 1) = 300 frames.
    /// Example 29.97fps (30000/1001): floor(10.0 * 30000 / 1001) = 299 frames (300 frames is rejected as 10.010s).
    pub fn plan_long_video(
        parent_job_id: &str,
        project_id: &str,
        source_media_id: Option<&str>,
        source_path: &Path,
        work_dir: &Path,
        transformation_intent: crate::ai::transformation::TransformationIntent,
        identity_mode: crate::ai::transformation::IdentityMode,
        requested_config: FlowRequestedGenerationConfig,
        _submitted_prompt: &str,
        prompt_hash: &str,
        max_segment_sec: f64,
    ) -> Result<FlowLongVideoPlan, String> {
        let max_sec = if max_segment_sec > 0.0 && max_segment_sec <= Self::MAX_SEGMENT_DURATION_SEC
        {
            max_segment_sec
        } else {
            Self::MAX_SEGMENT_DURATION_SEC
        };

        let (mut source_facts, mut timing_facts) =
            SourceMediaProbe::probe_file_detailed(source_path)
                .map_err(|e| format!("FAILED_TO_PROBE_SOURCE: {}", e))?;

        let mut working_proxy_created = false;
        let mut working_proxy_path: Option<PathBuf> = None;
        let mut working_proxy_sha256: Option<String> = None;
        let source_timing_mode = if timing_facts.is_vfr {
            // VFR detected: create deterministic CFR working proxy (Section E)
            fs::create_dir_all(work_dir)
                .map_err(|e| format!("Failed to create work dir: {}", e))?;
            let proxy_path = work_dir.join("working_proxy_cfr.mp4");

            let target_fps = 30.0;
            let output = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-i",
                    source_path.to_str().unwrap_or_default(),
                    "-c:v",
                    "libx264",
                    "-preset",
                    "fast",
                    "-crf",
                    "18",
                    "-pix_fmt",
                    "yuv420p",
                    "-r",
                    &format!("{:.4}", target_fps),
                    "-an",
                    proxy_path.to_str().unwrap_or_default(),
                ])
                .output()
                .map_err(|e| format!("Failed to create CFR working proxy: {}", e))?;

            if !output.status.success() {
                return Err(format!(
                    "FLOW_LONG_VIDEO_VFR_PROXY_FAILED: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            let (p_facts, p_timing) = SourceMediaProbe::probe_file_detailed(&proxy_path)
                .map_err(|e| format!("Failed to probe working proxy: {}", e))?;

            let p_bytes = fs::read(&proxy_path).unwrap_or_default();
            let mut hasher = Sha256::new();
            hasher.update(&p_bytes);
            working_proxy_sha256 = Some(format!("{:x}", hasher.finalize()));
            working_proxy_created = true;
            working_proxy_path = Some(proxy_path);

            source_facts = p_facts;
            timing_facts = p_timing;
            "VFR".to_string()
        } else {
            "CFR".to_string()
        };

        let r_num = timing_facts.r_frame_rate.num.max(1);
        let r_den = timing_facts.r_frame_rate.den.max(1);

        // Derive max frames per segment: floor(max_sec * num / den)
        let total_limit_frames_float = (max_sec * r_num as f64) / (r_den as f64);
        let mut max_frames_per_segment = total_limit_frames_float.floor() as u64;

        // Strictly enforce actual duration <= max_sec
        while max_frames_per_segment > 0
            && ((max_frames_per_segment as f64 * r_den as f64) / (r_num as f64)) > max_sec + 1e-7
        {
            max_frames_per_segment -= 1;
        }
        let max_frames_per_segment = max_frames_per_segment.max(1);

        // Segment Count Authority (Section D):
        let total_frames = timing_facts
            .nb_frames
            .unwrap_or_else(|| {
                (source_facts.duration_sec * (r_num as f64 / r_den as f64)).round() as u64
            })
            .max(1);

        let segment_count =
            ((total_frames + max_frames_per_segment - 1) / max_frames_per_segment) as usize;

        let mut segments = Vec::new();
        let mut start_frame: u64 = 0;

        for segment_index in 0..segment_count {
            let remaining = total_frames - start_frame;
            let seg_frames = remaining.min(max_frames_per_segment);
            let end_frame = start_frame + seg_frames;

            let start_sec = (start_frame as f64 * r_den as f64) / (r_num as f64);
            let end_sec = (end_frame as f64 * r_den as f64) / (r_num as f64);
            let planned_duration_sec = end_sec - start_sec;

            let start_ms = (start_sec * 1000.0).round() as u64;
            let end_ms = (end_sec * 1000.0).round() as u64;

            segments.push(FlowPlannedSegment {
                segment_index,
                start_frame,
                end_frame,
                start_ms,
                end_ms,
                planned_duration_sec,
                planned_frame_count: seg_frames,
                source_segment_path: PathBuf::new(),
                source_segment_sha256: String::new(),
                child_job_id: None,
                state: FlowJobState::Planning,
            });

            start_frame = end_frame;
        }

        // Validate logical timeline coverage: no gaps, no overlaps, contiguous
        if segments.is_empty() {
            return Err("EMPTY_SEGMENT_PLAN: No segments generated".to_string());
        }
        if segments[0].start_frame != 0 {
            return Err("INVALID_COVERAGE: First segment does not start at frame 0".to_string());
        }
        for i in 0..segments.len() - 1 {
            if segments[i].end_frame != segments[i + 1].start_frame {
                return Err(format!(
                    "TIMELINE_GAP_OR_OVERLAP: Segment {} ends at {} but segment {} starts at {}",
                    i,
                    segments[i].end_frame,
                    i + 1,
                    segments[i + 1].start_frame
                ));
            }
        }
        if segments.last().unwrap().end_frame != total_frames {
            return Err(
                "INVALID_COVERAGE: Last segment does not cover total source frames".to_string(),
            );
        }

        Ok(FlowLongVideoPlan {
            parent_job_id: parent_job_id.to_string(),
            project_id: project_id.to_string(),
            source_media_id: source_media_id.map(|s| s.to_string()),
            source_duration_ms: (source_facts.duration_sec * 1000.0).round() as u64,
            source_fps_rational: (r_num, r_den),
            source_timing_mode,
            working_proxy_created,
            working_proxy_path,
            working_proxy_sha256,
            strategy: "CONTIGUOUS_FRAME_ALIGNED".to_string(),
            segment_count,
            segments,
            requested_config,
            prompt_hash: prompt_hash.to_string(),
            transformation_intent,
            identity_mode,
            continuity_strategy: FlowIdentityContinuityStrategy::SamePromptBaseline,
            identity_continuity_guaranteed: false,
            created_at: Utc::now().to_rfc3339(),
        })
    }

    /// Extracts frame-accurate source segments and validates that every segment duration <= 10.000s.
    pub fn extract_long_video_segments(
        plan: &mut FlowLongVideoPlan,
        source_path: &Path,
        output_dir: &Path,
    ) -> Result<(), String> {
        fs::create_dir_all(output_dir)
            .map_err(|e| format!("Failed to create source-segments dir: {}", e))?;

        let input_path = if plan.working_proxy_created && plan.working_proxy_path.is_some() {
            plan.working_proxy_path.as_ref().unwrap()
        } else {
            source_path
        };

        let (r_num, r_den) = plan.source_fps_rational;
        let fps = r_num as f64 / r_den as f64;

        for seg in &mut plan.segments {
            let seg_filename = format!("segment_{:03}.mp4", seg.segment_index);
            let seg_path = output_dir.join(&seg_filename);

            let start_sec = (seg.start_frame as f64 * r_den as f64) / (r_num as f64);
            let duration_sec = seg.planned_duration_sec;

            let output = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-ss",
                    &format!("{:.6}", start_sec),
                    "-i",
                    input_path.to_str().unwrap_or_default(),
                    "-t",
                    &format!("{:.6}", duration_sec),
                    "-c:v",
                    "libx264",
                    "-preset",
                    "fast",
                    "-crf",
                    "18",
                    "-pix_fmt",
                    "yuv420p",
                    "-r",
                    &format!("{:.4}", fps),
                    "-an",
                    "-avoid_negative_ts",
                    "make_zero",
                    "-fflags",
                    "+genpts",
                    seg_path.to_str().unwrap_or_default(),
                ])
                .output()
                .map_err(|e| format!("FFmpeg segment extraction failed: {}", e))?;

            if !output.status.success() {
                return Err(format!(
                    "FFmpeg segment extraction failed for segment {}: {}",
                    seg.segment_index,
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            // Post-extraction ffprobe verification (Section 9 & 15)
            let probed = SourceMediaProbe::probe_file(&seg_path).map_err(|e| {
                format!(
                    "Failed to probe extracted segment {}: {}",
                    seg.segment_index, e
                )
            })?;

            if probed.duration_sec > Self::MAX_SEGMENT_DURATION_SEC + 1e-4 {
                return Err(format!(
                    "EXTRACTED_SEGMENT_EXCEEDS_10S: Segment {} probed duration {:.4}s exceeds 10.000s",
                    seg.segment_index, probed.duration_sec
                ));
            }

            let bytes = fs::read(&seg_path).map_err(|e| {
                format!(
                    "Failed to read extracted segment {}: {}",
                    seg.segment_index, e
                )
            })?;
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let sha256 = format!("{:x}", hasher.finalize());

            seg.source_segment_path = seg_path;
            seg.source_segment_sha256 = sha256;
            seg.state = FlowJobState::ReadyToSubmit;
        }

        Ok(())
    }

    /// Legacy single/multi-shot planner retained for backward compatibility.
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

    /// Legacy segment split retained for backward compatibility.
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
