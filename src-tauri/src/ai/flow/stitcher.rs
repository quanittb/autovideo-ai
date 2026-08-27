use super::manifest::{
    FlowAudioRestorationMode, FlowCanonicalGeometry, FlowFinalAudioPolicy, FlowNormalizedSegment,
    FlowOutputArtifactRecord, FlowPlannedSegment, FlowRationalFrameRate,
};
use super::output_validator::FlowOutputValidator;
use crate::ai::cloud::spec::{SourceMediaFacts, SourceMediaProbe};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct FlowVideoNormalizer;

fn get_facts_frame_count(facts: &SourceMediaFacts) -> u64 {
    facts
        .timing
        .as_ref()
        .and_then(|t| t.nb_frames)
        .unwrap_or_else(|| (facts.duration_sec * facts.fps).round() as u64)
}

impl FlowVideoNormalizer {
    /// Maximum allowable frame drift between raw provider child and planned timeline (Section G & 10).
    pub const RAW_CHILD_DURATION_TOLERANCE_FRAMES: i64 = 2;

    /// Normalizes a child video output to canonical video stream parameters and exact target frame count
    /// using a robust TWO-PASS NORMALIZATION process (Section 10).
    ///
    /// PASS 1 — Canonicalize Stream:
    /// - Converts raw child to requested exact rational FPS (`fps=fps={num}/{den}`), canonical geometry,
    ///   SAR 1:1, H.264 / yuv420p, and strips audio.
    /// - Writes to temporary `pass1` file in project-local working path.
    /// - Probes PASS 1 stream to discover `canonicalizedFrameCount`.
    ///
    /// PASS 2 — Exact Timeline:
    /// - If drift == 0: finalize immediately.
    /// - If child is short by <= 2 frames: deterministic clone-frame pad final frame (`tpad`).
    /// - If child is long by <= 2 frames: deterministic trim to exact planned frame count (`trim`).
    /// - If abs(drift) > 2 frames: fails parent job with `FLOW_CHILD_DURATION_DRIFT_EXCEEDED`.
    /// - Probes final output and requires `FINAL_NORMALIZED_FRAME_COUNT == PLANNED_FRAME_COUNT`.
    /// - Cleans up `pass1` temporary file and never mutates raw provider child.
    pub fn normalize_child_segment(
        raw_child_path: &Path,
        planned_segment: &FlowPlannedSegment,
        canonical_geometry: &FlowCanonicalGeometry,
        fps: impl Into<FlowRationalFrameRate>,
        normalized_output_path: &Path,
    ) -> Result<SourceMediaFacts, String> {
        let fps: FlowRationalFrameRate = fps.into();
        if !raw_child_path.exists() {
            return Err(format!(
                "NORMALIZER_INPUT_MISSING: Raw child segment does not exist at {:?}",
                raw_child_path
            ));
        }

        if let Some(parent) = normalized_output_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let raw_probe = SourceMediaProbe::probe_file(raw_child_path)
            .map_err(|e| format!("Failed to probe raw child segment: {}", e))?;

        // 1. Orientation check (Section I)
        let is_target_portrait = canonical_geometry
            .orientation
            .eq_ignore_ascii_case("PORTRAIT")
            || canonical_geometry.orientation.eq_ignore_ascii_case("9:16");
        let is_child_portrait = raw_probe.height >= raw_probe.width;

        if is_target_portrait && !is_child_portrait {
            return Err(format!(
                "FLOW_CHILD_ORIENTATION_MISMATCH: Child video is landscape ({}x{}) but portrait orientation was requested",
                raw_probe.width, raw_probe.height
            ));
        }

        let is_target_landscape = canonical_geometry
            .orientation
            .eq_ignore_ascii_case("LANDSCAPE")
            || canonical_geometry.orientation.eq_ignore_ascii_case("16:9");
        if is_target_landscape && is_child_portrait {
            return Err(format!(
                "FLOW_CHILD_ORIENTATION_MISMATCH: Child video is portrait ({}x{}) but landscape orientation was requested",
                raw_probe.width, raw_probe.height
            ));
        }

        let planned_frames = planned_segment.planned_frame_count;

        // -------------------------------------------------------------
        // PASS 1 — Canonicalize Stream (Section 10)
        // -------------------------------------------------------------
        let pass1_path = normalized_output_path.with_extension("pass1.mp4");

        let target_w = canonical_geometry.width - (canonical_geometry.width % 2);
        let target_h = canonical_geometry.height - (canonical_geometry.height % 2);

        let pass1_vf = format!(
            "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,fps=fps={},format=yuv420p",
            target_w, target_h, target_w, target_h, fps.to_ffmpeg_arg()
        );

        let pass1_out = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                raw_child_path.to_str().unwrap_or_default(),
                "-vf",
                &pass1_vf,
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-an",
                pass1_path.to_str().unwrap_or_default(),
            ])
            .output()
            .map_err(|e| format!("FFmpeg normalization pass 1 failed: {}", e))?;

        if !pass1_out.status.success() {
            let _ = fs::remove_file(&pass1_path);
            return Err(format!(
                "NORMALIZATION_PASS1_FAILED: ffmpeg exited with error: {}",
                String::from_utf8_lossy(&pass1_out.stderr)
            ));
        }

        let pass1_probe = SourceMediaProbe::probe_file(&pass1_path)
            .map_err(|e| format!("Failed to probe pass 1 output: {}", e))?;

        let canonicalized_frames = get_facts_frame_count(&pass1_probe);
        let drift_frames = (canonicalized_frames as i64) - (planned_frames as i64);

        if drift_frames.abs() > Self::RAW_CHILD_DURATION_TOLERANCE_FRAMES {
            let _ = fs::remove_file(&pass1_path);
            return Err(format!(
                "FLOW_CHILD_DURATION_DRIFT_EXCEEDED: Canonicalized child segment {} has {} frames, planned {} frames (drift {} > tolerance {})",
                planned_segment.segment_index,
                canonicalized_frames,
                planned_frames,
                drift_frames,
                Self::RAW_CHILD_DURATION_TOLERANCE_FRAMES
            ));
        }

        // -------------------------------------------------------------
        // PASS 2 — Exact Timeline (Section 10)
        // -------------------------------------------------------------
        if drift_frames == 0 {
            // Drift is zero; move or copy pass1 to final output
            #[cfg(target_os = "windows")]
            {
                if normalized_output_path.exists() {
                    let _ = fs::remove_file(normalized_output_path);
                }
            }
            fs::rename(&pass1_path, normalized_output_path)
                .or_else(|_| {
                    fs::copy(&pass1_path, normalized_output_path)
                        .map(|_| ())
                        .and_then(|_| fs::remove_file(&pass1_path))
                })
                .map_err(|e| format!("Failed to finalize normalized output: {}", e))?;
        } else if drift_frames < 0 {
            // Short by <= 2 frames: clone-pad final frame
            let pad_count = planned_frames - canonicalized_frames;
            let pass2_out = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-i",
                    pass1_path.to_str().unwrap_or_default(),
                    "-vf",
                    &format!("tpad=stop_mode=clone:stop={}", pad_count),
                    "-c:v",
                    "libx264",
                    "-preset",
                    "fast",
                    "-crf",
                    "18",
                    "-pix_fmt",
                    "yuv420p",
                    "-an",
                    normalized_output_path.to_str().unwrap_or_default(),
                ])
                .output()
                .map_err(|e| format!("FFmpeg normalization pass 2 pad failed: {}", e))?;

            let _ = fs::remove_file(&pass1_path);

            if !pass2_out.status.success() {
                return Err(format!(
                    "NORMALIZATION_PASS2_FAILED: pad failed: {}",
                    String::from_utf8_lossy(&pass2_out.stderr)
                ));
            }
        } else {
            // Long by <= 2 frames: trim to exact planned frame count
            let pass2_out = Command::new("ffmpeg")
                .args([
                    "-y",
                    "-i",
                    pass1_path.to_str().unwrap_or_default(),
                    "-vf",
                    &format!(
                        "trim=start_frame=0:end_frame={},setpts=PTS-STARTPTS",
                        planned_frames
                    ),
                    "-c:v",
                    "libx264",
                    "-preset",
                    "fast",
                    "-crf",
                    "18",
                    "-pix_fmt",
                    "yuv420p",
                    "-an",
                    normalized_output_path.to_str().unwrap_or_default(),
                ])
                .output()
                .map_err(|e| format!("FFmpeg normalization pass 2 trim failed: {}", e))?;

            let _ = fs::remove_file(&pass1_path);

            if !pass2_out.status.success() {
                return Err(format!(
                    "NORMALIZATION_PASS2_FAILED: trim failed: {}",
                    String::from_utf8_lossy(&pass2_out.stderr)
                ));
            }
        }

        // Post-normalization probe: strict frame equality required (Section 10)
        let norm_probe = SourceMediaProbe::probe_file(normalized_output_path)
            .map_err(|e| format!("Failed to probe final normalized child segment: {}", e))?;

        let final_frames = get_facts_frame_count(&norm_probe);
        if final_frames != planned_frames {
            return Err(format!(
                "NORMALIZATION_FRAME_COUNT_MISMATCH: Final normalized child has {} frames, expected exact planned {} frames",
                final_frames, planned_frames
            ));
        }

        Ok(norm_probe)
    }
}

