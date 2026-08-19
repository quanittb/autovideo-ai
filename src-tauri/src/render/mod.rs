use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use crate::error::AppError;
use crate::events::parse_ffmpeg_progress_line;
use crate::media::MediaService;
use crate::projects::{ProjectOutput, SourceMedia};

pub const RENDER_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderRequest {
    pub project_id: String,
    pub media_id: String,
    pub frame_directory: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
    pub fps: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub output_format: Option<String>,
    pub output_name: Option<String>,
    pub mode: Option<String>, // "test_1s", "test_3s", "full"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderOutputMetadata {
    pub valid: bool,
    pub output_path: PathBuf,
    pub duration_ms: u64,
    pub duration_seconds: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    pub has_audio: bool,
    pub file_size_bytes: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceVsOutputComparison {
    pub mode: String, // "TEST_1S", "TEST_3S", "FULL"
    pub source_duration_seconds: f64,
    pub output_duration_seconds: f64,
    pub duration_delta_seconds: f64,
    pub source_resolution: String,
    pub output_resolution: String,
    pub source_fps: f64,
    pub output_fps: f64,
    pub source_has_audio: bool,
    pub output_has_audio: bool,
    pub resolution_matches: bool,
    pub fps_matches: bool,
    pub audio_matches: bool,
    pub expected_frame_count: u64,
    pub actual_frame_count: u64,
    pub frame_count_matches: bool,
    pub duration_tolerance_seconds: f64,
    pub is_full_match: bool,
    pub timing_explanation: String,
    pub is_compatible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub job_id: String,
    pub project_id: String,
    pub media_id: String,
    pub mode: String,
    pub output_metadata: RenderOutputMetadata,
    pub comparison: SourceVsOutputComparison,
    pub manifest_path: PathBuf,
    pub project_output: ProjectOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderManifest {
    pub schema_version: u32,
    pub job_id: String,
    pub source_media_id: String,
    pub mode: String,
    pub frame_directory: String,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub audio_source: Option<String>,
    pub output_path: String,
    pub created_at: String,
    pub output_metadata: RenderOutputMetadata,
}

#[derive(Default)]
pub struct RenderService;

impl RenderService {
    pub fn new() -> Self {
        Self
    }

    /// Validates frame sequence on disk, executes real FFmpeg assembly with audio muxing, validates output, and writes render.json.
    pub fn render_video(
        &self,
        project_dir: &Path,
        source_media: &SourceMedia,
        request: &RenderRequest,
    ) -> Result<RenderResult, AppError> {
        self.render_video_with_progress(project_dir, source_media, request, &mut |_| {})
    }

    /// Validates frame sequence on disk, executes real FFmpeg assembly with audio muxing and live progress reporting.
    pub fn render_video_with_progress<F>(
        &self,
        project_dir: &Path,
        source_media: &SourceMedia,
        request: &RenderRequest,
        progress_callback: &mut F,
    ) -> Result<RenderResult, AppError>
    where
        F: FnMut(f32),
    {
        self.render_video_with_progress_and_cancel(
            project_dir,
            source_media,
            request,
            progress_callback,
            None,
            None,
            None,
        )
    }

    /// Validates frame sequence on disk, executes real FFmpeg assembly with audio muxing, live progress reporting, and cancellation support.
    pub fn render_video_with_progress_and_cancel<F>(
        &self,
        project_dir: &Path,
        source_media: &SourceMedia,
        request: &RenderRequest,
        progress_callback: &mut F,
        cancel_token: Option<Arc<AtomicBool>>,
        mut on_spawn_pid: Option<&mut dyn FnMut(u32)>,
        mut on_exit_pid: Option<&mut dyn FnMut(u32)>,
    ) -> Result<RenderResult, AppError>
    where
        F: FnMut(f32),
    {
        let media_service = MediaService::new();
        let runtime = media_service.check_runtime_status();
        if !runtime.ffmpeg.available {
            return Err(AppError::ffmpeg_not_available(
                "FFmpeg is required for video rendering",
            ));
        }

        // Check cancellation before start
        if let Some(ref ct) = cancel_token {
            if ct.load(Ordering::SeqCst) {
                return Err(AppError::cancelled());
            }
        }

        let mode_raw = request.mode.as_deref().unwrap_or("test_1s");
        let render_mode = match mode_raw.to_lowercase().as_str() {
            "full" => "FULL",
            "test_3s" => "TEST_3S",
            _ => "TEST_1S",
        };

        // 1. Resolve and validate frames directory
        let frames_dir = request.frame_directory.clone().unwrap_or_else(|| {
            project_dir
                .join("cache")
                .join("media")
                .join(&request.media_id)
                .join("frames")
        });

        if !frames_dir.exists() || !frames_dir.is_dir() {
            return Err(AppError::output_not_found(format!(
                "Frames directory does not exist: {}",
                frames_dir.display()
            )));
        }

        let mut frame_entries = Vec::new();
        if let Ok(entries) = fs::read_dir(&frames_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                        if ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("jpg") {
                            frame_entries.push(path);
                        }
                    }
                }
            }
        }

        if frame_entries.is_empty() {
            return Err(AppError::frame_sequence_invalid(
                "No valid image frames found in frame directory",
                frames_dir.display().to_string(),
            ));
        }

        // Sort numerically
        frame_entries.sort_by_key(|p| p.file_name().unwrap_or_default().to_os_string());

        // Validate frame files exist & have non-zero length
        for frame in &frame_entries {
            let metadata = fs::metadata(frame).map_err(|e| {
                AppError::frame_sequence_invalid(
                    "Failed to read frame file metadata",
                    e.to_string(),
                )
            })?;
            if metadata.len() == 0 {
                return Err(AppError::frame_sequence_invalid(
                    "Encountered empty zero-byte frame in sequence",
                    frame.display().to_string(),
                ));
            }
        }

        let actual_frame_count = frame_entries.len() as u64;

        // 2. Prepare isolated outputs workspace
        let job_id = format!("render-{}", Uuid::new_v4());
        let output_folder = project_dir.join("outputs").join(&job_id);
        fs::create_dir_all(&output_folder).map_err(|e| {
            AppError::render_failed("Failed to create output directory", e.to_string())
        })?;

        let default_output_name = match render_mode {
            "FULL" => "reconstructed_full.mp4".to_string(),
            "TEST_3S" => "reconstructed_3s.mp4".to_string(),
            _ => "reconstructed_1s.mp4".to_string(),
        };

        let output_file_name = request.output_name.clone().unwrap_or(default_output_name);
        let output_path = output_folder.join(&output_file_name);

        // 3. Resolve audio path
        let audio_candidate = request.audio_path.clone().unwrap_or_else(|| {
            project_dir
                .join("cache")
                .join("media")
                .join(&request.media_id)
                .join("audio")
                .join("source.wav")
        });

        let has_usable_audio = source_media.has_audio && audio_candidate.exists();

        // 4. Resolve FPS & compute accurate expected video duration
        let target_fps = request.fps.unwrap_or(source_media.fps);
        let expected_duration_seconds = actual_frame_count as f64 / target_fps;
        let input_pattern = frames_dir.join("%06d.png");

        // 5. Construct FFmpeg command
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y"); // Overwrite output file
        cmd.arg("-progress").arg("pipe:1"); // Machine readable progress on stdout
        cmd.arg("-framerate").arg(format!("{:.3}", target_fps));
        cmd.arg("-start_number").arg("0");
        cmd.arg("-i").arg(input_pattern.to_str().unwrap());

        if has_usable_audio {
            cmd.arg("-i").arg(audio_candidate.to_str().unwrap());
        }

        cmd.arg("-c:v").arg("libx264");
        cmd.arg("-pix_fmt").arg("yuv420p");

        if has_usable_audio {
            cmd.arg("-c:a").arg("aac");
            cmd.arg("-b:a").arg("128k");
            // Accurately cut audio to match frame sequence duration without truncation artifacts
            cmd.arg("-t")
                .arg(format!("{:.3}", expected_duration_seconds));
        }

        cmd.arg(output_path.to_str().unwrap());

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let total_frames_f64 = actual_frame_count as f64;

        let mut child = cmd.spawn().map_err(|e| {
            AppError::render_failed("Failed to invoke ffmpeg encoding process", e.to_string())
        })?;

        let pid = child.id();
        if let Some(ref mut cb) = on_spawn_pid {
            cb(pid);
        }

        let mut was_cancelled = false;
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                if let Some(ref ct) = cancel_token {
                    if ct.load(Ordering::SeqCst) {
                        was_cancelled = true;
                        break;
                    }
                }
                if let Some((k, v)) = parse_ffmpeg_progress_line(&line) {
                    if k == "frame" {
                        if let Ok(frame_num) = v.parse::<f64>() {
                            let percent =
                                ((frame_num / total_frames_f64) * 100.0).clamp(0.0, 99.0) as f32;
                            progress_callback(percent);
                        }
                    } else if k == "progress" && v == "end" {
                        progress_callback(100.0);
                    }
                }
            }
        }

        if was_cancelled
            || cancel_token
                .as_ref()
                .map(|ct| ct.load(Ordering::SeqCst))
                .unwrap_or(false)
        {
            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output();
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
            }
            let _ = child.kill();
            let _ = child.wait();
            if let Some(ref mut cb) = on_exit_pid {
                cb(pid);
            }
            if output_path.exists() {
                let _ = fs::remove_file(&output_path);
            }
            return Err(AppError::cancelled());
        }

        let output = child.wait_with_output().map_err(|e| {
            AppError::render_failed("Failed to wait on ffmpeg process", e.to_string())
        })?;

        if let Some(ref mut cb) = on_exit_pid {
            cb(pid);
        }

        if !output.status.success() {
            if cancel_token
                .as_ref()
                .map(|ct| ct.load(Ordering::SeqCst))
                .unwrap_or(false)
            {
                if output_path.exists() {
                    let _ = fs::remove_file(&output_path);
                }
                return Err(AppError::cancelled());
            }
            let stderr_msg = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::render_failed(
                "FFmpeg video reconstruction failed with non-zero exit code",
                stderr_msg.to_string(),
            ));
        }

        progress_callback(100.0);

        // 6. Validate generated video output on disk with FFprobe
        if !output_path.exists() {
            return Err(AppError::output_not_found(
                output_path.display().to_string(),
            ));
        }

        let output_probe = media_service.probe(&output_path).map_err(|e| {
            AppError::output_metadata_failed(
                "FFprobe validation failed on reconstructed MP4",
                e.message,
            )
        })?;

        let now = Utc::now().to_rfc3339();
        let output_metadata = RenderOutputMetadata {
            valid: true,
            output_path: output_path.clone(),
            duration_ms: output_probe.duration_ms,
            duration_seconds: output_probe.duration_ms as f64 / 1000.0,
            width: output_probe.width,
            height: output_probe.height,
            fps: output_probe.fps,
            video_codec: output_probe.video_codec.clone(),
            audio_codec: output_probe.audio_codec.clone(),
            has_audio: output_probe.has_audio,
            file_size_bytes: output_probe.file_size_bytes,
            created_at: now.clone(),
        };

        // 7. Source vs Output Comparison & Duration Audit
        let source_dur_sec = source_media.duration_ms as f64 / 1000.0;
        let out_dur_sec = output_metadata.duration_seconds;

        let expected_frame_count = match render_mode {
            "TEST_1S" => (1.0 * target_fps).round() as u64,
            "TEST_3S" => (3.0 * target_fps).round() as u64,
            _ => (source_dur_sec * target_fps).round() as u64,
        };

        let duration_tolerance_seconds = match render_mode {
            "FULL" => 0.10,
            _ => 0.05,
        };

        let delta_sec = (out_dur_sec
            - if render_mode == "FULL" {
                source_dur_sec
            } else {
                expected_duration_seconds
            })
        .abs();
        let is_full_match = render_mode == "FULL"
            && delta_sec <= 0.10
            && source_media.width == output_metadata.width
            && source_media.height == output_metadata.height;

        let timing_explanation = format!(
            "Mode: {}, Extracted: {} frames @ {:.2} FPS (expected: {} frames). Render Duration: {:.2}s (delta: {:.3}s)",
            render_mode,
            actual_frame_count,
            target_fps,
            expected_frame_count,
            out_dur_sec,
            delta_sec
        );

        let comparison = SourceVsOutputComparison {
            mode: render_mode.to_string(),
            source_duration_seconds: source_dur_sec,
            output_duration_seconds: out_dur_sec,
            duration_delta_seconds: delta_sec,
            source_resolution: format!("{}x{}", source_media.width, source_media.height),
            output_resolution: format!("{}x{}", output_metadata.width, output_metadata.height),
            source_fps: source_media.fps,
            output_fps: output_metadata.fps,
            source_has_audio: source_media.has_audio,
            output_has_audio: output_metadata.has_audio,
            resolution_matches: source_media.width == output_metadata.width
                && source_media.height == output_metadata.height,
            fps_matches: (source_media.fps - output_metadata.fps).abs() < 0.1,
            audio_matches: source_media.has_audio == output_metadata.has_audio,
            expected_frame_count,
            actual_frame_count,
            frame_count_matches: actual_frame_count == expected_frame_count
                || (actual_frame_count as i64 - expected_frame_count as i64).abs() <= 1,
            duration_tolerance_seconds,
            is_full_match,
            timing_explanation,
            is_compatible: true,
        };

        // 8. Write render.json manifest
        let manifest = RenderManifest {
            schema_version: RENDER_MANIFEST_SCHEMA_VERSION,
            job_id: job_id.clone(),
            source_media_id: request.media_id.clone(),
            mode: render_mode.to_string(),
            frame_directory: frames_dir.display().to_string(),
            fps: target_fps,
            width: output_metadata.width,
            height: output_metadata.height,
            audio_source: if has_usable_audio {
                Some(audio_candidate.display().to_string())
            } else {
                None
            },
            output_path: output_path.display().to_string(),
            created_at: now.clone(),
            output_metadata: output_metadata.clone(),
        };

        let manifest_path = output_folder.join("render.json");
        if let Ok(serialized) = serde_json::to_string_pretty(&manifest) {
            let _ = fs::write(&manifest_path, serialized);
        }

        let project_output = ProjectOutput {
            output_id: job_id.clone(),
            file_name: output_file_name,
            file_path: output_path,
            file_size_bytes: output_metadata.file_size_bytes,
            duration_ms: output_metadata.duration_ms,
            width: output_metadata.width,
            height: output_metadata.height,
            fps: output_metadata.fps,
            created_at: now,
        };

        Ok(RenderResult {
            job_id,
            project_id: request.project_id.clone(),
            media_id: request.media_id.clone(),
            mode: render_mode.to_string(),
            output_metadata,
            comparison,
            manifest_path,
            project_output,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_render_service_test1s_mode() {
        let media_service = MediaService::new();
        let runtime = media_service.check_runtime_status();
        if !runtime.ffmpeg.available || !runtime.ffprobe.available {
            return;
        }

        let temp = tempdir().unwrap();
        let proj_dir = temp.path().join("proj_test1s");
        let source_video = temp.path().join("source_1s.mp4");

        // 1. Generate 3-second 30 FPS source video with sine audio
        let gen_status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=3:size=320x240:rate=30",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:duration=3",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                source_video.to_str().unwrap(),
            ])
            .output();

        if let Ok(out) = gen_status {
            if !out.status.success() {
                return;
            }
        } else {
            return;
        }

        let imported = media_service
            .import_to_project(&proj_dir, &source_video)
            .expect("Import failed");

        // Extract exactly 1.0s (30 frames at 30 FPS)
        let frame_req = crate::media::FrameExtractionRequest {
            project_id: "proj_test1s".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: Some(0.0),
            end_time_seconds: Some(1.0),
            fps: Some(30.0),
            width: Some(320),
            height: Some(240),
            format: Some("png".to_string()),
        };

        let frame_res = media_service
            .extract_frames(&proj_dir, &imported.source_path, &frame_req)
            .expect("Extract frames failed");
        assert_eq!(frame_res.frame_count, 30); // 30 frames for 1.0s

        let audio_res = media_service
            .extract_audio(&proj_dir, &imported.source_path, &imported.media_id)
            .expect("Extract audio failed");

        let render_service = RenderService::new();
        let render_req = RenderRequest {
            project_id: "proj_test1s".to_string(),
            media_id: imported.media_id.clone(),
            frame_directory: Some(frame_res.frames_dir),
            audio_path: audio_res.audio_path,
            fps: Some(30.0),
            width: Some(320),
            height: Some(240),
            output_format: Some("mp4".to_string()),
            output_name: Some("reconstructed_1s.mp4".to_string()),
            mode: Some("test_1s".to_string()),
        };

        let result = render_service
            .render_video(&proj_dir, &imported, &render_req)
            .expect("Render test 1s failed");
        assert_eq!(result.mode, "TEST_1S");
        assert!(result.output_metadata.valid);
        assert!((result.output_metadata.duration_seconds - 1.0).abs() <= 0.05);
        assert_eq!(result.comparison.actual_frame_count, 30);
    }

    #[test]
    fn test_render_service_test3s_mode() {
        let media_service = MediaService::new();
        let runtime = media_service.check_runtime_status();
        if !runtime.ffmpeg.available || !runtime.ffprobe.available {
            return;
        }

        let temp = tempdir().unwrap();
        let proj_dir = temp.path().join("proj_test3s");
        let source_video = temp.path().join("source_3s.mp4");

        let gen_status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=5:size=320x240:rate=30",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:duration=5",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                source_video.to_str().unwrap(),
            ])
            .output();

        if let Ok(out) = gen_status {
            if !out.status.success() {
                return;
            }
        } else {
            return;
        }

        let imported = media_service
            .import_to_project(&proj_dir, &source_video)
            .expect("Import failed");

        // Extract exactly 3.0s (90 frames at 30 FPS)
        let frame_req = crate::media::FrameExtractionRequest {
            project_id: "proj_test3s".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: Some(0.0),
            end_time_seconds: Some(3.0),
            fps: Some(30.0),
            width: Some(320),
            height: Some(240),
            format: Some("png".to_string()),
        };

        let frame_res = media_service
            .extract_frames(&proj_dir, &imported.source_path, &frame_req)
            .expect("Extract frames failed");
        assert_eq!(frame_res.frame_count, 90); // 90 frames for 3.0s

        let audio_res = media_service
            .extract_audio(&proj_dir, &imported.source_path, &imported.media_id)
            .expect("Extract audio failed");

        let render_service = RenderService::new();
        let render_req = RenderRequest {
            project_id: "proj_test3s".to_string(),
            media_id: imported.media_id.clone(),
            frame_directory: Some(frame_res.frames_dir),
            audio_path: audio_res.audio_path,
            fps: Some(30.0),
            width: Some(320),
            height: Some(240),
            output_format: Some("mp4".to_string()),
            output_name: Some("reconstructed_3s.mp4".to_string()),
            mode: Some("test_3s".to_string()),
        };

        let result = render_service
            .render_video(&proj_dir, &imported, &render_req)
            .expect("Render test 3s failed");
        assert_eq!(result.mode, "TEST_3S");
        assert!(result.output_metadata.valid);
        assert!((result.output_metadata.duration_seconds - 3.0).abs() <= 0.05);
        assert_eq!(result.comparison.actual_frame_count, 90);
    }

    #[test]
    fn test_render_service_full_reconstruction_live() {
        let video_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !video_path.exists() {
            return;
        }

        let media_service = MediaService::new();
        let render_service = RenderService::new();
        let proj_dir = PathBuf::from(
            r"d:\rustProject\autovideo-ai\.autovideo_data\projects\proj_render_full_live",
        );
        let _ = fs::create_dir_all(&proj_dir);

        let imported = media_service
            .import_to_project(&proj_dir, &video_path)
            .expect("Import failed");

        // 1. Extract ALL frames at source FPS (30 FPS for 5.0s = 150 frames)
        let frame_req = crate::media::FrameExtractionRequest {
            project_id: "proj_render_full_live".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: None,
            end_time_seconds: None,
            fps: Some(imported.fps),
            width: None,
            height: None,
            format: Some("png".to_string()),
        };
        let frame_res = media_service
            .extract_frames(&proj_dir, &imported.source_path, &frame_req)
            .expect("Extract all frames failed");
        assert_eq!(frame_res.frame_count, 150);

        let audio_res = media_service
            .extract_audio(&proj_dir, &imported.source_path, &imported.media_id)
            .expect("Extract audio failed");

        let render_req = RenderRequest {
            project_id: "proj_render_full_live".to_string(),
            media_id: imported.media_id.clone(),
            frame_directory: Some(frame_res.frames_dir),
            audio_path: audio_res.audio_path,
            fps: Some(imported.fps),
            width: Some(imported.width),
            height: Some(imported.height),
            output_format: Some("mp4".to_string()),
            output_name: Some("reconstructed_full.mp4".to_string()),
            mode: Some("full".to_string()),
        };

        let result = render_service
            .render_video(&proj_dir, &imported, &render_req)
            .expect("Full render failed");

        println!("[PHASE 4C FULL RECONSTRUCTION PASS]");
        println!(
            "Output Video: {}",
            result.output_metadata.output_path.display()
        );
        println!(
            "Source Duration: {:.2}s | Output Duration: {:.2}s (Delta: {:.3}s)",
            result.comparison.source_duration_seconds,
            result.comparison.output_duration_seconds,
            result.comparison.duration_delta_seconds
        );
        println!(
            "Resolution: {}x{}",
            result.output_metadata.width, result.output_metadata.height
        );
        println!("FPS: {:.2}", result.output_metadata.fps);
        println!(
            "Frames: {} / {}",
            result.comparison.actual_frame_count, result.comparison.expected_frame_count
        );
        println!("Is Full Match: {}", result.comparison.is_full_match);

        assert!(result.output_metadata.valid);
        assert!(result.comparison.is_full_match);
        assert_eq!(result.output_metadata.width, 1080);
        assert_eq!(result.output_metadata.height, 1920);
        assert_eq!(result.comparison.actual_frame_count, 150);
        assert!(result.comparison.duration_delta_seconds <= 0.10);
    }

    #[test]
    fn test_render_service_douyin_modes() {
        let video_path =
            PathBuf::from(r"C:\Users\quant\Dropbox\PC\Downloads\Douyin_1782229041.mp4");
        if !video_path.exists() {
            return;
        }

        let media_service = MediaService::new();
        let render_service = RenderService::new();
        let proj_dir = PathBuf::from(
            r"d:\rustProject\autovideo-ai\.autovideo_data\projects\proj_render_douyin_audit",
        );
        let _ = fs::create_dir_all(&proj_dir);

        let imported = media_service
            .import_to_project(&proj_dir, &video_path)
            .expect("Import failed");
        let audio_res = media_service
            .extract_audio(&proj_dir, &imported.source_path, &imported.media_id)
            .expect("Audio failed");

        // TEST 1: 1-Second Reconstruction (30 frames at 30 FPS)
        let frame_req_1s = crate::media::FrameExtractionRequest {
            project_id: "proj_render_douyin_audit".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: Some(0.0),
            end_time_seconds: Some(1.0),
            fps: Some(imported.fps),
            width: None,
            height: None,
            format: Some("png".to_string()),
        };
        let frame_res_1s = media_service
            .extract_frames(&proj_dir, &imported.source_path, &frame_req_1s)
            .expect("1s frame extraction failed");
        assert_eq!(frame_res_1s.frame_count, 30);

        let render_req_1s = RenderRequest {
            project_id: "proj_render_douyin_audit".to_string(),
            media_id: imported.media_id.clone(),
            frame_directory: Some(frame_res_1s.frames_dir),
            audio_path: audio_res.audio_path.clone(),
            fps: Some(imported.fps),
            width: Some(imported.width),
            height: Some(imported.height),
            output_format: Some("mp4".to_string()),
            output_name: Some("reconstructed_1s.mp4".to_string()),
            mode: Some("test_1s".to_string()),
        };
        let res_1s = render_service
            .render_video(&proj_dir, &imported, &render_req_1s)
            .expect("Render 1s failed");
        println!(
            "[TEST 1 — 1 SECOND RECONSTRUCTION]: Output={}, Duration={:.2}s, Frames={}, FPS={:.2}",
            res_1s.output_metadata.output_path.display(),
            res_1s.output_metadata.duration_seconds,
            res_1s.comparison.actual_frame_count,
            res_1s.output_metadata.fps
        );
        assert!((res_1s.output_metadata.duration_seconds - 1.0).abs() <= 0.05);

        // TEST 2: 3-Second Reconstruction (90 frames at 30 FPS)
        let frame_req_3s = crate::media::FrameExtractionRequest {
            project_id: "proj_render_douyin_audit".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: Some(0.0),
            end_time_seconds: Some(3.0),
            fps: Some(imported.fps),
            width: None,
            height: None,
            format: Some("png".to_string()),
        };
        let frame_res_3s = media_service
            .extract_frames(&proj_dir, &imported.source_path, &frame_req_3s)
            .expect("3s frame extraction failed");
        assert_eq!(frame_res_3s.frame_count, 90);

        let render_req_3s = RenderRequest {
            project_id: "proj_render_douyin_audit".to_string(),
            media_id: imported.media_id.clone(),
            frame_directory: Some(frame_res_3s.frames_dir),
            audio_path: audio_res.audio_path.clone(),
            fps: Some(imported.fps),
            width: Some(imported.width),
            height: Some(imported.height),
            output_format: Some("mp4".to_string()),
            output_name: Some("reconstructed_3s.mp4".to_string()),
            mode: Some("test_3s".to_string()),
        };
        let res_3s = render_service
            .render_video(&proj_dir, &imported, &render_req_3s)
            .expect("Render 3s failed");
        println!(
            "[TEST 2 — 3 SECOND RECONSTRUCTION]: Output={}, Duration={:.2}s, Frames={}, FPS={:.2}",
            res_3s.output_metadata.output_path.display(),
            res_3s.output_metadata.duration_seconds,
            res_3s.comparison.actual_frame_count,
            res_3s.output_metadata.fps
        );
        assert!((res_3s.output_metadata.duration_seconds - 3.0).abs() <= 0.05);

        // TEST 3: Full Reconstruction (All 730 frames at 30 FPS)
        let frame_req_full = crate::media::FrameExtractionRequest {
            project_id: "proj_render_douyin_audit".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: None,
            end_time_seconds: None,
            fps: Some(imported.fps),
            width: None,
            height: None,
            format: Some("png".to_string()),
        };
        let frame_res_full = media_service
            .extract_frames(&proj_dir, &imported.source_path, &frame_req_full)
            .expect("Full frame extraction failed");
        assert_eq!(frame_res_full.frame_count, 730);

        let render_req_full = RenderRequest {
            project_id: "proj_render_douyin_audit".to_string(),
            media_id: imported.media_id.clone(),
            frame_directory: Some(frame_res_full.frames_dir),
            audio_path: audio_res.audio_path,
            fps: Some(imported.fps),
            width: Some(imported.width),
            height: Some(imported.height),
            output_format: Some("mp4".to_string()),
            output_name: Some("reconstructed_full.mp4".to_string()),
            mode: Some("full".to_string()),
        };
        let res_full = render_service
            .render_video(&proj_dir, &imported, &render_req_full)
            .expect("Full render failed");
        println!("[TEST 3 — FULL RECONSTRUCTION]: Output={}, Duration={:.2}s (Source={:.2}s, Delta={:.3}s), Frames={}, FPS={:.2}",
            res_full.output_metadata.output_path.display(),
            res_full.output_metadata.duration_seconds,
            res_full.comparison.source_duration_seconds,
            res_full.comparison.duration_delta_seconds,
            res_full.comparison.actual_frame_count,
            res_full.output_metadata.fps
        );
        assert!(res_full.comparison.is_full_match);
        assert!(res_full.comparison.duration_delta_seconds <= 0.10);
    }
}
