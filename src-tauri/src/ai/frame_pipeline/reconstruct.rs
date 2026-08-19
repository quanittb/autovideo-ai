use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::ai::frame_pipeline::artifact::AiArtifactManager;
use crate::ai::frame_pipeline::config::AiJobConfig;
use crate::error::AppError;
use crate::events::parse_ffmpeg_progress_line;
use crate::media::{MediaMetadata, MediaService};

// =========================================================================
// RATIONAL FPS
// =========================================================================

/// Represents exact rational frame rates (e.g. 30000/1001 for 29.97 fps, 24000/1001 for 23.976 fps, 30/1, 24/1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RationalFps {
    pub num: u32,
    pub den: u32,
}

impl RationalFps {
    pub fn new(num: u32, den: u32) -> Self {
        let d = if den == 0 { 1 } else { den };
        Self { num, den: d }
    }

    pub fn from_f64(fps: f64) -> Self {
        if fps <= 0.0 {
            return Self::new(30, 1);
        }

        // Common broadcast & film NTSC/PAL rational standards
        let candidates = [
            (24000, 1001, 24000.0 / 1001.0),
            (24, 1, 24.0),
            (25, 1, 25.0),
            (30000, 1001, 30000.0 / 1001.0),
            (30, 1, 30.0),
            (50, 1, 50.0),
            (60000, 1001, 60000.0 / 1001.0),
            (60, 1, 60.0),
            (120, 1, 120.0),
        ];

        for (num, den, val) in candidates {
            if (fps - val).abs() < 0.015 {
                return Self::new(num, den);
            }
        }

        // Integer or direct fractional approximation
        if (fps - fps.round()).abs() < 0.001 {
            Self::new(fps.round() as u32, 1)
        } else {
            // General 1000 base
            Self::new((fps * 1000.0).round() as u32, 1000)
        }
    }

    pub fn from_str_ratio(s: &str) -> Option<Self> {
        let s = s.trim();
        if let Some((num, den)) = s.split_once('/') {
            let n = num.trim().parse::<u32>().ok()?;
            let d = den.trim().parse::<u32>().ok()?;
            if d > 0 {
                return Some(Self::new(n, d));
            }
        } else if let Ok(f) = s.parse::<f64>() {
            return Some(Self::from_f64(f));
        }
        None
    }

    pub fn as_f64(&self) -> f64 {
        if self.den == 0 {
            self.num as f64
        } else {
            self.num as f64 / self.den as f64
        }
    }

    pub fn to_ffmpeg_arg(&self) -> String {
        if self.den == 1 {
            format!("{}", self.num)
        } else {
            format!("{}/{}", self.num, self.den)
        }
    }
}

impl Default for RationalFps {
    fn default() -> Self {
        Self::new(30, 1)
    }
}

// =========================================================================
// CODEC & AUDIO PRESERVATION
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    H265,
    Av1,
    ProRes,
}

impl VideoCodec {
    pub fn ffmpeg_encoder(&self) -> &'static str {
        match self {
            Self::H264 => "libx264",
            Self::H265 => "libx265",
            Self::Av1 => "libsvtav1",
            Self::ProRes => "prores_ks",
        }
    }

    pub fn default_extension(&self) -> &'static str {
        match self {
            Self::H264 | Self::H265 | Self::Av1 => "mp4",
            Self::ProRes => "mov",
        }
    }
}

