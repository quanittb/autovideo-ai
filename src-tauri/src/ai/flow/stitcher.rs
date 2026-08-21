use super::manifest::{FlowFinalAudioPolicy, FlowOutputArtifactRecord};
use super::output_validator::FlowOutputValidator;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct FlowStitcher;

impl FlowStitcher {
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
            // No audio muxing needed; rename temp video to output
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
