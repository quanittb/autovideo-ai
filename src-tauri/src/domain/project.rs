use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub source_video_path: Option<String>,
    pub media_info: Option<super::media::MediaInfo>,
    pub transformation: TransformationConfig,
    pub is_mock_demo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformationConfig {
    pub category: String, // "scene", "character", "style", "advanced"
    pub original_character: Option<String>,
    pub replacement_character: Option<String>,
    pub prompt: String,
    pub resolution: String,
    pub quality: String,
    pub format: String,
    pub fps: u32,
    pub remove_watermark: bool,
}

impl Default for TransformationConfig {
    fn default() -> Self {
        Self {
            category: "character".to_string(),
            original_character: Some("Fox".to_string()),
            replacement_character: Some("Rabbit".to_string()),
            prompt: "A cute white rabbit wearing a scarf in an autumn forest".to_string(),
            resolution: "1080p (1920x1080)".to_string(),
            quality: "High Quality".to_string(),
            format: "MP4".to_string(),
            fps: 30,
            remove_watermark: true,
        }
    }
}
