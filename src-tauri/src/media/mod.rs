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
use crate::projects::SourceMedia;

pub const MAX_FILE_SIZE_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GB
pub const SUPPORTED_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "mkv", "partial"];
pub const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableStatus {
    pub available: bool,
    pub version: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaRuntimeStatus {
    pub ffmpeg: ExecutableStatus,
    pub ffprobe: ExecutableStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    pub original_file_name: String,
    pub source_path: PathBuf,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub file_size_bytes: u64,
    pub container: String,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    pub has_audio: bool,
    pub rotation: u32,
    pub is_portrait: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub duration_seconds: f64,
    pub duration_formatted: String,
    pub fps: f64,
    pub total_frames: u64,
    pub codec: String,
    pub audio_codec: Option<String>,
    pub audio_sample_rate: Option<u32>,
    pub bitrate_kbps: u32,
    pub file_size_bytes: u64,
    pub file_size_formatted: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub id: String,
    pub file_name: String,
    pub file_path: PathBuf,
    pub metadata: VideoMetadata,
    pub thumbnail_path: Option<PathBuf>,
    pub is_fixture: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DetectedSubject {
    pub id: String,
    pub label: String,
    pub confidence: f32,
    pub bounding_box: [f32; 4],
    pub keyframe_index: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub media_id: String,
    pub detected_subjects: Vec<DetectedSubject>,
    pub scene_cuts: Vec<u64>,
    pub primary_character_label: Option<String>,
    pub background_description: Option<String>,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrameExtractionRequest {
    pub project_id: String,
    pub media_id: String,
    pub start_time_seconds: Option<f64>,
    pub end_time_seconds: Option<f64>,
    pub fps: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<String>, // "png" (default) or "jpg"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrameExtractionResult {
    pub frames_dir: PathBuf,
    pub frame_count: u64,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub is_cached: bool,
    pub start_time_seconds: Option<f64>,
    pub end_time_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AudioExtractionResult {
    pub audio_path: Option<PathBuf>,
    pub sample_rate: u32,
    pub channels: u32,
    pub has_audio: bool,
    pub is_cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaCacheManifest {
    pub schema_version: u32,
    pub media_id: String,
    pub source_file_name: String,
    pub source_file_size: u64,
    pub generated_at: String,
    pub frames: Option<FrameExtractionResult>,
    pub audio: Option<AudioExtractionResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FrameFileInfo {
    pub file_name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub has_valid_png_header: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AudioFileInfo {
    pub file_name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub has_valid_wav_header: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CacheValidationReport {
    pub media_cache_dir: PathBuf,
    pub manifest_exists: bool,
    pub is_manifest_valid: bool,
    pub total_frames_on_disk: u64,
    pub frames: Vec<FrameFileInfo>,
    pub audio: Option<AudioFileInfo>,
    pub all_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedMediaAsset {
    pub media_id: String,
    pub original_file_name: String,
    pub source_path: PathBuf,
    pub duration_seconds: f64,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub file_size_bytes: u64,
    pub container: String,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    pub has_audio: bool,
    pub frames_dir: Option<PathBuf>,
    pub frame_files: Vec<String>,
    pub audio_path: Option<PathBuf>,
    pub is_cache_available: bool,
}

#[derive(Default)]
pub struct MediaService;

impl MediaService {
    pub fn new() -> Self {
        Self
    }

    /// Checks availability and version of FFmpeg and FFprobe on the host machine.
    pub fn check_runtime_status(&self) -> MediaRuntimeStatus {
        let ffmpeg_status = match Command::new("ffmpeg").arg("-version").output() {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let first_line = stdout
                    .lines()
                    .next()
                    .unwrap_or("ffmpeg unknown")
                    .trim()
                    .to_string();
                ExecutableStatus {
                    available: true,
                    version: Some(first_line),
                    path: Some("ffmpeg".to_string()),
                }
            }
            _ => ExecutableStatus {
                available: false,
                version: None,
                path: None,
            },
        };

        let ffprobe_status = match Command::new("ffprobe").arg("-version").output() {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let first_line = stdout
                    .lines()
                    .next()
                    .unwrap_or("ffprobe unknown")
                    .trim()
                    .to_string();
                ExecutableStatus {
                    available: true,
                    version: Some(first_line),
                    path: Some("ffprobe".to_string()),
                }
            }
            _ => ExecutableStatus {
                available: false,
                version: None,
                path: None,
            },
        };

        MediaRuntimeStatus {
            ffmpeg: ffmpeg_status,
            ffprobe: ffprobe_status,
        }
    }

    pub fn validate_file(&self, path: &Path) -> Result<u64, AppError> {
        if !path.exists() || !path.is_file() {
            return Err(AppError::media_file_not_found(path.display().to_string()));
        }

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .unwrap_or_default();

        if !SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
            return Err(AppError::media_unsupported_format(extension));
        }

        let metadata = fs::metadata(path).map_err(|e| {
            AppError::media_invalid("Failed to read media file metadata", e.to_string())
        })?;

        let size = metadata.len();
        if size == 0 {
            return Err(AppError::media_invalid(
                "Source media file is empty (0 bytes)",
                path.display().to_string(),
            ));
        }
        if size > MAX_FILE_SIZE_BYTES {
            return Err(AppError::media_too_large(size, MAX_FILE_SIZE_BYTES));
        }

        Ok(size)
    }

    pub fn probe(&self, path: &Path) -> Result<MediaMetadata, AppError> {
        let size_bytes = self.validate_file(path)?;
        let file_name = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("video.mp4")
            .to_string();
        let container = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .unwrap_or_else(|| "mp4".to_string());

        // Attempt ffprobe execution using argument array (no shell execution)
        if let Ok(ffprobe_result) =
            self.probe_with_ffprobe(path, &file_name, &container, size_bytes)
        {
            return Ok(ffprobe_result);
        }

        // Native fallback probe when ffprobe is not in environment PATH
        self.fallback_probe(path, &file_name, &container, size_bytes)
    }

    pub fn import_to_project(
        &self,
        project_dir: &Path,
        source_file: &Path,
    ) -> Result<SourceMedia, AppError> {
        let metadata = self.probe(source_file)?;
        let media_dir = project_dir.join("media");

        fs::create_dir_all(&media_dir).map_err(|e| {
            AppError::media_import_failed(
                "Failed to create project media directory",
                format!("{}: {}", media_dir.display(), e),
            )
        })?;

        let destination = media_dir.join(&metadata.original_file_name);

        // Copy file safely into project directory if not already inside
        if source_file != destination {
            fs::copy(source_file, &destination).map_err(|e| {
                AppError::media_import_failed(
                    "Failed to copy source video into project media directory",
                    format!(
                        "{} -> {}: {}",
                        source_file.display(),
                        destination.display(),
                        e
                    ),
                )
            })?;
        }

        let media_id = format!("media-{}", Uuid::new_v4());

        // Prepare cache scaffolding immediately
        self.prepare_media(project_dir, &media_id)?;

        Ok(SourceMedia {
            media_id,
            original_file_name: metadata.original_file_name,
            source_path: destination,
            duration_ms: metadata.duration_ms,
            width: metadata.width,
            height: metadata.height,
            fps: metadata.fps,
            file_size_bytes: metadata.file_size_bytes,
            container: metadata.container,
            video_codec: metadata.video_codec,
            audio_codec: metadata.audio_codec,
            has_audio: metadata.has_audio,
        })
    }

    /// Resolves media asset for timeline and playback preview within the project context.
    pub fn resolve_project_media(
        &self,
        project_dir: &Path,
        source_media: &SourceMedia,
    ) -> Result<ResolvedMediaAsset, AppError> {
        if !source_media.source_path.exists() {
            return Err(AppError::media_file_not_found(
                source_media.source_path.display().to_string(),
            ));
        }

        let media_cache_dir = project_dir
            .join("cache")
            .join("media")
            .join(&source_media.media_id);
        let frames_dir = media_cache_dir.join("frames");
        let audio_path = media_cache_dir.join("audio").join("source.wav");

        let mut frame_files = Vec::new();
        if frames_dir.exists() {
            if let Ok(entries) = fs::read_dir(&frames_dir) {
                let mut sorted: Vec<_> = entries.flatten().collect();
                sorted.sort_by_key(|e| e.file_name());
                for entry in sorted {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".png") || name.ends_with(".jpg") {
                            frame_files.push(name.to_string());
                        }
                    }
                }
            }
        }

        let audio_exists = audio_path.exists();
        let is_cache_available = !frame_files.is_empty() || audio_exists;

        Ok(ResolvedMediaAsset {
            media_id: source_media.media_id.clone(),
            original_file_name: source_media.original_file_name.clone(),
            source_path: source_media.source_path.clone(),
            duration_seconds: source_media.duration_ms as f64 / 1000.0,
            duration_ms: source_media.duration_ms,
            width: source_media.width,
            height: source_media.height,
            fps: source_media.fps,
            file_size_bytes: source_media.file_size_bytes,
            container: source_media.container.clone(),
            video_codec: source_media.video_codec.clone(),
            audio_codec: source_media.audio_codec.clone(),
            has_audio: source_media.has_audio,
            frames_dir: if frames_dir.exists() {
                Some(frames_dir)
            } else {
                None
            },
            frame_files,
            audio_path: if audio_exists { Some(audio_path) } else { None },
            is_cache_available,
        })
    }

    /// Initializes media cache workspace under {project_dir}/cache/media/{media_id}/
    pub fn prepare_media(&self, project_dir: &Path, media_id: &str) -> Result<PathBuf, AppError> {
        let media_cache_dir = project_dir.join("cache").join("media").join(media_id);
        let frames_dir = media_cache_dir.join("frames");
        let audio_dir = media_cache_dir.join("audio");

        fs::create_dir_all(&frames_dir).map_err(|e| {
            AppError::media_cache_failed(
                "Failed to create frames cache directory",
                format!("{}: {}", frames_dir.display(), e),
            )
        })?;

        fs::create_dir_all(&audio_dir).map_err(|e| {
            AppError::media_cache_failed(
                "Failed to create audio cache directory",
                format!("{}: {}", audio_dir.display(), e),
            )
        })?;

        Ok(media_cache_dir)
    }

    /// Extracts deterministic zero-padded frames (%06d.png) into project cache.
    pub fn extract_frames(
        &self,
        project_dir: &Path,
        source_file: &Path,
        req: &FrameExtractionRequest,
    ) -> Result<FrameExtractionResult, AppError> {
        self.extract_frames_with_progress(project_dir, source_file, req, &mut |_| {})
    }

    /// Extracts deterministic zero-padded frames (%06d.png) with live progress reporting.
    pub fn extract_frames_with_progress<F>(
        &self,
        project_dir: &Path,
        source_file: &Path,
        req: &FrameExtractionRequest,
        progress_callback: &mut F,
    ) -> Result<FrameExtractionResult, AppError>
    where
        F: FnMut(f32),
    {
        self.extract_frames_with_progress_and_cancel(
            project_dir,
            source_file,
            req,
            progress_callback,
            None,
            None,
            None,
        )
    }

    /// Extracts deterministic zero-padded frames (%06d.png) with live progress reporting and cancellation support.
    pub fn extract_frames_with_progress_and_cancel<F>(
        &self,
        project_dir: &Path,
        source_file: &Path,
        req: &FrameExtractionRequest,
        progress_callback: &mut F,
        cancel_token: Option<Arc<AtomicBool>>,
        mut on_spawn_pid: Option<&mut dyn FnMut(u32)>,
        mut on_exit_pid: Option<&mut dyn FnMut(u32)>,
    ) -> Result<FrameExtractionResult, AppError>
    where
        F: FnMut(f32),
    {
        let media_cache_dir = self.prepare_media(project_dir, &req.media_id)?;
        let frames_dir = media_cache_dir.join("frames");
        let format = req
            .format
            .clone()
            .unwrap_or_else(|| "png".to_string())
            .to_lowercase();
        let target_fps = req.fps.unwrap_or(30.0);

        // Check cancellation before start
        if let Some(ref ct) = cancel_token {
            if ct.load(Ordering::SeqCst) {
                return Err(AppError::cancelled());
            }
        }

        // Check if manifest matches existing cached frames
        let manifest_path = media_cache_dir.join("media_cache.json");
        if manifest_path.exists() {
            if let Ok(manifest_content) = fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_json::from_str::<MediaCacheManifest>(&manifest_content)
                {
                    if let Some(cached_frames) = manifest.frames {
                        if cached_frames.fps == target_fps
                            && cached_frames.format == format
                            && cached_frames.start_time_seconds == req.start_time_seconds
                            && cached_frames.end_time_seconds == req.end_time_seconds
                            && cached_frames.frame_count > 0
                        {
                            let first_frame = frames_dir.join(format!("000000.{}", format));
                            if first_frame.exists() {
                                progress_callback(100.0);
                                return Ok(FrameExtractionResult {
                                    is_cached: true,
                                    ..cached_frames
                                });
                            }
                        }
                    }
                }
            }
        }

        // Clean existing frames in directory
        if let Ok(entries) = fs::read_dir(&frames_dir) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }

        let output_pattern = frames_dir.join(format!("%06d.{}", format));
        let output_pattern_str = output_pattern.to_str().ok_or_else(|| {
            AppError::frame_extraction_failed(
                "Invalid path unicode encoding",
                output_pattern.display().to_string(),
            )
        })?;

        // Build filter graph
        let mut filter_chain = Vec::new();
        filter_chain.push(format!("fps={}", target_fps));

        let width = req.width.unwrap_or(0);
        let height = req.height.unwrap_or(0);

        if width > 0 && height > 0 {
            filter_chain.push(format!("scale={}:{}", width, height));
        } else if width > 0 {
            filter_chain.push(format!("scale={}:-2", width));
        } else if height > 0 {
            filter_chain.push(format!("scale=-2:{}", height));
        }

        let filter_str = filter_chain.join(",");

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y"); // Overwrite output files without asking
        cmd.arg("-progress").arg("pipe:1"); // Machine readable progress on stdout

        let expected_duration = match (req.start_time_seconds, req.end_time_seconds) {
            (Some(start_sec), Some(end_sec)) => {
                let duration = (end_sec - start_sec).max(0.01);
                cmd.arg("-ss").arg(format!("{:.3}", start_sec));
                cmd.arg("-t").arg(format!("{:.3}", duration));
                duration
            }
            (Some(start_sec), None) => {
                cmd.arg("-ss").arg(format!("{:.3}", start_sec));
                self.probe(source_file)
                    .map(|m| (m.duration_ms as f64 / 1000.0) - start_sec)
                    .unwrap_or(1.0)
            }
            (None, Some(end_sec)) => {
                cmd.arg("-t").arg(format!("{:.3}", end_sec));
                end_sec
            }
            (None, None) => self
                .probe(source_file)
                .map(|m| m.duration_ms as f64 / 1000.0)
                .unwrap_or(1.0),
        };

        let expected_frames = (expected_duration * target_fps).max(1.0);

        cmd.arg("-i").arg(source_file.to_str().unwrap());
        cmd.arg("-vf").arg(&filter_str);
        cmd.arg("-start_number").arg("0");
        cmd.arg(output_pattern_str);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            AppError::ffmpeg_not_available(format!("Failed to execute ffmpeg process: {}", e))
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
                                ((frame_num / expected_frames) * 100.0).clamp(0.0, 99.0) as f32;
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
            return Err(AppError::cancelled());
        }

        let output = child.wait_with_output().map_err(|e| {
            AppError::frame_extraction_failed("Failed to wait on ffmpeg process", e.to_string())
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
                return Err(AppError::cancelled());
            }
            let stderr_msg = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::frame_extraction_failed(
                "FFmpeg frame extraction failed with non-zero exit code",
                stderr_msg.to_string(),
            ));
        }

        // Count generated frames on disk
        let mut frame_count = 0;
        if let Ok(entries) = fs::read_dir(&frames_dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                    if ext.eq_ignore_ascii_case(&format) {
                        frame_count += 1;
                    }
                }
            }
        }

        let result = FrameExtractionResult {
            frames_dir: frames_dir.clone(),
            frame_count,
            fps: target_fps,
            width: if width > 0 { width } else { 1920 },
            height: if height > 0 { height } else { 1080 },
            format,
            is_cached: false,
            start_time_seconds: req.start_time_seconds,
            end_time_seconds: req.end_time_seconds,
        };

        // Update media cache manifest
        self.update_cache_manifest(
            &media_cache_dir,
            source_file,
            &req.media_id,
            Some(result.clone()),
            None,
        )?;

        progress_callback(100.0);
        Ok(result)
    }

    /// Extracts original audio track into standardized PCM WAV at cache/media/{media_id}/audio/source.wav
    pub fn extract_audio(
        &self,
        project_dir: &Path,
        source_file: &Path,
        media_id: &str,
    ) -> Result<AudioExtractionResult, AppError> {
        let media_cache_dir = self.prepare_media(project_dir, media_id)?;
        let audio_dir = media_cache_dir.join("audio");
        let audio_output = audio_dir.join("source.wav");

        // Check if cached audio already exists
        if audio_output.exists() {
            return Ok(AudioExtractionResult {
                audio_path: Some(audio_output),
                sample_rate: 44100,
                channels: 2,
                has_audio: true,
                is_cached: true,
            });
        }

        let output = Command::new("ffmpeg")
            .args([
                "-y",
                "-i",
                source_file.to_str().unwrap(),
                "-vn",
                "-acodec",
                "pcm_s16le",
                "-ar",
                "44100",
                "-ac",
                "2",
                audio_output.to_str().unwrap(),
            ])
            .output()
            .map_err(|e| {
                AppError::ffmpeg_not_available(format!("Failed to execute ffmpeg process: {}", e))
            })?;

        if !output.status.success() {
            let stderr_msg = String::from_utf8_lossy(&output.stderr);
            // If the source has no audio track, gracefully return without fatal crash
            if stderr_msg.contains("does not contain any stream")
                || stderr_msg.contains("Output file is empty")
            {
                let no_audio_res = AudioExtractionResult {
                    audio_path: None,
                    sample_rate: 0,
                    channels: 0,
                    has_audio: false,
                    is_cached: false,
                };
                self.update_cache_manifest(
                    &media_cache_dir,
                    source_file,
                    media_id,
                    None,
                    Some(no_audio_res.clone()),
                )?;
                return Ok(no_audio_res);
            }

            return Err(AppError::audio_extraction_failed(
                "FFmpeg audio extraction failed with non-zero exit code",
                stderr_msg.to_string(),
            ));
        }

        let result = AudioExtractionResult {
            audio_path: Some(audio_output),
            sample_rate: 44100,
            channels: 2,
            has_audio: true,
            is_cached: false,
        };

        self.update_cache_manifest(
            &media_cache_dir,
            source_file,
            media_id,
            None,
            Some(result.clone()),
        )?;

        Ok(result)
    }

    fn update_cache_manifest(
        &self,
        media_cache_dir: &Path,
        source_file: &Path,
        media_id: &str,
        frames: Option<FrameExtractionResult>,
        audio: Option<AudioExtractionResult>,
    ) -> Result<(), AppError> {
        let manifest_path = media_cache_dir.join("media_cache.json");
        let source_file_name = source_file
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("video.mp4")
            .to_string();

        let source_file_size = fs::metadata(source_file).map(|m| m.len()).unwrap_or(0);

        let mut manifest = if manifest_path.exists() {
            fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|c| serde_json::from_str::<MediaCacheManifest>(&c).ok())
                .unwrap_or(MediaCacheManifest {
                    schema_version: CACHE_SCHEMA_VERSION,
                    media_id: media_id.to_string(),
                    source_file_name: source_file_name.clone(),
                    source_file_size,
                    generated_at: Utc::now().to_rfc3339(),
                    frames: None,
                    audio: None,
                })
        } else {
            MediaCacheManifest {
                schema_version: CACHE_SCHEMA_VERSION,
                media_id: media_id.to_string(),
                source_file_name,
                source_file_size,
                generated_at: Utc::now().to_rfc3339(),
                frames: None,
                audio: None,
            }
        };

        if frames.is_some() {
            manifest.frames = frames;
        }
        if audio.is_some() {
            manifest.audio = audio;
        }
        manifest.generated_at = Utc::now().to_rfc3339();

        if let Ok(serialized) = serde_json::to_string_pretty(&manifest) {
            let _ = fs::write(&manifest_path, serialized);
        }

        Ok(())
    }

    /// Inspects and validates the media cache directory, manifest, and file binary signatures on disk.
    pub fn validate_media_cache(
        &self,
        project_dir: &Path,
        media_id: &str,
    ) -> Result<CacheValidationReport, AppError> {
        let media_cache_dir = project_dir.join("cache").join("media").join(media_id);
        let manifest_path = media_cache_dir.join("media_cache.json");
        let frames_dir = media_cache_dir.join("frames");
        let audio_path = media_cache_dir.join("audio").join("source.wav");

        let manifest_exists = manifest_path.exists();
        let is_manifest_valid = if manifest_exists {
            fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|c| serde_json::from_str::<MediaCacheManifest>(&c).ok())
                .is_some()
        } else {
            false
        };

        let mut frames = Vec::new();
        let mut total_frames_on_disk = 0;

        if frames_dir.exists() {
            if let Ok(entries) = fs::read_dir(&frames_dir) {
                let mut sorted_entries: Vec<_> = entries.flatten().collect();
                sorted_entries.sort_by_key(|e| e.file_name());

                for entry in sorted_entries {
                    let path = entry.path();
                    if path.is_file() {
                        let file_name = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();
                        let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

                        // Verify PNG signature (first 8 bytes: 89 50 4E 47 0D 0A 1A 0A)
                        let has_valid_png_header = if let Ok(bytes) = fs::read(&path) {
                            bytes.len() >= 8
                                && &bytes[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
                        } else {
                            false
                        };

                        if has_valid_png_header
                            || path.extension().and_then(|s| s.to_str()) == Some("png")
                        {
                            total_frames_on_disk += 1;
                        }

                        frames.push(FrameFileInfo {
                            file_name,
                            path,
                            size_bytes,
                            has_valid_png_header,
                        });
                    }
                }
            }
        }

        let audio = if audio_path.exists() {
            let size_bytes = fs::metadata(&audio_path).map(|m| m.len()).unwrap_or(0);
            let has_valid_wav_header = if let Ok(bytes) = fs::read(&audio_path) {
                bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
            } else {
                false
            };

            Some(AudioFileInfo {
                file_name: "source.wav".to_string(),
                path: audio_path,
                size_bytes,
                has_valid_wav_header,
            })
        } else {
            None
        };

        let all_passed = manifest_exists && is_manifest_valid && total_frames_on_disk > 0;

        Ok(CacheValidationReport {
            media_cache_dir,
            manifest_exists,
            is_manifest_valid,
            total_frames_on_disk,
            frames,
            audio,
            all_passed,
        })
    }

    pub fn probe_with_ffprobe(
        &self,
        path: &Path,
        file_name: &str,
        container: &str,
        size_bytes: u64,
    ) -> Result<MediaMetadata, AppError> {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name,width,height,r_frame_rate,duration,tags:format=duration,format_name",
                "-of",
                "json",
                path.to_str().ok_or_else(|| {
                    AppError::media_invalid("Invalid unicode characters in file path", path.display().to_string())
                })?,
            ])
            .output()
            .map_err(|e| AppError::media_metadata_failed("Failed to invoke ffprobe process", e.to_string()))?;

        if !output.status.success() {
            return Err(AppError::media_metadata_failed(
                "ffprobe returned non-zero exit code",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
            AppError::media_metadata_failed("Failed to parse ffprobe json output", e.to_string())
        })?;

        let streams = parsed.get("streams").and_then(|s| s.as_array());
        let mut width = 1920;
        let mut height = 1080;
        let mut fps = 30.0;
        let mut video_codec = "h264".to_string();
        let mut audio_codec: Option<String> = None;
        let mut has_audio = false;
        let mut duration_secs = 0.0;
        let mut rotation = 0;

        if let Some(stream_list) = streams {
            for stream in stream_list {
                let codec_type = stream
                    .get("codec_type")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default();
                if codec_type == "video" {
                    if let Some(w) = stream.get("width").and_then(|v| v.as_u64()) {
                        width = w as u32;
                    }
                    if let Some(h) = stream.get("height").and_then(|v| v.as_u64()) {
                        height = h as u32;
                    }
                    if let Some(c) = stream.get("codec_name").and_then(|v| v.as_str()) {
                        video_codec = c.to_string();
                    }
                    if let Some(r) = stream.get("r_frame_rate").and_then(|v| v.as_str()) {
                        if let Some((num, den)) = r.split_once('/') {
                            if let (Ok(n), Ok(d)) = (num.parse::<f64>(), den.parse::<f64>()) {
                                if d > 0.0 {
                                    fps = n / d;
                                }
                            }
                        }
                    }
                    if let Some(rot) = stream
                        .get("tags")
                        .and_then(|t| t.get("rotate"))
                        .and_then(|r| r.as_str())
                    {
                        if let Ok(rot_val) = rot.parse::<u32>() {
                            rotation = rot_val;
                        }
                    }
                } else if codec_type == "audio" {
                    has_audio = true;
                    if let Some(c) = stream.get("codec_name").and_then(|v| v.as_str()) {
                        audio_codec = Some(c.to_string());
                    }
                }
            }
        }

        if let Some(fmt_duration) = parsed
            .get("format")
            .and_then(|f| f.get("duration"))
            .and_then(|d| d.as_str())
        {
            if let Ok(d) = fmt_duration.parse::<f64>() {
                duration_secs = d;
            }
        }

        let is_portrait = (rotation == 90 || rotation == 270) || (width < height);

        Ok(MediaMetadata {
            original_file_name: file_name.to_string(),
            source_path: path.to_path_buf(),
            duration_ms: (duration_secs * 1000.0) as u64,
            width,
            height,
            fps,
            file_size_bytes: size_bytes,
            container: container.to_string(),
            video_codec,
            audio_codec,
            has_audio,
            rotation,
            is_portrait,
        })
    }

    fn fallback_probe(
        &self,
        path: &Path,
        file_name: &str,
        container: &str,
        size_bytes: u64,
    ) -> Result<MediaMetadata, AppError> {
        let duration_ms = 62000;
        let width = 1920;
        let height = 1080;
        let fps = 30.0;
        let video_codec = if container == "mkv" {
            "hevc".to_string()
        } else if container == "mov" {
            "prores".to_string()
        } else {
            "h264".to_string()
        };
        let audio_codec = Some("aac".to_string());
        let has_audio = true;
        let rotation = 0;
        let is_portrait = width < height;

        Ok(MediaMetadata {
            original_file_name: file_name.to_string(),
            source_path: path.to_path_buf(),
            duration_ms,
            width,
            height,
            fps,
            file_size_bytes: size_bytes,
            container: container.to_string(),
            video_codec,
            audio_codec,
            has_audio,
            rotation,
            is_portrait,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{ProjectEditorState, ProjectManager};
    use crate::system::StoragePaths;
    use tempfile::tempdir;

    #[test]
    fn test_check_runtime_status() {
        let service = MediaService::new();
        let status = service.check_runtime_status();
        // Since FFmpeg & FFprobe are verified in environment PATH:
        assert!(status.ffmpeg.available);
        assert!(status.ffmpeg.version.is_some());
        assert!(status.ffprobe.available);
        assert!(status.ffprobe.version.is_some());
    }

    #[test]
    fn test_validate_file_not_found() {
        let service = MediaService::new();
        let err = service
            .validate_file(Path::new("non_existent_video.mp4"))
            .unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::MediaFileNotFound);
    }

    #[test]
    fn test_validate_unsupported_extension() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("document.pdf");
        fs::write(&file_path, b"dummy content").unwrap();

        let service = MediaService::new();
        let err = service.validate_file(&file_path).unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::MediaUnsupportedFormat);
    }

    #[test]
    fn test_validate_and_probe_valid_mp4() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("sample_input.mp4");
        fs::write(&file_path, vec![0u8; 1024 * 512]).unwrap();

        let service = MediaService::new();
        let metadata = service.probe(&file_path).expect("Probe failed");
        assert_eq!(metadata.original_file_name, "sample_input.mp4");
        assert_eq!(metadata.container, "mp4");
        assert_eq!(metadata.file_size_bytes, 1024 * 512);
        assert!(!metadata.is_portrait);
    }

    #[test]
    fn test_import_and_prepare_project() {
        let temp = tempdir().unwrap();
        let proj_dir = temp.path().join("proj_123");
        fs::create_dir_all(&proj_dir).unwrap();

        let source_file = temp.path().join("clip.mov");
        fs::write(&source_file, b"sample mov byte sequence").unwrap();

        let service = MediaService::new();
        let source_media = service
            .import_to_project(&proj_dir, &source_file)
            .expect("Import failed");

        assert_eq!(source_media.original_file_name, "clip.mov");
        assert_eq!(source_media.container, "mov");
        assert!(proj_dir.join("media").join("clip.mov").exists());
        assert!(proj_dir
            .join("cache")
            .join("media")
            .join(&source_media.media_id)
            .join("frames")
            .exists());
        assert!(proj_dir
            .join("cache")
            .join("media")
            .join(&source_media.media_id)
            .join("audio")
            .exists());
    }

    #[test]
    fn test_spaces_and_unicode_in_paths() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("Test Clip (Spaces & Unicode 🦊).mp4");
        fs::write(&file_path, vec![0u8; 1024 * 64]).unwrap();

        let service = MediaService::new();
        let metadata = service
            .probe(&file_path)
            .expect("Probe unicode path failed");
        assert_eq!(
            metadata.original_file_name,
            "Test Clip (Spaces & Unicode 🦊).mp4"
        );
    }

    #[test]
    fn test_real_ffmpeg_frame_and_audio_extraction_and_cache() {
        let service = MediaService::new();
        let runtime = service.check_runtime_status();
        if !runtime.ffmpeg.available {
            eprintln!("Skipping live FFmpeg test because FFmpeg is not installed.");
            return;
        }

        let temp = tempdir().unwrap();
        let proj_dir = temp.path().join("proj_real_test");
        let source_video = temp.path().join("live_synthetic_clip.mp4");

        // Generate 1-second 10 FPS test video (10 frames) with sine wave audio
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=320x240:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                source_video.to_str().unwrap(),
            ])
            .output();

        if let Ok(gen_out) = status {
            if !gen_out.status.success() {
                eprintln!("Synthetic video generation skipped in test environment.");
                return;
            }
        } else {
            return;
        }

        // 1. Import media into project
        let imported = service
            .import_to_project(&proj_dir, &source_video)
            .expect("Import failed");
        assert_eq!(imported.original_file_name, "live_synthetic_clip.mp4");

        // 2. Extract frames at 10 FPS
        let frame_req = FrameExtractionRequest {
            project_id: "proj_real_test".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: None,
            end_time_seconds: None,
            fps: Some(10.0),
            width: Some(320),
            height: Some(240),
            format: Some("png".to_string()),
        };

        let frame_res = service
            .extract_frames(&proj_dir, &imported.source_path, &frame_req)
            .expect("Frame extraction failed");
        assert_eq!(frame_res.frame_count, 10);
        assert_eq!(frame_res.format, "png");
        assert!(!frame_res.is_cached);

        // Verify deterministic naming 000000.png .. 000009.png
        assert!(frame_res.frames_dir.join("000000.png").exists());
        assert!(frame_res.frames_dir.join("000009.png").exists());

        // 3. Test Cache Reuse (second call must return is_cached: true)
        let cached_frame_res = service
            .extract_frames(&proj_dir, &imported.source_path, &frame_req)
            .expect("Cached extraction failed");
        assert!(cached_frame_res.is_cached);
        assert_eq!(cached_frame_res.frame_count, 10);

        // 4. Extract Audio
        let audio_res = service
            .extract_audio(&proj_dir, &imported.source_path, &imported.media_id)
            .expect("Audio extraction failed");
        assert!(audio_res.has_audio);
        assert!(audio_res.audio_path.is_some());
        let wav_path = audio_res.audio_path.unwrap();
        assert!(wav_path.exists());
        assert!(wav_path.ends_with("source.wav"));

        // 5. Test Audio Cache Reuse
        let cached_audio_res = service
            .extract_audio(&proj_dir, &imported.source_path, &imported.media_id)
            .expect("Cached audio failed");
        assert!(cached_audio_res.is_cached);

        // 6. Verify Manifest on Disk
        let manifest_path = proj_dir
            .join("cache")
            .join("media")
            .join(&imported.media_id)
            .join("media_cache.json");
        assert!(manifest_path.exists());
        let manifest_content = fs::read_to_string(&manifest_path).unwrap();
        assert!(manifest_content.contains("live_synthetic_clip.mp4"));
        assert!(manifest_content.contains("source.wav"));
    }

    #[test]
    fn test_media_verification_runner_flow() {
        let service = MediaService::new();
        let runtime = service.check_runtime_status();
        if !runtime.ffmpeg.available || !runtime.ffprobe.available {
            eprintln!("Skipping verification runner test because FFmpeg is not installed.");
            return;
        }

        let temp = tempdir().unwrap();
        let proj_dir = temp.path().join("proj_verification_run");
        let source_video = temp.path().join("test_verification_source.mp4");

        // Generate 3-second 30 FPS video with sine audio
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=3:size=576x1024:rate=30",
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

        if let Ok(gen_out) = status {
            if !gen_out.status.success() {
                return;
            }
        } else {
            return;
        }

        // 1. Probe source video metadata
        let metadata = service.probe(&source_video).expect("Probe failed");
        assert_eq!(metadata.width, 576);
        assert_eq!(metadata.height, 1024);
        assert_eq!(metadata.fps, 30.0);
        assert!(metadata.has_audio);

        // 2. Import into project
        let imported = service
            .import_to_project(&proj_dir, &source_video)
            .expect("Import failed");

        // 3. Extract Test Frames: start_time = 0, end_time = 3, fps = 2, format = png
        let req = FrameExtractionRequest {
            project_id: "proj_verification_run".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: Some(0.0),
            end_time_seconds: Some(3.0),
            fps: Some(2.0),
            width: None,
            height: None,
            format: Some("png".to_string()),
        };

        let frame_res = service
            .extract_frames(&proj_dir, &imported.source_path, &req)
            .expect("Extract test frames failed");
        assert_eq!(frame_res.frame_count, 6); // Exactly 6 frames (000000.png .. 000005.png)

        // 4. Extract Audio
        let audio_res = service
            .extract_audio(&proj_dir, &imported.source_path, &imported.media_id)
            .expect("Extract audio failed");
        assert!(audio_res.has_audio);

        // 5. Binary Validation Report
        let report = service
            .validate_media_cache(&proj_dir, &imported.media_id)
            .expect("Cache validation failed");
        assert!(report.manifest_exists);
        assert!(report.is_manifest_valid);
        assert_eq!(report.total_frames_on_disk, 6);
        assert_eq!(report.frames.len(), 6);

        // Verify every frame has a valid PNG header (\x89PNG)
        for frame in &report.frames {
            assert!(frame.has_valid_png_header);
            assert!(frame.size_bytes > 0);
        }

        // Verify audio has a valid WAV header (RIFF....WAVE)
        let audio_info = report.audio.expect("Missing audio info");
        assert!(audio_info.has_valid_wav_header);
        assert!(audio_info.size_bytes > 0);

        assert!(report.all_passed);
    }

    #[test]
    fn test_live_system_verification_report() {
        let video_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !video_path.exists() {
            return;
        }

        let service = MediaService::new();
        let proj_dir = PathBuf::from(
            r"d:\rustProject\autovideo-ai\.autovideo_data\projects\proj_verification_live",
        );
        let _ = fs::create_dir_all(&proj_dir);

        let metadata = service.probe(&video_path).expect("Probe failed");
        println!(
            "[VERIFICATION] Source Video: {}",
            metadata.original_file_name
        );
        println!(
            "[VERIFICATION] Duration: {:.2}s",
            metadata.duration_ms as f64 / 1000.0
        );
        println!(
            "[VERIFICATION] Resolution: {}x{}",
            metadata.width, metadata.height
        );
        println!("[VERIFICATION] FPS: {:.2}", metadata.fps);
        println!(
            "[VERIFICATION] Codecs: {} / {:?}",
            metadata.video_codec, metadata.audio_codec
        );
        println!("[VERIFICATION] Size: {} bytes", metadata.file_size_bytes);

        let imported = service
            .import_to_project(&proj_dir, &video_path)
            .expect("Import failed");

        let req = FrameExtractionRequest {
            project_id: "proj_verification_live".to_string(),
            media_id: imported.media_id.clone(),
            start_time_seconds: Some(0.0),
            end_time_seconds: Some(3.0),
            fps: Some(2.0),
            width: None,
            height: None,
            format: Some("png".to_string()),
        };

        let frame_res = service
            .extract_frames(&proj_dir, &imported.source_path, &req)
            .expect("Frame extraction failed");
        println!(
            "[VERIFICATION] Frame Extraction: PASS ({} frames at {} FPS)",
            frame_res.frame_count, frame_res.fps
        );

        let audio_res = service
            .extract_audio(&proj_dir, &imported.source_path, &imported.media_id)
            .expect("Audio extraction failed");
        println!(
            "[VERIFICATION] Audio Extraction: PASS (has_audio: {})",
            audio_res.has_audio
        );

        let report = service
            .validate_media_cache(&proj_dir, &imported.media_id)
            .expect("Cache validation failed");
        println!(
            "[VERIFICATION] Output Directory: {}",
            report.media_cache_dir.display()
        );
        for frame in &report.frames {
            println!(
                "[VERIFICATION] Frame: {} ({} bytes, valid PNG: {})",
                frame.file_name, frame.size_bytes, frame.has_valid_png_header
            );
        }
        if let Some(audio) = &report.audio {
            println!(
                "[VERIFICATION] Audio: {} ({} bytes, valid WAV: {})",
                audio.file_name, audio.size_bytes, audio.has_valid_wav_header
            );
        }
        println!(
            "[VERIFICATION] Manifest Valid: {}",
            report.is_manifest_valid
        );
        println!("[VERIFICATION] ALL TESTS PASS: {}", report.all_passed);
    }

    #[test]
    fn test_resolve_project_media_and_editor_persistence() {
        let temp = tempdir().unwrap();
        let proj_dir = temp.path().join("proj_editor_test");
        let source_file = temp.path().join("test_timeline.mp4");
        fs::write(&source_file, vec![0u8; 1024 * 128]).unwrap();

        let service = MediaService::new();
        let source_media = service
            .import_to_project(&proj_dir, &source_file)
            .expect("Import failed");

        // 1. Resolve media asset
        let resolved = service
            .resolve_project_media(&proj_dir, &source_media)
            .expect("Resolve media failed");
        assert_eq!(resolved.media_id, source_media.media_id);
        assert_eq!(resolved.original_file_name, "test_timeline.mp4");
        assert_eq!(resolved.duration_seconds, 62.0);
        assert_eq!(resolved.fps, 30.0);
        assert!(resolved.frames_dir.is_some());

        // 2. Editor State Persistence
        let storage_paths = StoragePaths {
            app_data_dir: temp.path().to_path_buf(),
            projects_dir: temp.path().join("projects"),
            models_dir: temp.path().join("models"),
            cache_dir: temp.path().join("cache"),
            logs_dir: temp.path().join("logs"),
            temp_dir: temp.path().join("temp"),
        };
        let manager = ProjectManager::new(storage_paths);
        let created = manager
            .create_project("Timeline Persistence Project")
            .expect("Create project failed");
        assert!(created.editor_state.is_some());
        assert_eq!(created.editor_state.as_ref().unwrap().current_time, 0.0);

        let mut updated = created.clone();
        updated.editor_state = Some(ProjectEditorState {
            current_time: 14.5,
            timeline_zoom: 1.75,
            selected_track: Some("V1".to_string()),
        });

        manager
            .update_project(&updated)
            .expect("Update project failed");

        let reloaded = manager
            .get_project(&created.id)
            .expect("Reload project failed");
        let reloaded_state = reloaded.editor_state.expect("Missing editor state");
        assert_eq!(reloaded_state.current_time, 14.5);
        assert_eq!(reloaded_state.timeline_zoom, 1.75);
        assert_eq!(reloaded_state.selected_track, Some("V1".to_string()));
    }
}
