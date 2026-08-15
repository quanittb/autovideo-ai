use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::media::MediaAsset;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransformationRequest {
    pub category: String, // "character" (MVP), "scene", "style", "advanced"
    pub original_character: Option<String>,
    pub replacement_character: Option<String>,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub seed: Option<u64>,
}

impl Default for TransformationRequest {
    fn default() -> Self {
        Self {
            category: "character".to_string(),
            original_character: Some("Fox".to_string()),
            replacement_character: Some("Rabbit".to_string()),
            prompt: "A cute white rabbit wearing a scarf".to_string(),
            negative_prompt: None,
            seed: Some(42),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransformationPlan {
    pub estimated_frames: u64,
    pub pipeline_steps: Vec<String>,
    pub required_models: Vec<String>,
    pub estimated_duration_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub source_asset: Option<MediaAsset>,
    pub transformation_request: TransformationRequest,
    pub transformation_plan: Option<TransformationPlan>,
    pub output_video_path: Option<PathBuf>,
    pub is_fixture: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub thumbnail_path: Option<String>,
    pub has_output: bool,
    pub is_fixture: bool,
}

impl From<&Project> for ProjectSummary {
    fn from(p: &Project) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            created_at: p.created_at.clone(),
            updated_at: p.updated_at.clone(),
            thumbnail_path: p.source_asset.as_ref().and_then(|a| a.thumbnail_path.as_ref().map(|p| p.to_string_lossy().to_string())),
            has_output: p.output_video_path.is_some(),
            is_fixture: p.is_fixture,
        }
    }
}
