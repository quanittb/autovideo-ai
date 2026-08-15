use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettings {
    pub resolution: String, // "1080p (1920x1080)", "4K (3840x2160)", "720p"
    pub quality: String,    // "High Quality", "Standard", "Lossless"
    pub format: String,     // "MP4", "MOV", "MKV"
    pub fps: u32,           // 24, 30, 60
    pub remove_watermark: bool,
    pub output_directory: Option<PathBuf>,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            resolution: "1080p (1920x1080)".to_string(),
            quality: "High Quality".to_string(),
            format: "MP4".to_string(),
            fps: 30,
            remove_watermark: true,
            output_directory: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub output_path: PathBuf,
    pub file_size_bytes: u64,
    pub duration_seconds: f64,
    pub rendered_frames: u64,
    pub render_time_seconds: f64,
}
