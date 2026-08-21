use super::error::CloudProviderError;
use super::manifest::{SegmentBoundary, SegmentPlan};
use super::spec::{DetailedTimingFacts, SourceMediaFacts, SourceMediaProbe};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SEGMENTATION_POLICY_VERSION: u32 = 1;
pub const SPLIT_ENCODING_POLICY_VERSION: u32 = 1;
pub const DEFAULT_MAX_SEGMENT_DURATION_SEC: f64 = 60.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoSegment {
    pub segment_id: String,
    pub source_path: PathBuf,
    pub start_sec: f64,
    pub end_sec: f64,
    pub duration_sec: f64,
    pub prompt: Option<String>,
    pub reference_image: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct SplitEncodingPolicy {
    pub container: String,
    pub video_codec: String,
    pub pixel_format: String,
    pub preset: String,
    pub crf: u32,
    pub preserve_audio: bool,
    pub policy_version: u32,
}

impl Default for SplitEncodingPolicy {
    fn default() -> Self {
        Self {
            container: "mp4".to_string(),
            video_codec: "libx264".to_string(),
            pixel_format: "yuv420p".to_string(),
            preset: "fast".to_string(),
            crf: 18,
            preserve_audio: false, // Video-only child segments
            policy_version: SPLIT_ENCODING_POLICY_VERSION,
        }
    }
}

pub struct SegmentPlanner;

impl SegmentPlanner {
    pub fn plan_segments(
        source_path: &Path,
        duration_sec: f64,
        max_segment_sec: f64,
        prompt: &str,
        reference_image: Option<PathBuf>,
    ) -> Vec<VideoSegment> {
        let mut segments = Vec::new();
        let mut current_start = 0.0;
        let mut index = 0;

        while current_start < duration_sec {
            let current_end = (current_start + max_segment_sec).min(duration_sec);
            let segment_dur = current_end - current_start;

            segments.push(VideoSegment {
                segment_id: format!("seg_{}", index),
                source_path: source_path.to_path_buf(),
                start_sec: current_start,
                end_sec: current_end,
                duration_sec: segment_dur,
                prompt: Some(prompt.to_string()),
                reference_image: reference_image.clone(),
            });

            current_start = current_end;
            index += 1;
        }

        segments
    }

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

        let fps = timing_facts.r_frame_rate.to_f64();
        if fps <= 0.0 || !fps.is_finite() {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_TIMING_FPS: Probed invalid timing fps: {:.2}",
                fps
            )));
        }

        let total_source_duration_sec = source_facts.duration_sec;
        if total_source_duration_sec <= 0.0 || !total_source_duration_sec.is_finite() {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_DURATION: Probed non-positive duration: {:.2}s",
                total_source_duration_sec
            )));
        }

        let total_frames = if let Some(nbf) = timing_facts.nb_frames {
            nbf
        } else {
            (total_source_duration_sec * fps).round() as u64
        };

        if total_frames == 0 {
            return Err(CloudProviderError::RequestInvalid(
                "ZERO_FRAMES: Source video contains 0 frames".to_string(),
            ));
        }

        // Compute largest legal frame count strictly below provider limit (strictly < 60s)
        let max_legal_frames = ((provider_limit_sec * fps).floor() as u64).saturating_sub(1);
        if max_legal_frames == 0 {
            return Err(CloudProviderError::RequestInvalid(format!(
                "PROVIDER_LIMIT_TOO_LOW: Provider limit {:.2}s cannot accommodate 1 frame at {:.2} fps",
                provider_limit_sec, fps
            )));
        }

        let segment_count = ((total_frames as f64) / (max_legal_frames as f64)).ceil() as usize;
        let segment_count = segment_count.max(1);

        let frames_per_segment = ((total_frames as f64) / (segment_count as f64)).ceil() as u64;

        let mut boundaries = Vec::with_capacity(segment_count);
        for i in 0..segment_count {
            let start_frame = i as u64 * frames_per_segment;
            let end_frame = ((i as u64 + 1) * frames_per_segment).min(total_frames);
            let seg_frames = end_frame.saturating_sub(start_frame);
            let expected_duration_sec = seg_frames as f64 / fps;
            let start_ms = ((start_frame as f64 / fps) * 1000.0).round() as u64;
            let end_ms = ((end_frame as f64 / fps) * 1000.0).round() as u64;

            boundaries.push(SegmentBoundary {
                index: i,
                start_frame,
                end_frame,
                start_pts: start_frame,
                end_pts: end_frame,
                start_ms,
                end_ms,
                expected_duration_sec,
            });
        }

        let plan_id = format!(
            "plan-{}-{:.0}s-{}seg",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>(),
            total_source_duration_sec,
            segment_count
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
}

pub struct SegmentSplitter;

impl SegmentSplitter {
    pub fn get_ffmpeg_build_fingerprint() -> String {
        match Command::new("ffmpeg").arg("-version").output() {
            Ok(output) if output.status.success() => {
                let text = String::from_utf8_lossy(&output.stdout);
                let first_line = text.lines().next().unwrap_or("ffmpeg_unknown").trim();
                let mut hasher = sha2::Sha256::default();
                use sha2::Digest;
                hasher.update(first_line.as_bytes());
                format!("{:x}", hasher.finalize())
            }
            _ => "ffmpeg_default_fingerprint".to_string(),
        }
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
                        "SPLIT_ENCODE_INVOKE_FAILED: Failed to invoke ffmpeg: {}",
                        e
                    ))
                })?;

            if !output.status.success() {
                return Err(CloudProviderError::JobFailed(format!(
                    "SPLIT_ENCODE_FAILED: ffmpeg exited with non-zero code: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }

            // Authoritative re-probe of created segment on disk
            let facts = SourceMediaProbe::probe_file(out_path)?;

            if facts.duration_sec <= max_provider_limit_sec {
                return Ok(facts);
            }

            // Local duration correction loop: shrink by 1 frame duration
            dur_sec = (dur_sec - frame_time).max(frame_time);
            if attempt == 2 {
                return Err(CloudProviderError::JobFailed(format!(
                    "SEGMENT_DURATION_LIMIT_VIOLATION: Probed segment duration {:.3}s exceeds provider limit {:.2}s after 3 correction iterations",
                    facts.duration_sec, max_provider_limit_sec
                )));
            }
        }

        SourceMediaProbe::probe_file(out_path)
    }
}

pub struct SegmentStitcher;

impl SegmentStitcher {
    pub fn check_stream_copy_compatibility(
        artifacts: &[PathBuf],
    ) -> Result<bool, CloudProviderError> {
        if artifacts.is_empty() {
            return Ok(false);
        }

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

        if artifacts.len() == 1 {
            fs::copy(&artifacts[0], out_path).map_err(|e| {
                CloudProviderError::JobFailed(format!("FAILED_COPY_SINGLE_SEGMENT: {}", e))
            })?;
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
                    // Check output validity
                    if let Ok(probed) = SourceMediaProbe::probe_file(out_path) {
                        if probed.width > 0 && probed.height > 0 && probed.duration_sec > 0.0 {
                            return Ok(());
                        }
                    }
                }
            }
        }

        // Fallback: VP9 alpha re-encode concat filter
        Self::stitch_with_vp9_reencode(artifacts, out_path)
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