impl Default for VideoCodec {
    fn default() -> Self {
        Self::H264
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioPreservationMode {
    PreserveOriginal,
    TranscodeAac,
    None,
}

impl Default for AudioPreservationMode {
    fn default() -> Self {
        Self::PreserveOriginal
    }
}

// =========================================================================
// CONFIGURATION & MANIFEST
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoReconstructionConfig {
    pub source_video_path: PathBuf,
    pub frames_dir: PathBuf,
    pub output_path: PathBuf,
    pub frame_pattern: String,
    pub expected_frame_count: usize,
    pub width: u32,
    pub height: u32,
    pub fps: RationalFps,
    pub pixel_format: String,
    pub codec: VideoCodec,
    pub crf: u32,
    pub audio_source: Option<PathBuf>,
    pub audio_mode: AudioPreservationMode,
    pub overwrite: bool,
}

impl Default for VideoReconstructionConfig {
    fn default() -> Self {
        Self {
            source_video_path: PathBuf::new(),
            frames_dir: PathBuf::new(),
            output_path: PathBuf::new(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 0,
            width: 1920,
            height: 1080,
            fps: RationalFps::default(),
            pixel_format: "yuv420p".to_string(),
            codec: VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: AudioPreservationMode::PreserveOriginal,
            overwrite: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FrameManifestEntry {
    pub frame_index: usize,
    pub artifact_path: PathBuf,
    pub status: String,
    pub width: u32,
    pub height: u32,
    pub file_size_bytes: u64,
    pub config_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructionTelemetry {
    pub total_duration_ms: f64,
    pub validation_duration_ms: f64,
    pub encoding_duration_ms: f64,
    pub mux_duration_ms: f64,
    pub output_size_bytes: u64,
    pub frames_reconstructed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructionManifest {
    pub job_id: String,
    pub source_path: PathBuf,
    pub model_id: Option<String>,
    pub model_config_hash: Option<String>,
    pub frame_count: usize,
    pub fps_num: u32,
    pub fps_den: u32,
    pub fps_f64: f64,
    pub width: u32,
    pub height: u32,
    pub codec: VideoCodec,
    pub has_audio: bool,
    pub frames: Vec<FrameManifestEntry>,
    pub output_path: PathBuf,
    pub output_size_bytes: u64,
    pub telemetry: ReconstructionTelemetry,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructionResult {
    pub output_path: PathBuf,
    pub output_metadata: MediaMetadata,
    pub manifest: ReconstructionManifest,
    pub telemetry: ReconstructionTelemetry,
}

// =========================================================================
// VIDEO RECONSTRUCTOR ENGINE
// =========================================================================

pub struct VideoReconstructor;

impl VideoReconstructor {
    /// Validates the full frame sequence on disk in strict numeric order (0..N-1).
    /// Enforces contiguous sequence, valid PNG magic bytes, consistent dimensions, non-zero file sizes,
    /// and config hash consistency if AI artifacts are present.
    pub fn validate_frame_sequence(
        frames_dir: &Path,
        expected_count: usize,
        expected_width: Option<u32>,
        expected_height: Option<u32>,
        ai_artifact_mgr: Option<&AiArtifactManager>,
        expected_config_hash: Option<&str>,
    ) -> Result<Vec<FrameManifestEntry>, AppError> {
        if !frames_dir.exists() {
            return Err(AppError::frame_sequence_invalid(
                "Frames directory does not exist",
                frames_dir.display().to_string(),
            ));
        }

        if expected_count == 0 {
            return Err(AppError::frame_sequence_invalid(
                "Expected frame count cannot be zero",
                "0 frames",
            ));
        }

        let mut manifest_entries = Vec::with_capacity(expected_count);

        for idx in 0..expected_count {
            let frame_file_name = format!("{:06}.png", idx);
            let frame_path = frames_dir.join(&frame_file_name);

            // 1. File existence
            if !frame_path.exists() {
                return Err(AppError::frame_sequence_invalid(
                    format!("Missing frame at contiguous index {}", idx),
                    frame_path.display().to_string(),
                ));
            }

            // 2. File size > 0
            let meta = fs::metadata(&frame_path).map_err(|e| {
                AppError::frame_sequence_invalid(
                    format!("Failed to read metadata for frame {}", idx),
                    e.to_string(),
                )
            })?;

            if meta.len() == 0 {
                return Err(AppError::frame_sequence_invalid(
                    format!("Encountered 0-byte corrupt frame at index {}", idx),
                    frame_path.display().to_string(),
                ));
            }

            // 3. PNG Magic Bytes validation
            let mut file = File::open(&frame_path).map_err(|e| {
                AppError::frame_sequence_invalid(
                    format!("Failed to open frame at index {}", idx),
                    e.to_string(),
                )
            })?;
            let mut header = [0u8; 8];
            use std::io::Read;
            let read_bytes = file.read(&mut header).unwrap_or(0);
            if read_bytes < 8 || header != [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
                return Err(AppError::frame_sequence_invalid(
                    format!("Corrupted or invalid PNG header at frame index {}", idx),
                    frame_path.display().to_string(),
                ));
            }

            // 4. Dimension inspection
            let (w, h) = image::image_dimensions(&frame_path).map_err(|e| {
                AppError::frame_sequence_invalid(
                    format!("Failed to decode image dimensions for frame {}", idx),
                    e.to_string(),
                )
            })?;

            if let Some(exp_w) = expected_width {
                if w != exp_w {
                    return Err(AppError::frame_sequence_invalid(
                        format!(
                            "Frame {} width mismatch: expected {}, got {}",
                            idx, exp_w, w
                        ),
                        frame_path.display().to_string(),
                    ));
                }
            }

            if let Some(exp_h) = expected_height {
                if h != exp_h {
                    return Err(AppError::frame_sequence_invalid(
                        format!(
                            "Frame {} height mismatch: expected {}, got {}",
                            idx, exp_h, h
                        ),
                        frame_path.display().to_string(),
                    ));
                }
            }

            // 5. Check AI metadata and config hash if present
            let mut status = "passthrough".to_string();
            let mut config_hash = None;

            if let Some(mgr) = ai_artifact_mgr {
                if let Ok(Some(ai_meta)) = mgr.load_frame_metadata(idx) {
                    status = format!("{:?}", ai_meta.status).to_lowercase();
                    config_hash = Some(ai_meta.config_hash.clone());

                    if let Some(exp_hash) = expected_config_hash {
                        if ai_meta.config_hash != exp_hash {
                            return Err(AppError::media_invalid(
                                format!(
                                    "Config hash mismatch on frame {}: expected {}, got {}",
                                    idx, exp_hash, ai_meta.config_hash
                                ),
                                frame_path.display().to_string(),
                            ));
                        }
                    }
                }
            }

            manifest_entries.push(FrameManifestEntry {
                frame_index: idx,
                artifact_path: frame_path,
                status,
                width: w,
                height: h,
                file_size_bytes: meta.len(),
                config_hash,
            });
        }

        Ok(manifest_entries)
    }

    /// Executes real FFmpeg video reconstruction with exact rational FPS, original audio muxing,
    /// atomic output generation, cancellation safety, and real-time progress callbacks.
    pub fn reconstruct_video<F, S, E>(
        config: &VideoReconstructionConfig,
        job_id: &str,
        ai_config: Option<&AiJobConfig>,
        ai_artifact_mgr: Option<&AiArtifactManager>,
        mut on_progress: F,
        cancel_token: Option<Arc<AtomicBool>>,
        mut on_spawn_pid: Option<S>,
        mut on_exit_pid: Option<E>,
    ) -> Result<ReconstructionResult, AppError>
    where
        F: FnMut(f32, usize, usize),
        S: FnMut(u32),
        E: FnMut(u32),
    {
        let t_total_start = Instant::now();

        // 1. Validate frames sequence strictly
        let t_val_start = Instant::now();
        let expected_hash = ai_config.map(|c| {
            crate::ai::compute_ai_config_hash(
                &c.model_id,
                &c.preprocessing,
                c.postprocessing.as_ref(),
            )
        });
        let manifest_entries = Self::validate_frame_sequence(
            &config.frames_dir,
            config.expected_frame_count,
            Some(config.width),
            Some(config.height),
            ai_artifact_mgr,
            expected_hash.as_deref(),
        )?;
        let val_duration_ms = t_val_start.elapsed().as_secs_f64() * 1000.0;

        // 2. Prepare output parent directory
        if let Some(parent) = config.output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::storage_error("Failed to create output directory", e.to_string())
            })?;
        }

        // 3. Setup atomic temporary file
        let output_filename = config
            .output_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output.mp4");
        let temp_output_filename = format!(".tmp-{}-{}", Uuid::new_v4(), output_filename);
        let temp_output_path = config
            .output_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&temp_output_filename);

        // 4. Resolve audio stream & duration
        let has_usable_audio = match config.audio_mode {
            AudioPreservationMode::None => false,
            AudioPreservationMode::PreserveOriginal | AudioPreservationMode::TranscodeAac => config
                .audio_source
                .as_ref()
                .map(|p| p.exists())
                .unwrap_or(false),
        };

        let target_duration_seconds =
            config.expected_frame_count as f64 / config.fps.as_f64().max(0.001);
        let input_pattern = config.frames_dir.join(&config.frame_pattern);

        // 5. Construct FFmpeg command
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y"); // overwrite temp if exists
        cmd.arg("-progress").arg("pipe:1");
        cmd.arg("-framerate").arg(config.fps.to_ffmpeg_arg());
        cmd.arg("-start_number").arg("0");
        cmd.arg("-i").arg(input_pattern.to_str().ok_or_else(|| {
            AppError::media_invalid(
                "Invalid input pattern path",
                input_pattern.display().to_string(),
            )
        })?);

        if has_usable_audio {
            if let Some(audio_src) = &config.audio_source {
                cmd.arg("-i").arg(audio_src.to_str().ok_or_else(|| {
                    AppError::media_invalid(
                        "Invalid audio source path",
                        audio_src.display().to_string(),
                    )
                })?);
            }
        }

        cmd.arg("-c:v").arg(config.codec.ffmpeg_encoder());
        cmd.arg("-pix_fmt").arg(&config.pixel_format);
        cmd.arg("-crf").arg(format!("{}", config.crf));

        if has_usable_audio {
            cmd.arg("-c:a").arg("aac");
            cmd.arg("-b:a").arg("192k");
            // Match audio duration precisely with frame duration to prevent drift
            cmd.arg("-t").arg(format!("{:.4}", target_duration_seconds));
        } else {
            cmd.arg("-an"); // explicitly disable audio if no audio stream exists
        }

        cmd.arg(temp_output_path.to_str().ok_or_else(|| {
            AppError::media_invalid(
                "Invalid temp output path",
                temp_output_path.display().to_string(),
            )
        })?);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let t_encode_start = Instant::now();
        let mut child = cmd.spawn().map_err(|e| {
            AppError::render_failed("Failed to invoke ffmpeg encoding process", e.to_string())
        })?;

        let pid = child.id();
        if let Some(ref mut cb) = on_spawn_pid {
            cb(pid);
        }

        let total_frames_usize = config.expected_frame_count;
        let total_frames_f64 = total_frames_usize as f64;
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
                        if let Ok(frame_num) = v.parse::<usize>() {
                            let percent = ((frame_num as f64 / total_frames_f64) * 100.0)
                                .clamp(0.0, 99.0) as f32;
                            on_progress(percent, frame_num, total_frames_usize);
                        }
                    } else if k == "progress" && v == "end" {
                        on_progress(100.0, total_frames_usize, total_frames_usize);
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
            if temp_output_path.exists() {
                let _ = fs::remove_file(&temp_output_path);
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
                if temp_output_path.exists() {
                    let _ = fs::remove_file(&temp_output_path);
                }
                return Err(AppError::cancelled());
            }
            let stderr_msg = String::from_utf8_lossy(&output.stderr);
            if temp_output_path.exists() {
                let _ = fs::remove_file(&temp_output_path);
            }
            return Err(AppError::render_failed(
                "FFmpeg video reconstruction failed with non-zero exit code",
                stderr_msg.to_string(),
            ));
        }

        let encode_duration_ms = t_encode_start.elapsed().as_secs_f64() * 1000.0;
        on_progress(100.0, total_frames_usize, total_frames_usize);

        // 6. Deep validate temporary video with FFprobe prior to atomic commit
        let media_service = MediaService::new();
        let output_probe = media_service.probe(&temp_output_path).map_err(|e| {
            if temp_output_path.exists() {
                let _ = fs::remove_file(&temp_output_path);
            }
            AppError::output_metadata_failed(
                "FFprobe validation failed on reconstructed temporary MP4",
                e.message,
            )
        })?;

        // 7. Validate output file size > 0
        if output_probe.file_size_bytes == 0 {
            if temp_output_path.exists() {
                let _ = fs::remove_file(&temp_output_path);
            }
            return Err(AppError::output_invalid(
                "Reconstructed MP4 is empty (0 bytes)",
                temp_output_path.display().to_string(),
            ));
        }

        // 8. Atomic Rename to final output destination
        #[cfg(target_os = "windows")]
        {
            if config.output_path.exists() {
                let _ = fs::remove_file(&config.output_path);
            }
        }
        fs::rename(&temp_output_path, &config.output_path).map_err(|e| {
            let _ = fs::remove_file(&temp_output_path);
            AppError::storage_error("Failed to commit final reconstructed video", e.to_string())
        })?;

        let total_duration_ms = t_total_start.elapsed().as_secs_f64() * 1000.0;

        let telemetry = ReconstructionTelemetry {
            total_duration_ms,
            validation_duration_ms: val_duration_ms,
            encoding_duration_ms: encode_duration_ms,
            mux_duration_ms: 0.0,
            output_size_bytes: output_probe.file_size_bytes,
            frames_reconstructed: total_frames_usize,
        };

        // 9. Write authoritative ReconstructionManifest alongside output
        let manifest = ReconstructionManifest {
            job_id: job_id.to_string(),
            source_path: config.source_video_path.clone(),
            model_id: ai_config.map(|c| c.model_id.clone()),
            model_config_hash: expected_hash,
            frame_count: total_frames_usize,
            fps_num: config.fps.num,
            fps_den: config.fps.den,
            fps_f64: config.fps.as_f64(),
            width: config.width,
            height: config.height,
            codec: config.codec,
            has_audio: has_usable_audio,
            frames: manifest_entries,
            output_path: config.output_path.clone(),
            output_size_bytes: output_probe.file_size_bytes,
            telemetry: telemetry.clone(),
            created_at: Utc::now().to_rfc3339(),
        };

        let manifest_path = config
            .output_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("reconstruction_manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| {
            AppError::storage_error("Failed to serialize reconstruction manifest", e.to_string())
        })?;
        let _ = fs::write(&manifest_path, manifest_json);

        Ok(ReconstructionResult {
            output_path: config.output_path.clone(),
            output_metadata: output_probe,
            manifest,
            telemetry,
        })
    }

    /// Performs deep validation on the final reconstructed media file against source specifications.
    pub fn validate_reconstructed_video(
        output_path: &Path,
        expected_width: u32,
        expected_height: u32,
        expected_fps: RationalFps,
        _expected_frame_count: usize,
        expected_has_audio: bool,
    ) -> Result<MediaMetadata, AppError> {
        if !output_path.exists() {
            return Err(AppError::output_not_found(
                output_path.display().to_string(),
            ));
        }

        let media_service = MediaService::new();
        let meta = media_service.probe(output_path)?;

        if meta.file_size_bytes == 0 {
            return Err(AppError::output_invalid(
                "Final reconstructed video is 0 bytes",
                output_path.display().to_string(),
            ));
        }

        if meta.video_codec.is_empty() {
            return Err(AppError::output_invalid(
                "Output video is missing a valid video codec",
                output_path.display().to_string(),
            ));
        }

        if meta.width != expected_width {
            return Err(AppError::output_invalid(
                format!(
                    "Output width mismatch: expected {}, got {}",
                    expected_width, meta.width
                ),
                output_path.display().to_string(),
            ));
        }

        if meta.height != expected_height {
            return Err(AppError::output_invalid(
                format!(
                    "Output height mismatch: expected {}, got {}",
                    expected_height, meta.height
                ),
                output_path.display().to_string(),
            ));
        }

        // Rational FPS check
        let exp_fps_f64 = expected_fps.as_f64();
        let fps_delta = (meta.fps - exp_fps_f64).abs();
        if fps_delta > 0.05 {
            return Err(AppError::output_invalid(
                format!(
                    "Output FPS mismatch: expected ~{:.3} ({}), got {:.3}",
                    exp_fps_f64,
                    expected_fps.to_ffmpeg_arg(),
                    meta.fps
                ),
                output_path.display().to_string(),
            ));
        }

        // Audio match check
        if expected_has_audio && !meta.has_audio {
            return Err(AppError::output_invalid(
                "Expected audio stream in output, but probed media has no audio",
                output_path.display().to_string(),
            ));
        }

        Ok(meta)
    }
}

// =========================================================================
// UNIT TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_phase6e_01_rational_fps_parsing_and_formatting() {
        let fps1 = RationalFps::from_str_ratio("30000/1001").unwrap();
        assert_eq!(fps1.num, 30000);
        assert_eq!(fps1.den, 1001);
        assert!((fps1.as_f64() - 29.97002997).abs() < 0.0001);
        assert_eq!(fps1.to_ffmpeg_arg(), "30000/1001");

        let fps2 = RationalFps::from_str_ratio("24/1").unwrap();
        assert_eq!(fps2.num, 24);
        assert_eq!(fps2.den, 1);
        assert_eq!(fps2.to_ffmpeg_arg(), "24");

        let fps3 = RationalFps::from_f64(29.97);
        assert_eq!(fps3.num, 30000);
        assert_eq!(fps3.den, 1001);

        let fps4 = RationalFps::from_f64(23.976);
        assert_eq!(fps4.num, 24000);
        assert_eq!(fps4.den, 1001);

        let fps5 = RationalFps::from_f64(60.0);
        assert_eq!(fps5.num, 60);
        assert_eq!(fps5.den, 1);
    }

    #[test]
    fn test_phase6e_02_frame_sequence_validation_valid() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..5 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = image::RgbImage::new(64, 64);
            img.save(&p).unwrap();
        }

