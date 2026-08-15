use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub file_name: String,
    pub file_path: String,
    pub duration_seconds: f64,
    pub duration_formatted: String,
    pub resolution: String,
    pub width: u32,
    pub height: u32,
    pub file_size_bytes: u64,
    pub file_size_formatted: String,
    pub fps: f64,
    pub codec: String,
}
