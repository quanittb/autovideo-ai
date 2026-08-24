use super::manifest::FlowOutputArtifactRecord;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct FlowOutputValidator;

impl FlowOutputValidator {
    pub fn validate_child_artifact(
        file_path: &Path,
        expected_duration_sec: f64,
    ) -> Result<FlowOutputArtifactRecord, String> {
        if !file_path.exists() {
            return Err(format!(
                "VALIDATION_FAILED: Output file does not exist at {:?}",
                file_path
            ));
        }

        let bytes = fs::read(file_path)
            .map_err(|e| format!("VALIDATION_FAILED: Failed to read file bytes: {}", e))?;
        if bytes.is_empty() {
            return Err("VALIDATION_FAILED: Downloaded artifact is 0 bytes (empty)".to_string());
        }

        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha256 = format!("{:x}", hasher.finalize());

        // Probe via ffprobe
        let probe_output = Command::new("ffprobe")
            .arg("-v")
            .arg("error")
            .arg("-select_streams")
            .arg("v:0")
            .arg("-show_entries")
            .arg("stream=width,height,r_frame_rate,nb_frames,duration,codec_name:format=duration")
            .arg("-of")
            .arg("json")
            .arg(file_path)
            .output()
            .map_err(|e| format!("FFprobe execution failed: {}", e))?;

        if !probe_output.status.success() {
            let stderr = String::from_utf8_lossy(&probe_output.stderr);
            return Err(format!(
                "VALIDATION_FAILED: Output file is corrupt or unreadable: {}",
                stderr
            ));
        }

        let json_str = String::from_utf8_lossy(&probe_output.stdout);
        let val: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| format!("Failed to parse ffprobe json: {}", e))?;

        let stream = val
            .get("streams")
            .and_then(|s| s.get(0))
            .ok_or_else(|| "VALIDATION_FAILED: No video stream found in artifact".to_string())?;

        let width = stream.get("width").and_then(|w| w.as_u64()).unwrap_or(0) as u32;
        let height = stream.get("height").and_then(|h| h.as_u64()).unwrap_or(0) as u32;

        if width == 0 || height == 0 {
            return Err(format!(
                "VALIDATION_FAILED: Invalid dimensions {}x{}",
                width, height
            ));
        }

        let duration_sec = stream
            .get("duration")
            .and_then(|d| d.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        let r_fps_str = stream
            .get("r_frame_rate")
            .and_then(|r| r.as_str())
            .unwrap_or("30/1");
        let fps = parse_fraction(r_fps_str).unwrap_or(30.0);

        let frame_count = stream
            .get("nb_frames")
            .and_then(|n| n.as_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|| ((duration_sec * fps).round() as u64).max(1));

        let effective_video_duration = if duration_sec > 0.0 {
            duration_sec
        } else if frame_count > 0 && fps > 0.0 {
            (frame_count as f64) / fps
        } else {
            val.get("format")
                .and_then(|f| f.get("duration"))
                .and_then(|d| d.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0)
        };

        if effective_video_duration <= 0.0 {
            return Err("VALIDATION_FAILED: Video stream duration must be positive".to_string());
        }

        if frame_count == 0 {
            return Err("VALIDATION_FAILED: Video has 0 frames".to_string());
        }

        // Conservative duration drift tolerance: max(0.5s, 5% of expected duration)
        let tolerance = (0.5_f64).max(expected_duration_sec * 0.05);
        let drift = (effective_video_duration - expected_duration_sec).abs();
        if drift > tolerance {
            return Err(format!(
                "FLOW_OUTPUT_DURATION_MISMATCH: Duration drift too large (expected {:.3}s, got {:.3}s, drift {:.3}s > tolerance {:.3}s)",
                expected_duration_sec, effective_video_duration, drift, tolerance
            ));
        }

        // Check audio stream and enforce audio-video timeline alignment
        let (has_audio, audio_duration) = check_audio_stream_info(file_path);
        if let Some(a_dur) = audio_duration {
            let a_drift = (a_dur - effective_video_duration).abs();
            if a_drift > tolerance {
                return Err(format!(
                    "FLOW_OUTPUT_DURATION_MISMATCH: Audio stream duration ({:.3}s) differs from video stream duration ({:.3}s) by {:.3}s > tolerance {:.3}s",
                    a_dur, effective_video_duration, a_drift, tolerance
                ));
            }
        }

        Ok(FlowOutputArtifactRecord {
            final_path: file_path.to_path_buf(),
            sha256,
            duration_sec: effective_video_duration,
            width,
            height,
            fps,
            frame_count,
            has_audio,
            validated_at: Utc::now().to_rfc3339(),
        })
    }
}

fn parse_fraction(val: &str) -> Option<f64> {
    if let Some((num, den)) = val.split_once('/') {
        let n: f64 = num.trim().parse().ok()?;
        let d: f64 = den.trim().parse().ok()?;
        if d != 0.0 {
            return Some(n / d);
        }
    } else {
        return val.trim().parse().ok();
    }
    None
}

fn check_audio_stream_info(file_path: &Path) -> (bool, Option<f64>) {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a:0")
        .arg("-show_entries")
        .arg("stream=duration:format=duration")
        .arg("-of")
        .arg("json")
        .arg(file_path)
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            let json_str = String::from_utf8_lossy(&out.stdout);
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                let stream_dur = val
                    .get("streams")
                    .and_then(|s| s.get(0))
                    .and_then(|st| st.get("duration"))
                    .and_then(|d| d.as_str())
                    .and_then(|s| s.parse::<f64>().ok());

                if let Some(dur) = stream_dur {
                    return (true, Some(dur));
                }

                let has_stream = val
                    .get("streams")
                    .and_then(|s| s.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);

                if has_stream {
                    let fmt_dur = val
                        .get("format")
                        .and_then(|f| f.get("duration"))
                        .and_then(|d| d.as_str())
                        .and_then(|s| s.parse::<f64>().ok());
                    return (true, fmt_dur);
                }
            }
        }
    }
    (false, None)
}