pub struct FlowStitcher;

impl FlowStitcher {
    /// Stitches normalized child video streams in explicit sorted segmentIndex order and muxes full original audio ONCE.
    ///
    /// Ordering Contract (Section 12):
    /// - Accepts `&[FlowNormalizedSegment]` with explicit `segment_index`.
    /// - Sorts segments by `segment_index`.
    /// - Validates: first index is 0, no duplicate index, no gaps, last index is N - 1.
    /// - Does NOT rely on filesystem or caller array ordering.
    pub fn stitch_long_video_timeline(
        segments: &[FlowNormalizedSegment],
        source_audio_path: Option<&Path>,
        expected_total_frames: u64,
        fps: FlowRationalFrameRate,
        output_file_path: &Path,
    ) -> Result<(FlowOutputArtifactRecord, FlowAudioRestorationMode), String> {
        if segments.is_empty() {
            return Err(
                "STITCH_FAILED: No normalized segment records provided for stitching".to_string(),
            );
        }

        // 1. Sort and validate explicit segment ordering (Section 12)
        let mut sorted_segments = segments.to_vec();
        sorted_segments.sort_by_key(|s| s.segment_index);

        if sorted_segments[0].segment_index != 0 {
            return Err("STITCH_ORDER_INVALID: First segment index is not 0".to_string());
        }
        for i in 0..sorted_segments.len() - 1 {
            if sorted_segments[i].segment_index == sorted_segments[i + 1].segment_index {
                return Err(format!(
                    "STITCH_ORDER_DUPLICATE: Duplicate segment index {}",
                    sorted_segments[i].segment_index
                ));
            }
            if sorted_segments[i + 1].segment_index != sorted_segments[i].segment_index + 1 {
                return Err(format!(
                    "STITCH_ORDER_GAP: Gap detected between segment {} and {}",
                    sorted_segments[i].segment_index,
                    sorted_segments[i + 1].segment_index
                ));
            }
        }
        if sorted_segments.last().unwrap().segment_index != sorted_segments.len() - 1 {
            return Err(
                "STITCH_ORDER_INVALID: Last segment index does not match segment count - 1"
                    .to_string(),
            );
        }

        if let Some(parent) = output_file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Prepare concat list file in strict sorted order
        let list_file = output_file_path.with_extension("concat_list.txt");
        let mut list_content = String::new();
        for seg in &sorted_segments {
            let path_str = seg.path.to_string_lossy().replace('\\', "/");
            list_content.push_str(&format!("file '{}'\n", path_str));
        }
        fs::write(&list_file, &list_content)
            .map_err(|e| format!("Failed to write concat list: {}", e))?;

        let temp_video = output_file_path.with_extension("stitched_video_only.mp4");

        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                list_file.to_str().unwrap_or_default(),
                "-c:v",
                "copy",
                "-an",
                temp_video.to_str().unwrap_or_default(),
            ])
            .output()
            .map_err(|e| format!("FFmpeg concat execution failed: {}", e))?;

        let _ = fs::remove_file(&list_file);

        if !output.status.success() {
            return Err(format!(
                "FFmpeg concat demuxer failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Timeline drift check (Section H & 21)
        let inter_probe = SourceMediaProbe::probe_file(&temp_video)
            .map_err(|e| format!("Failed to probe intermediate stitched video: {}", e))?;

        let inter_frames = get_facts_frame_count(&inter_probe);
        let drift = (inter_frames as i64 - expected_total_frames as i64).abs();
        if drift > 1 {
            let _ = fs::remove_file(&temp_video);
            return Err(format!(
                "FINAL_VIDEO_TIMELINE_DRIFT_EXCEEDED: Stitched video has {} frames, expected {} frames (drift {} > 1 frame)",
                inter_frames, expected_total_frames, drift
            ));
        }

        // Audio Restoration (Section J & K): Mux original source audio ONCE
        let audio_mode = if let Some(audio_src) = source_audio_path {
            let audio_probe = SourceMediaProbe::probe_file(audio_src).ok();
            let has_audio = audio_probe.map(|p| p.has_audio).unwrap_or(false);

            if audio_src.exists() && has_audio {
                // Try Stream Copy first
                let copy_out = Command::new("ffmpeg")
                    .args([
                        "-y",
                        "-i",
                        temp_video.to_str().unwrap_or_default(),
                        "-i",
                        audio_src.to_str().unwrap_or_default(),
                        "-map",
                        "0:v:0",
                        "-map",
                        "1:a:0",
                        "-c:v",
                        "copy",
                        "-c:a",
                        "copy",
                        output_file_path.to_str().unwrap_or_default(),
                    ])
                    .output();

                let can_copy = match copy_out {
                    Ok(ref res) => res.status.success(),
                    Err(_) => false,
                };

                if can_copy {
                    let _ = fs::remove_file(&temp_video);
                    FlowAudioRestorationMode::StreamCopy
                } else {
                    // Fallback to deterministic transcode
                    let trans_out = Command::new("ffmpeg")
                        .args([
                            "-y",
                            "-i",
                            temp_video.to_str().unwrap_or_default(),
                            "-i",
                            audio_src.to_str().unwrap_or_default(),
                            "-map",
                            "0:v:0",
                            "-map",
                            "1:a:0",
                            "-c:v",
                            "copy",
                            "-c:a",
                            "aac",
                            "-b:a",
                            "192k",
                            output_file_path.to_str().unwrap_or_default(),
                        ])
                        .output()
                        .map_err(|e| format!("FFmpeg audio transcode muxing failed: {}", e))?;

                    let _ = fs::remove_file(&temp_video);

                    if !trans_out.status.success() {
                        return Err(format!(
                            "AUDIO_MUX_FAILED: Transcode muxing failed: {}",
                            String::from_utf8_lossy(&trans_out.stderr)
                        ));
                    }
                    FlowAudioRestorationMode::DeterministicTranscode
                }
            } else {
                let _ = fs::rename(&temp_video, output_file_path);
                FlowAudioRestorationMode::NoSourceAudio
            }
        } else {
            let _ = fs::rename(&temp_video, output_file_path);
            FlowAudioRestorationMode::NoSourceAudio
        };

        // Final output validation: expected duration using exact rational FPS (Section 9)
        let expected_duration_sec = fps.expected_duration_sec(expected_total_frames);
        let final_record =
            FlowOutputValidator::validate_child_artifact(output_file_path, expected_duration_sec)?;

        Ok((final_record, audio_mode))
    }

    /// Legacy stitcher retained for backward compatibility.
    pub fn stitch_flow_segments(
        segment_paths: &[PathBuf],
        source_audio_path: Option<&Path>,
        expected_duration_sec: f64,
        _audio_policy: &FlowFinalAudioPolicy,
        output_file_path: &Path,
    ) -> Result<FlowOutputArtifactRecord, String> {
        let normalized_segments: Vec<FlowNormalizedSegment> = segment_paths
            .iter()
            .enumerate()
            .map(|(idx, p)| FlowNormalizedSegment {
                segment_index: idx,
                path: p.clone(),
                frame_count: 0,
                sha256: String::new(),
            })
            .collect();

        let expected_total_frames = (expected_duration_sec * 30.0).round() as u64;
        let rational_fps = FlowRationalFrameRate::new(30, 1);
        let (record, _) = Self::stitch_long_video_timeline(
            &normalized_segments,
            source_audio_path,
            expected_total_frames,
            rational_fps,
            output_file_path,
        )?;
        Ok(record)
    }
}
