use super::manifest::{
    FlowAudioRestorationMode, FlowCanonicalGeometry, FlowFinalAudioPolicy,
    FlowOutputArtifactRecord, FlowPlannedSegment,
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
    /// Maximum allowable frame drift between raw provider child and planned timeline (Section G).
    pub const RAW_CHILD_DURATION_TOLERANCE_FRAMES: i64 = 2;

    /// Normalizes a child video output to canonical video stream parameters and exact target frame count.
    ///
    /// Normalizes:
    /// - Codec (H.264), pixel format (yuv420p), CFR fps, timebase, SAR (1:1), dimensions (aspect-ratio preserved).
    /// - Target timeline: If child is longer by <= 2 frames, trims extra frames.
    ///   If child is shorter by <= 2 frames, pads using deterministic clone-frame padding (tpad).
    ///   If drift > 2 frames, returns FLOW_CHILD_DURATION_DRIFT_EXCEEDED.
    pub fn normalize_child_segment(
        raw_child_path: &Path,
        planned_segment: &FlowPlannedSegment,
        canonical_geometry: &FlowCanonicalGeometry,
        target_fps: f64,
        normalized_output_path: &Path,
    ) -> Result<SourceMediaFacts, String> {
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

        // 2. Timeline frame drift check (Section G)
        let raw_frames = get_facts_frame_count(&raw_probe);
        let planned_frames = planned_segment.planned_frame_count;
        let drift_frames = (raw_frames as i64) - (planned_frames as i64);

        if drift_frames.abs() > Self::RAW_CHILD_DURATION_TOLERANCE_FRAMES {
            return Err(format!(
                "FLOW_CHILD_DURATION_DRIFT_EXCEEDED: Raw child segment {} has {} frames, planned {} frames (drift {} > tolerance {})",
                planned_segment.segment_index,
                raw_frames,
                planned_frames,
                drift_frames,
                Self::RAW_CHILD_DURATION_TOLERANCE_FRAMES
            ));
        }

        // 3. Build deterministic normalization filter chain
        let mut vf_filters = Vec::new();

        if drift_frames > 0 {
            // Raw child is longer: trim deterministic extra frames
            vf_filters.push(format!("trim=start_frame=0:end_frame={}", planned_frames));
            vf_filters.push("setpts=PTS-STARTPTS".to_string());
        } else if drift_frames < 0 {
            // Raw child is shorter: clone pad last frame
            let pad_count = planned_frames - raw_frames;
            vf_filters.push(format!("tpad=stop_mode=clone:stop={}", pad_count));
        }

        // Aspect-ratio preserving scale and pad to canonical canvas (Section I)
        let target_w = canonical_geometry.width - (canonical_geometry.width % 2);
        let target_h = canonical_geometry.height - (canonical_geometry.height % 2);
        vf_filters.push(format!(
            "scale={}:{}:force_original_aspect_ratio=decrease",
            target_w, target_h
        ));
        vf_filters.push(format!(
            "pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black",
            target_w, target_h
        ));
        vf_filters.push("setsar=1".to_string());
        vf_filters.push(format!("fps=fps={:.4}", target_fps));
        vf_filters.push("format=yuv420p".to_string());

        let vf_arg = vf_filters.join(",");

        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                raw_child_path.to_str().unwrap_or_default(),
                "-vf",
                &vf_arg,
                "-c:v",
                "libx264",
                "-preset",
                "fast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-an", // Strip audio during video normalization
                normalized_output_path.to_str().unwrap_or_default(),
            ])
            .output()
            .map_err(|e| format!("FFmpeg normalization execution failed: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "NORMALIZATION_FAILED: ffmpeg exited with error: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let norm_probe = SourceMediaProbe::probe_file(normalized_output_path)
            .map_err(|e| format!("Failed to probe normalized child segment: {}", e))?;

        Ok(norm_probe)
    }
}

pub struct FlowStitcher;

