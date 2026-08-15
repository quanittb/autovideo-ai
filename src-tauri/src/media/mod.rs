use std::path::PathBuf;
use serde::{Deserialize, Serialize};

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
    pub label: String, // e.g. "Fox", "Human", "Dog"
    pub confidence: f32,
    pub bounding_box: [f32; 4], // [x, y, w, h] normalized
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
