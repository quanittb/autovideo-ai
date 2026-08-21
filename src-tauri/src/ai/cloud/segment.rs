use super::error::CloudProviderError;
use super::job::ValidationPolicy;
use super::manifest::{SegmentBoundary, SegmentPlan};
use super::spec::{DetailedTimingFacts, SourceMediaFacts, SourceMediaProbe};
use super::validator::CloudOutputValidator;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_MAX_SEGMENT_DURATION_SEC: f64 = 60.0;
pub const SEGMENTATION_POLICY_VERSION: u32 = 1;
pub const SPLIT_ENCODING_POLICY_VERSION: u32 = 1;

pub struct SegmentPlanner;

impl SegmentPlanner {
    pub fn plan(
        source_facts: &SourceMediaFacts,
        timing_facts: &DetailedTimingFacts,
        provider_limit_sec: f64,
    ) -> Result<SegmentPlan, CloudProviderError> {
        if timing_facts.is_vfr {
            return Err(CloudProviderError::RequestInvalid(
                "UNSUPPORTED_VFR_SEGMENTATION: Source video has variable frame rate (VFR) which is not supported for deterministic segmentation".to_string(),
            ));
        }

        let fps = source_facts.fps;
        if fps <= 0.0 || !fps.is_finite() {
            return Err(CloudProviderError::RequestInvalid(
                "INVALID_FPS: Framerate must be a positive finite number".to_string(),
            ));
        }

        let total_frames = timing_facts
            .nb_frames
            .unwrap_or_else(|| (source_facts.duration_sec * fps).round() as u64);

        if total_frames == 0 {
            return Err(CloudProviderError::RequestInvalid(
                "EMPTY_SOURCE: Video has 0 frames".to_string(),
            ));
        }

        let total_source_duration_sec = source_facts.duration_sec;
        let r_num = timing_facts.r_frame_rate.num.max(1) as f64;
        let r_den = timing_facts.r_frame_rate.den.max(1) as f64;
        let total_limit_frames_float = (provider_limit_sec * r_num) / r_den;
        let mut max_segment_frames = total_limit_frames_float.floor() as u64;
        while max_segment_frames > 0
            && ((max_segment_frames as f64 * r_den) / r_num) >= provider_limit_sec
        {
            max_segment_frames -= 1;
        }
        let max_segment_frames = max_segment_frames.max(1);

        let mut boundaries = Vec::new();
        let mut current_start_frame: u64 = 0;
        let mut index = 0;

        let time_base_den = timing_facts.time_base.den.max(1);
        let time_base_num = timing_facts.time_base.num.max(1);

        while current_start_frame < total_frames {
            let remaining = total_frames - current_start_frame;
            let segment_frames = remaining.min(max_segment_frames);
            let end_frame = current_start_frame + segment_frames;

            let start_sec = (current_start_frame as f64 * r_den) / r_num;
            let end_sec = (end_frame as f64 * r_den) / r_num;
            let expected_duration_sec = end_sec - start_sec;

            let start_pts =
                ((start_sec * time_base_den as f64) / time_base_num as f64).round() as u64;
            let end_pts = ((end_sec * time_base_den as f64) / time_base_num as f64).round() as u64;

            let start_ms = (start_sec * 1000.0).round() as u64;
            let end_ms = (end_sec * 1000.0).round() as u64;

            boundaries.push(SegmentBoundary {
                index,
                start_frame: current_start_frame,
                end_frame,
                start_pts,
                end_pts,
                start_ms,
                end_ms,
                expected_duration_sec,
                start_sec,
                end_sec,
            });

            current_start_frame = end_frame;
            index += 1;
        }

        let plan_id = format!(
            "plan_{}_{}_{}segs",
            (total_source_duration_sec * 1000.0).round() as u64,
            (fps * 1000.0).round() as u64,
            boundaries.len()
        );

        Ok(SegmentPlan {
            plan_id,
            source_facts: source_facts.clone(),
            timing_facts: timing_facts.clone(),
            boundaries,
            policy_version: SEGMENTATION_POLICY_VERSION,
            provider_limit_ms: (provider_limit_sec * 1000.0).round() as u64,
            total_source_duration_sec,
        })
    }

    pub fn plan_segments(
        _source_path: &Path,
        duration_sec: f64,
        segment_duration_sec: f64,
        _prompt: &str,
        _negative_prompt: Option<&str>,
    ) -> Vec<SegmentBoundary> {
        let fps = 30.0;
        let mut boundaries = Vec::new();
        let mut current_start_sec = 0.0f64;
        let mut index = 0;

        while current_start_sec < duration_sec {
            let remaining = duration_sec - current_start_sec;
            let dur = remaining.min(segment_duration_sec);
            let end_sec = current_start_sec + dur;

            let start_frame = (current_start_sec * fps).round() as u64;
            let end_frame = (end_sec * fps).round() as u64;
            let start_ms = (current_start_sec * 1000.0).round() as u64;
            let end_ms = (end_sec * 1000.0).round() as u64;

            boundaries.push(SegmentBoundary {
                index,
                start_frame,
                end_frame,
                start_pts: start_ms,
                end_pts: end_ms,
                start_ms,
                end_ms,
                expected_duration_sec: dur,
                start_sec: current_start_sec,
                end_sec,
            });

            current_start_sec = end_sec;
            index += 1;
        }

        boundaries
    }
}