impl FlowStitcher {
    /// Stitches normalized child video streams in strict segmentIndex order and muxes full original audio ONCE.
    pub fn stitch_long_video_timeline(
        normalized_child_paths: &[PathBuf],
        source_audio_path: Option<&Path>,
        expected_total_frames: u64,
        target_fps: f64,
        output_file_path: &Path,
    ) -> Result<(FlowOutputArtifactRecord, FlowAudioRestorationMode), String> {
        if normalized_child_paths.is_empty() {
            return Err(
                "STITCH_FAILED: No normalized segment paths provided for stitching".to_string(),
            );
        }

        if let Some(parent) = output_file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Prepare concat list file in strict input order
        let list_file = output_file_path.with_extension("concat_list.txt");
        let mut list_content = String::new();
        for path in normalized_child_paths {
            let path_str = path.to_string_lossy().replace('\\', "/");
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
            if audio_src.exists() {
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

        // Final output validation
        let expected_duration_sec = expected_total_frames as f64 / target_fps;
        let final_record =
            FlowOutputValidator::validate_child_artifact(output_file_path, expected_duration_sec)?;

        Ok((final_record, audio_mode))
    }

    /// Legacy stitcher retained for backward compatibility.
    pub fn stitch_flow_segments(
        segment_paths: &[PathBuf],
        source_audio_path: Option<&Path>,
        expected_total_duration_sec: f64,
        audio_policy: &FlowFinalAudioPolicy,
        output_file_path: &Path,
    ) -> Result<FlowOutputArtifactRecord, String> {
        if segment_paths.is_empty() {
            return Err("STITCH_FAILED: No segment paths provided for stitching".to_string());
        }

        if let Some(parent) = output_file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Prepare concat list file
        let list_file = output_file_path.with_extension("concat.txt");
        let mut list_content = String::new();
        for path in segment_paths {
            let path_str = path.to_string_lossy().replace('\\', "/");
            list_content.push_str(&format!("file '{}'\n", path_str));
        }
        fs::write(&list_file, &list_content)
            .map_err(|e| format!("Failed to write concat list: {}", e))?;

        // Stitched video without audio first
        let temp_video = output_file_path.with_extension("temp_video.mp4");

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y")
            .arg("-f")
            .arg("concat")
            .arg("-safe")
            .arg("0")
            .arg("-i")
            .arg(&list_file)
            .arg("-c:v")
            .arg("libx264")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-an") // Discard child audio tracks during concat
            .arg(&temp_video);

        let output = cmd
            .output()
            .map_err(|e| format!("FFmpeg video concatenation failed: {}", e))?;

        let _ = fs::remove_file(&list_file);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("FFmpeg concat demuxer failed: {}", stderr));
        }

        // Duration Safety: Validate intermediate concatenated video duration before audio muxing
        if let Err(err) =
            FlowOutputValidator::validate_child_artifact(&temp_video, expected_total_duration_sec)
        {
            let _ = fs::remove_file(&temp_video);
            return Err(err);
        }

        // Mux original source audio ONCE into final output if requested and available
        if audio_policy.preserve_original_audio
            && source_audio_path.is_some()
            && source_audio_path.unwrap().exists()
        {
            let src_audio = source_audio_path.unwrap();
            let mut mux_cmd = Command::new("ffmpeg");
            mux_cmd
                .arg("-y")
                .arg("-i")
                .arg(&temp_video)
                .arg("-i")
                .arg(src_audio)
                .arg("-map")
                .arg("0:v:0")
                .arg("-map")
                .arg("1:a:0?")
                .arg("-c:v")
                .arg("copy")
                .arg("-c:a")
                .arg("aac")
                .arg("-b:a")
                .arg("192k")
                .arg(output_file_path);

            let mux_out = mux_cmd
                .output()
                .map_err(|e| format!("FFmpeg audio muxing failed: {}", e))?;

            let _ = fs::remove_file(&temp_video);

            if !mux_out.status.success() {
                let stderr = String::from_utf8_lossy(&mux_out.stderr);
                return Err(format!("FFmpeg audio muxing failed: {}", stderr));
            }
        } else {
            #[cfg(target_os = "windows")]
            {
                if output_file_path.exists() {
                    let _ = fs::remove_file(output_file_path);
                }
                let _ = fs::rename(&temp_video, output_file_path);
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = fs::rename(&temp_video, output_file_path);
            }
        }

        // Validate final stitched artifact
        let final_record = FlowOutputValidator::validate_child_artifact(
            output_file_path,
            expected_total_duration_sec,
        )?;

        Ok(final_record)
    }
}
