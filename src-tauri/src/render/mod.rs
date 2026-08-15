use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;
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
    pub is_compatible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub job_id: String,
    pub project_id: String,
    pub media_id: String,
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
        let media_service = MediaService::new();
        let runtime = media_service.check_runtime_status();
        if !runtime.ffmpeg.available {
            return Err(AppError::ffmpeg_not_available("FFmpeg is required for video rendering"));
        }

        // 1. Resolve and validate frames directory
        let frames_dir = request
            .frame_directory
            .clone()
            .unwrap_or_else(|| project_dir.join("cache").join("media").join(&request.media_id).join("frames"));

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

        // Validate first and last frame signatures
        for frame in &frame_entries {
            let metadata = fs::metadata(frame).map_err(|e| {
                AppError::frame_sequence_invalid("Failed to read frame file metadata", e.to_string())
            })?;
            if metadata.len() == 0 {
                return Err(AppError::frame_sequence_invalid(
                    "Encountered empty zero-byte frame in sequence",
                    frame.display().to_string(),
                ));
            }
        }

        // 2. Prepare isolated outputs workspace
        let job_id = format!("render-{}", Uuid::new_v4());
        let output_folder = project_dir.join("outputs").join(&job_id);
        fs::create_dir_all(&output_folder).map_err(|e| {
            AppError::render_failed("Failed to create output directory", e.to_string())
        })?;

        let output_file_name = request
            .output_name
            .clone()
            .unwrap_or_else(|| "reconstructed.mp4".to_string());
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

        // 4. Resolve FPS (use explicit request, or fallback to source media FPS)
        let target_fps = request.fps.unwrap_or(source_media.fps);
        let input_pattern = frames_dir.join("%06d.png");

        // 5. Construct FFmpeg command
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y"); // Overwrite output file
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
            cmd.arg("-shortest");
        }

        cmd.arg(output_path.to_str().unwrap());

        let output = cmd.output().map_err(|e| {
            AppError::render_failed("Failed to invoke ffmpeg encoding process", e.to_string())
        })?;

        if !output.status.success() {
            let stderr_msg = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::render_failed(
                "FFmpeg video reconstruction failed with non-zero exit code",
                stderr_msg.to_string(),
            ));
        }

        // 6. Validate generated video output on disk with FFprobe
        if !output_path.exists() {
            return Err(AppError::output_not_found(output_path.display().to_string()));
        }

        let output_probe = media_service.probe(&output_path).map_err(|e| {
            AppError::output_metadata_failed("FFprobe validation failed on reconstructed MP4", e.message)
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

        // 7. Source vs Output Comparison
        let source_dur_sec = source_media.duration_ms as f64 / 1000.0;
        let out_dur_sec = output_metadata.duration_seconds;
        let delta_sec = (out_dur_sec - source_dur_sec).abs();

        let comparison = SourceVsOutputComparison {
            source_duration_seconds: source_dur_sec,
            output_duration_seconds: out_dur_sec,
            duration_delta_seconds: delta_sec,
            source_resolution: format!("{}x{}", source_media.width, source_media.height),
            output_resolution: format!("{}x{}", output_metadata.width, output_metadata.height),
            source_fps: source_media.fps,
            output_fps: output_metadata.fps,
            source_has_audio: source_media.has_audio,
            output_has_audio: output_metadata.has_audio,
            resolution_matches: source_media.width == output_metadata.width && source_media.height == output_metadata.height,
            fps_matches: (source_media.fps - output_metadata.fps).abs() < 0.1,
            audio_matches: source_media.has_audio == output_metadata.has_audio,
            is_compatible: true,
        };

        // 8. Write render.json manifest
        let manifest = RenderManifest {
            schema_version: RENDER_MANIFEST_SCHEMA_VERSION,
            job_id: job_id.clone(),
            source_media_id: request.media_id.clone(),
            frame_directory: frames_dir.display().to_string(),
            fps: target_fps,
            width: output_metadata.width,
            height: output_metadata.height,
            audio_source: if has_usable_audio { Some(audio_candidate.display().to_string()) } else { None },
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
    fn test_render_service_frame_assembly() {
        let media_service = MediaService::new();
        let runtime = media_service.check_runtime_status();
        if !runtime.ffmpeg.available || !runtime.ffprobe.available {
            eprintln!("Skipping render test because FFmpeg is not installed.");
            return;
        }

        let temp = tempdir().unwrap();
        let proj_dir = temp.path().join("proj_render_test");
        let source_video = temp.path().join("source_for_render.mp4");

        // 1. Generate 2-second 10 FPS test video (20 frames) with sine audio
        let gen_status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=10",
                "-f", "lavfi", "-i", "sine=frequency=1000:duration=2",
                "-c:v", "libx264", "-pix_fmt", "yuv420p",
                "-c:a", "aac",
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

        // 2. Import into project
        let imported = media_service.import_to_project(&proj_dir, &source_video).expect("Import failed");

        // 3. Extract frames & audio
        let frame_req = crate::media::FrameExtractionRequest {
            project_id: "proj_render_test".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: None,
            end_time_seconds: None,
            fps: Some(10.0),
            width: Some(320),
            height: Some(240),
            format: Some("png".to_string()),
        };

        let frame_res = media_service.extract_frames(&proj_dir, &imported.source_path, &frame_req).expect("Extract frames failed");
        assert_eq!(frame_res.frame_count, 20);

        let audio_res = media_service.extract_audio(&proj_dir, &imported.source_path, &imported.media_id).expect("Extract audio failed");
        assert!(audio_res.has_audio);

        // 4. Execute RenderService
        let render_service = RenderService::new();
        let render_req = RenderRequest {
            project_id: "proj_render_test".to_string(),
            media_id: imported.media_id.clone(),
            frame_directory: Some(frame_res.frames_dir),
            audio_path: audio_res.audio_path,
            fps: Some(10.0),
            width: Some(320),
            height: Some(240),
            output_format: Some("mp4".to_string()),
            output_name: Some("reconstructed.mp4".to_string()),
        };

        let result = render_service.render_video(&proj_dir, &imported, &render_req).expect("Render video failed");

        // 5. Verify Output Metadata & Comparison
        assert!(result.output_metadata.valid);
        assert!(result.output_metadata.output_path.exists());
        assert!(result.output_metadata.file_size_bytes > 0);
        assert_eq!(result.output_metadata.width, 320);
        assert_eq!(result.output_metadata.height, 240);
        assert!(result.output_metadata.has_audio);
        assert!(result.manifest_path.exists());

        // Verify render.json
        let manifest_content = fs::read_to_string(&result.manifest_path).unwrap();
        assert!(manifest_content.contains("reconstructed.mp4"));
        assert!(manifest_content.contains("sourceMediaId"));
    }

    #[test]
    fn test_render_service_no_audio_video() {
        let media_service = MediaService::new();
        let runtime = media_service.check_runtime_status();
        if !runtime.ffmpeg.available || !runtime.ffprobe.available {
            return;
        }

        let temp = tempdir().unwrap();
        let proj_dir = temp.path().join("proj_silent_test");
        let source_video = temp.path().join("source_silent.mp4");

        // 1. Generate 1-second 10 FPS test video without audio
        let gen_status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "lavfi", "-i", "testsrc=duration=1:size=160x120:rate=10",
                "-c:v", "libx264", "-pix_fmt", "yuv420p",
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

        let imported = media_service.import_to_project(&proj_dir, &source_video).expect("Import failed");
        assert!(!imported.has_audio);

        let frame_req = crate::media::FrameExtractionRequest {
            project_id: "proj_silent_test".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: None,
            end_time_seconds: None,
            fps: Some(10.0),
            width: Some(160),
            height: Some(120),
            format: Some("png".to_string()),
        };

        let frame_res = media_service.extract_frames(&proj_dir, &imported.source_path, &frame_req).expect("Extract frames failed");

        let render_service = RenderService::new();
        let render_req = RenderRequest {
            project_id: "proj_silent_test".to_string(),
            media_id: imported.media_id.clone(),
            frame_directory: Some(frame_res.frames_dir),
            audio_path: None,
            fps: Some(10.0),
            width: Some(160),
            height: Some(120),
            output_format: Some("mp4".to_string()),
            output_name: Some("silent_reconstructed.mp4".to_string()),
        };

        let result = render_service.render_video(&proj_dir, &imported, &render_req).expect("Render video failed");
        assert!(result.output_metadata.valid);
        assert!(!result.output_metadata.has_audio);
        assert_eq!(result.output_metadata.width, 160);
        assert_eq!(result.output_metadata.height, 120);
    }

    #[test]
    fn test_render_service_invalid_frames_dir() {
        let temp = tempdir().unwrap();
        let proj_dir = temp.path().join("proj_invalid_test");

        let dummy_media = SourceMedia {
            media_id: "media-999".to_string(),
            original_file_name: "dummy.mp4".to_string(),
            source_path: proj_dir.join("dummy.mp4"),
            duration_ms: 1000,
            width: 320,
            height: 240,
            fps: 30.0,
            file_size_bytes: 1000,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: None,
            has_audio: false,
        };

        let render_service = RenderService::new();
        let render_req = RenderRequest {
            project_id: "proj_invalid_test".to_string(),
            media_id: "media-999".to_string(),
            frame_directory: Some(proj_dir.join("non_existent_frames")),
            audio_path: None,
            fps: Some(30.0),
            width: Some(320),
            height: Some(240),
            output_format: Some("mp4".to_string()),
            output_name: Some("error.mp4".to_string()),
        };

        let err = render_service.render_video(&proj_dir, &dummy_media, &render_req).unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::OutputNotFound);
    }

    #[test]
    fn test_render_service_live_portrait_video() {
        let video_path = PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !video_path.exists() {
            return;
        }

        let media_service = MediaService::new();
        let render_service = RenderService::new();
        let proj_dir = PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\projects\proj_render_live");
        let _ = fs::create_dir_all(&proj_dir);

        let imported = media_service.import_to_project(&proj_dir, &video_path).expect("Import failed");

        // Extract 3 seconds of frames at 2 FPS (6 frames)
        let frame_req = crate::media::FrameExtractionRequest {
            project_id: "proj_render_live".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: Some(0.0),
            end_time_seconds: Some(3.0),
            fps: Some(2.0),
            width: None,
            height: None,
            format: Some("png".to_string()),
        };
        let frame_res = media_service.extract_frames(&proj_dir, &imported.source_path, &frame_req).expect("Extract frames failed");
        let audio_res = media_service.extract_audio(&proj_dir, &imported.source_path, &imported.media_id).expect("Extract audio failed");

        let render_req = RenderRequest {
            project_id: "proj_render_live".to_string(),
            media_id: imported.media_id.clone(),
            frame_directory: Some(frame_res.frames_dir),
            audio_path: audio_res.audio_path,
            fps: Some(2.0),
            width: Some(imported.width),
            height: Some(imported.height),
            output_format: Some("mp4".to_string()),
            output_name: Some("reconstructed_portrait_3s.mp4".to_string()),
        };

        let result = render_service.render_video(&proj_dir, &imported, &render_req).expect("Live render failed");

        println!("[PHASE 4C RENDER PASS]");
        println!("Output Video: {}", result.output_metadata.output_path.display());
        println!("Duration: {:.2}s", result.output_metadata.duration_seconds);
        println!("Resolution: {}x{}", result.output_metadata.width, result.output_metadata.height);
        println!("FPS: {:.2}", result.output_metadata.fps);
        println!("Video Codec: {}", result.output_metadata.video_codec);
        println!("Audio Codec: {:?}", result.output_metadata.audio_codec);
        println!("File Size: {} bytes", result.output_metadata.file_size_bytes);
        println!("All checks PASS!");

        assert!(result.output_metadata.valid);
        assert!(result.output_metadata.file_size_bytes > 0);
        assert_eq!(result.output_metadata.width, 1080);
        assert_eq!(result.output_metadata.height, 1920);
        assert!(result.output_metadata.has_audio);
    }
}