pub struct SegmentSplitter;

impl SegmentSplitter {
    pub fn get_ffmpeg_build_fingerprint() -> Result<String, CloudProviderError> {
        let output = Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map_err(|e| {
                CloudProviderError::ProviderUnavailable(format!(
                    "FFMPEG_NOT_FOUND: Failed to invoke ffmpeg to get fingerprint: {}",
                    e
                ))
            })?;
        if !output.status.success() {
            return Err(CloudProviderError::ProviderUnavailable(
                "FFMPEG_VERSION_FAILED: ffmpeg -version returned non-zero exit code".to_string(),
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let first_line = text
            .lines()
            .next()
            .ok_or_else(|| {
                CloudProviderError::ProviderUnavailable(
                    "FFMPEG_OUTPUT_EMPTY: ffmpeg output empty".to_string(),
                )
            })?
            .trim();
        if first_line.is_empty() {
            return Err(CloudProviderError::ProviderUnavailable(
                "FFMPEG_VERSION_EMPTY: Unable to extract ffmpeg build identity".to_string(),
            ));
        }
        let mut hasher = sha2::Sha256::default();
        use sha2::Digest;
        hasher.update(first_line.as_bytes());
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn split_segment(
        source_path: &Path,
        boundary: &SegmentBoundary,
        fps: f64,
        out_path: &Path,
        max_provider_limit_sec: f64,
    ) -> Result<SourceMediaFacts, CloudProviderError> {
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                CloudProviderError::JobFailed(format!(
                    "FAILED_CREATE_SPLIT_DIR: Failed to create dir {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        let start_sec = boundary.start_ms as f64 / 1000.0;
        let mut dur_sec = boundary.expected_duration_sec;
        let frame_time = 1.0 / fps;

        for attempt in 0..3 {
            let output = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-ss",
                    &format!("{:.6}", start_sec),
                    "-t",
                    &format!("{:.6}", dur_sec),
                    "-i",
                    source_path.to_str().ok_or_else(|| {
                        CloudProviderError::RequestInvalid("Invalid path unicode".to_string())
                    })?,
                    "-c:v",
                    "libx264",
                    "-preset",
                    "fast",
                    "-crf",
                    "18",
                    "-pix_fmt",
                    "yuv420p",
                    "-an", // Explicitly strip audio in child segment
                    "-avoid_negative_ts",
                    "make_zero",
                    "-fflags",
                    "+genpts",
                    out_path.to_str().ok_or_else(|| {
                        CloudProviderError::RequestInvalid("Invalid path unicode".to_string())
                    })?,
                ])
                .output()
                .map_err(|e| {
                    CloudProviderError::JobFailed(format!(
                        "SPLIT_FAILED: Failed to invoke ffmpeg: {}",
                        e
                    ))
                })?;

            if !output.status.success() {
                return Err(CloudProviderError::JobFailed(format!(
                    "SPLIT_FAILED: ffmpeg exited with non-zero status: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }

            let facts = SourceMediaProbe::probe_file(out_path)?;
            if facts.duration_sec <= max_provider_limit_sec {
                return Ok(facts);
            }

            // Exceeds provider duration limit - reduce by frame time and retry
            dur_sec -= frame_time * (attempt as f64 + 1.0);
        }

        Err(CloudProviderError::JobFailed(
            "SEGMENT_DURATION_LIMIT_VIOLATION: Split segment duration consistently exceeded provider limit after 3 correction attempts".to_string(),
        ))
    }
}

pub struct SegmentStitcher;

impl SegmentStitcher {
    pub fn check_stream_copy_compatibility(
        artifacts: &[PathBuf],
    ) -> Result<bool, CloudProviderError> {
        let mut ref_facts: Option<SourceMediaFacts> = None;

        for path in artifacts {
            if !path.exists() {
                return Ok(false);
            }

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext != "webm" {
                return Ok(false);
            }

            let facts = SourceMediaProbe::probe_file(path)?;
            if let Some(ref rf) = ref_facts {
                if facts.width != rf.width || facts.height != rf.height {
                    return Ok(false);
                }
                if (facts.fps - rf.fps).abs() > 0.05 {
                    return Ok(false);
                }
            } else {
                ref_facts = Some(facts);
            }
        }

        Ok(true)
    }

    pub fn stitch_segments(
        artifacts: &[PathBuf],
        out_path: &Path,
    ) -> Result<(), CloudProviderError> {
        if artifacts.is_empty() {
            return Err(CloudProviderError::RequestInvalid(
                "NO_ARTIFACTS_TO_STITCH: Segment artifact list is empty".to_string(),
            ));
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                CloudProviderError::JobFailed(format!(
                    "FAILED_CREATE_DIR: Failed to create dir {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        let validator = CloudOutputValidator::new();
        let validation_policy = ValidationPolicy {
            expected_duration_sec: None,
            expected_width: None,
            expected_height: None,
            expected_fps: None,
            require_audio: false,
            require_alpha: true,
            expected_container: Some("webm".to_string()),
            expected_video_codec: Some("vp9".to_string()),
        };

        if artifacts.len() == 1 {
            fs::copy(&artifacts[0], out_path).map_err(|e| {
                CloudProviderError::JobFailed(format!("FAILED_COPY_SINGLE_SEGMENT: {}", e))
            })?;
            validator.validate_artifact_with_policy(out_path, &validation_policy)?;
            return Ok(());
        }

        let is_stream_copy_compatible =
            Self::check_stream_copy_compatibility(artifacts).unwrap_or(false);

        if is_stream_copy_compatible {
            // Attempt Concat Demuxer stream-copy
            let temp_list_path = out_path.with_extension("concat_list.txt");
            let mut list_file = fs::File::create(&temp_list_path).map_err(|e| {
                CloudProviderError::JobFailed(format!("FAILED_CREATE_CONCAT_LIST: {}", e))
            })?;

            for path in artifacts {
                let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
                let escaped_path = canonical.to_str().unwrap_or("").replace('\\', "/");
                writeln!(list_file, "file '{}'", escaped_path).map_err(|e| {
                    CloudProviderError::JobFailed(format!("FAILED_WRITE_CONCAT_LIST: {}", e))
                })?;
            }
            drop(list_file);

            let status = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-f",
                    "concat",
                    "-safe",
                    "0",
                    "-i",
                    temp_list_path.to_str().unwrap_or(""),
                    "-c",
                    "copy",
                    out_path.to_str().ok_or_else(|| {
                        CloudProviderError::RequestInvalid("Invalid path unicode".to_string())
                    })?,
                ])
                .output();

            let _ = fs::remove_file(&temp_list_path);

            if let Ok(out) = status {
                if out.status.success() && out_path.exists() {
                    // Production validation of stream-copied stitched output
                    if validator
                        .validate_artifact_with_policy(out_path, &validation_policy)
                        .is_ok()
                    {
                        return Ok(());
                    }
                }
            }
        }

        // Fallback: VP9 alpha re-encode concat filter
        Self::stitch_with_vp9_reencode(artifacts, out_path)?;
        validator.validate_artifact_with_policy(out_path, &validation_policy)?;
        Ok(())
    }

    pub fn stitch_with_vp9_reencode(
        artifacts: &[PathBuf],
        out_path: &Path,
    ) -> Result<(), CloudProviderError> {
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y");

        for path in artifacts {
            cmd.arg("-i").arg(path);
        }

        let count = artifacts.len();
        let mut filter = String::new();
        for i in 0..count {
            filter.push_str(&format!("[{}:v:0]", i));
        }
        filter.push_str(&format!("concat=n={}:v=1:a=0[outv]", count));

        cmd.args([
            "-filter_complex",
            &filter,
            "-map",
            "[outv]",
            "-c:v",
            "libvpx-vp9",
            "-pix_fmt",
            "yuva420p",
            "-auto-alt-ref",
            "0",
            out_path.to_str().ok_or_else(|| {
                CloudProviderError::RequestInvalid("Invalid path unicode".to_string())
            })?,
        ]);

        let output = cmd.output().map_err(|e| {
            CloudProviderError::JobFailed(format!(
                "VP9_CONCAT_REENCODE_FAILED: Failed to invoke ffmpeg: {}",
                e
            ))
        })?;

        if !output.status.success() {
            return Err(CloudProviderError::JobFailed(format!(
                "ALPHA_STITCH_REENCODE_UNSUPPORTED: Concat re-encode failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }
}

pub struct FinalAudioMuxer;

impl FinalAudioMuxer {
    pub fn mux_original_audio(
        stitched_video: &Path,
        original_source: &Path,
        out_path: &Path,
    ) -> Result<(), CloudProviderError> {
        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                stitched_video.to_str().ok_or_else(|| {
                    CloudProviderError::RequestInvalid("Invalid path unicode".to_string())
                })?,
                "-i",
                original_source.to_str().ok_or_else(|| {
                    CloudProviderError::RequestInvalid("Invalid path unicode".to_string())
                })?,
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "copy",
                "-c:a",
                "libopus",
                "-b:a",
                "128k",
                out_path.to_str().ok_or_else(|| {
                    CloudProviderError::RequestInvalid("Invalid path unicode".to_string())
                })?,
            ])
            .output()
            .map_err(|e| {
                CloudProviderError::JobFailed(format!(
                    "FINAL_AUDIO_MUX_FAILED: Failed to invoke ffmpeg: {}",
                    e
                ))
            })?;

        if !output.status.success() {
            return Err(CloudProviderError::JobFailed(format!(
                "FINAL_AUDIO_MUX_FAILED: ffmpeg exited with non-zero code: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }
}
