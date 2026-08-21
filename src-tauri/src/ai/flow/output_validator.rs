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
            .or_else(|| {
                val.get("format")
                    .and_then(|f| f.get("duration"))
                    .and_then(|d| d.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
            })
            .unwrap_or(0.0);

        if duration_sec <= 0.0 {
            return Err("VALIDATION_FAILED: Video stream duration must be positive".to_string());
        }

        // Duration drift check: must not differ by more than tolerance (e.g. 2.0s)
        let drift = (duration_sec - expected_duration_sec).abs();
        if drift > 2.0 {
            return Err(format!(
                "VALIDATION_FAILED: Duration drift too large (expected {:.2}s, got {:.2}s, drift {:.2}s)",
                expected_duration_sec, duration_sec, drift
            ));
        }

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

        if frame_count == 0 {
            return Err("VALIDATION_FAILED: Video has 0 frames".to_string());
        }

        // Check if has audio stream
        let has_audio = check_has_audio_stream(file_path);

        Ok(FlowOutputArtifactRecord {
            final_path: file_path.to_path_buf(),
            sha256,
            duration_sec,
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

fn check_has_audio_stream(file_path: &Path) -> bool {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a:0")
        .arg("-show_entries")
        .arg("stream=codec_type")
        .arg("-of")
        .arg("csv=p=0")
        .arg(file_path)
        .output();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        return !stdout.is_empty() && stdout.contains("audio");
    }
    false
}