        let res = VideoReconstructor::validate_frame_sequence(
            &frames_dir,
            5,
            Some(64),
            Some(64),
            None,
            None,
        );
        assert!(res.is_ok());
        let entries = res.unwrap();
        assert_eq!(entries.len(), 5);
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry.frame_index, i);
            assert_eq!(entry.width, 64);
            assert_eq!(entry.height, 64);
            assert!(entry.file_size_bytes > 0);
        }
    }

    #[test]
    fn test_phase6e_03_frame_sequence_validation_missing_frame() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        fs::create_dir_all(&frames_dir).unwrap();

        // Write frames 0, 1, 3, 4 (missing 2)
        for i in [0, 1, 3, 4] {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = image::RgbImage::new(64, 64);
            img.save(&p).unwrap();
        }

        let res = VideoReconstructor::validate_frame_sequence(
            &frames_dir,
            5,
            Some(64),
            Some(64),
            None,
            None,
        );
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.message.contains("Missing frame at contiguous index 2"));
    }

    #[test]
    fn test_phase6e_04_frame_sequence_validation_zero_byte_frame() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..3 {
            let p = frames_dir.join(format!("{:06}.png", i));
            if i == 1 {
                File::create(&p).unwrap(); // empty 0 bytes
            } else {
                let img = image::RgbImage::new(64, 64);
                img.save(&p).unwrap();
            }
        }

        let res = VideoReconstructor::validate_frame_sequence(
            &frames_dir,
            3,
            Some(64),
            Some(64),
            None,
            None,
        );
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.message.contains("0-byte corrupt frame at index 1"));
    }

    #[test]
    fn test_phase6e_05_frame_sequence_validation_dimension_mismatch() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..3 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = if i == 2 {
                image::RgbImage::new(32, 64) // mismatch width
            } else {
                image::RgbImage::new(64, 64)
            };
            img.save(&p).unwrap();
        }

        let res = VideoReconstructor::validate_frame_sequence(
            &frames_dir,
            3,
            Some(64),
            Some(64),
            None,
            None,
        );
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.message.contains("width mismatch"));
    }

    #[test]
    fn test_phase6e_06_frame_sequence_validation_config_hash_mismatch() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        let ai_cache_dir = temp.path().join("ai_cache");
        fs::create_dir_all(&frames_dir).unwrap();

        let mgr = AiArtifactManager::new(&ai_cache_dir);
        let png_path = frames_dir.join("000000.png");
        let img = image::RgbImage::new(64, 64);
        img.save(&png_path).unwrap();
        let png_bytes = fs::read(&png_path).unwrap();

        let meta = crate::ai::AiFrameMetadata {
            frame_index: 0,
            status: crate::ai::AiFrameStatus::Completed,
            model_id: "test-model".to_string(),
            provider: "cpu".to_string(),
            decode_duration_ms: 1.0,
            preprocess_duration_ms: 1.0,
            inference_duration_ms: 2.0,
            postprocess_duration_ms: 1.0,
            total_duration_ms: 5.0,
            input_width: 64,
            input_height: 64,
            output_width: 64,
            output_height: 64,
            output_artifact_path: "output.png".to_string(),
            config_hash: "hash_old_123".to_string(),
            ..Default::default()
        };

        mgr.write_frame_artifact(&meta, &png_bytes).unwrap();

        let res = VideoReconstructor::validate_frame_sequence(
            &mgr.reconstruction_frames_dir(),
            1,
            Some(64),
            Some(64),
            Some(&mgr),
            Some("hash_new_456"),
        );

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.message.contains("Config hash mismatch"));
    }

    #[test]
    fn test_phase6e_07_real_ffmpeg_reconstruction_and_atomic_output() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        fs::create_dir_all(&frames_dir).unwrap();

        // 6 frames at 30 fps = 0.2s video
        for i in 0..6 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let mut img = image::RgbImage::new(64, 64);
            // Color variation per frame
            for pixel in img.pixels_mut() {
                *pixel = image::Rgb([((i * 40) % 255) as u8, 120, 200]);
            }
            img.save(&p).unwrap();
        }

        let output_mp4 = temp.path().join("final_reconstructed.mp4");

        let config = VideoReconstructionConfig {
            source_video_path: temp.path().join("source.mp4"),
            frames_dir: frames_dir.clone(),
            output_path: output_mp4.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 6,
            width: 64,
            height: 64,
            fps: RationalFps::new(30, 1),
            pixel_format: "yuv420p".to_string(),
            codec: VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: AudioPreservationMode::None,
            overwrite: true,
        };

        let mut progress_calls = 0;
        let res = VideoReconstructor::reconstruct_video(
            &config,
            "test-job-6e",
            None,
            None,
            |_prog, _cur, _tot| {
                progress_calls += 1;
            },
            None,
            None::<fn(u32)>,
            None::<fn(u32)>,
        );

        assert!(res.is_ok());
        let recon_res = res.unwrap();
        assert!(output_mp4.exists());
        assert!(recon_res.output_metadata.file_size_bytes > 0);
        assert_eq!(recon_res.output_metadata.width, 64);
        assert_eq!(recon_res.output_metadata.height, 64);
        assert!(!recon_res.output_metadata.has_audio);
        assert!(progress_calls > 0);

        // Verify deep validation passes
        let deep_val = VideoReconstructor::validate_reconstructed_video(
            &output_mp4,
            64,
            64,
            RationalFps::new(30, 1),
            6,
            false,
        );
        assert!(deep_val.is_ok());
    }

    #[test]
    fn test_phase6e_08_reconstruction_cancellation() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..10 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = image::RgbImage::new(64, 64);
            img.save(&p).unwrap();
        }

        let output_mp4 = temp.path().join("cancelled.mp4");
        let cancel_token = Arc::new(AtomicBool::new(true)); // Pre-cancelled

        let config = VideoReconstructionConfig {
            source_video_path: temp.path().join("source.mp4"),
            frames_dir: frames_dir.clone(),
            output_path: output_mp4.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 10,
            width: 64,
            height: 64,
            fps: RationalFps::new(30, 1),
            pixel_format: "yuv420p".to_string(),
            codec: VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: AudioPreservationMode::None,
            overwrite: true,
        };

        let res = VideoReconstructor::reconstruct_video(
            &config,
            "test-cancel-job",
            None,
            None,
            |_p, _c, _t| {},
            Some(cancel_token),
            None::<fn(u32)>,
            None::<fn(u32)>,
        );

        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, crate::error::ErrorCode::Cancelled);
        assert!(!output_mp4.exists());
    }
}
