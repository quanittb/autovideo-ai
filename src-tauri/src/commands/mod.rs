use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::command;

use crate::error::AppError;
use crate::models::{ModelDescriptor, ModelManager};
use crate::projects::{Project, ProjectSummary, TransformationRequest};
use crate::system::{HardwareProfile, StoragePaths};
use crate::media::{MediaAsset, VideoMetadata};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub environment: String,
}

#[command]
pub fn get_app_info() -> AppInfo {
    AppInfo {
        name: "AutoVideo AI".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        environment: if cfg!(debug_assertions) { "development".to_string() } else { "production".to_string() },
    }
}

#[command]
pub fn get_hardware_profile() -> HardwareProfile {
    HardwareProfile::detect()
}

#[command]
pub fn get_storage_paths() -> StoragePaths {
    StoragePaths::default_paths()
}

#[command]
pub fn get_ai_status() -> Result<String, AppError> {
    // Strictly adhering to NEVER FAKE AI rule
    Err(AppError::model_not_available(
        "Character Inpainting Diffusion v1.0",
        "Local model weights are not installed. Download weights in Settings or use fixture demo mode.",
    ))
}

#[command]
pub fn list_models() -> Vec<ModelDescriptor> {
    let manager = ModelManager::new(StoragePaths::default_paths().models_dir);
    manager.list_available_descriptors()
}

#[command]
pub fn list_projects() -> Vec<ProjectSummary> {
    let sample_proj = get_fixture_project();
    vec![
        ProjectSummary::from(&sample_proj),
        ProjectSummary {
            id: "proj-winter".to_string(),
            name: "Winter to Autumn".to_string(),
            created_at: "2 hours ago".to_string(),
            updated_at: "2 hours ago".to_string(),
            thumbnail_path: None,
            has_output: false,
            is_fixture: true,
        },
        ProjectSummary {
            id: "proj-beach".to_string(),
            name: "Beach Vacation".to_string(),
            created_at: "2 days ago".to_string(),
            updated_at: "2 days ago".to_string(),
            thumbnail_path: None,
            has_output: false,
            is_fixture: true,
        },
        ProjectSummary {
            id: "proj-market".to_string(),
            name: "Home to Market".to_string(),
            created_at: "3 days ago".to_string(),
            updated_at: "3 days ago".to_string(),
            thumbnail_path: None,
            has_output: false,
            is_fixture: true,
        },
    ]
}

#[command]
pub fn get_project(id: String) -> Result<Project, AppError> {
    if id == "proj-fox-rabbit" || id == "proj-2" {
        Ok(get_fixture_project())
    } else {
        Err(AppError::file_not_found(format!("Project with ID '{}' not found", id)))
    }
}

#[command]
pub fn create_project(name: String) -> Result<Project, AppError> {
    if name.trim().is_empty() {
        return Err(AppError::invalid_input("Project name cannot be empty"));
    }

    Ok(Project {
        id: format!("proj-{}", uuid::Uuid::new_v4().simple()),
        name,
        created_at: "Just now".to_string(),
        updated_at: "Just now".to_string(),
        source_asset: None,
        transformation_request: TransformationRequest::default(),
        transformation_plan: None,
        output_video_path: None,
        is_fixture: false,
    })
}

#[command]
pub fn delete_project(id: String) -> Result<bool, AppError> {
    if id.is_empty() {
        return Err(AppError::invalid_input("Project ID cannot be empty"));
    }
    Ok(true)
}

fn get_fixture_project() -> Project {
    Project {
        id: "proj-fox-rabbit".to_string(),
        name: "Fox to Rabbit".to_string(),
        created_at: "1 day ago".to_string(),
        updated_at: "1 day ago".to_string(),
        source_asset: Some(MediaAsset {
            id: "asset-fox-1".to_string(),
            file_name: "input_video.mp4".to_string(),
            file_path: PathBuf::from("fixtures/videos/input_video.mp4"),
            metadata: VideoMetadata {
                width: 1920,
                height: 1080,
                duration_seconds: 62.0,
                duration_formatted: "01:02".to_string(),
                fps: 30.0,
                total_frames: 1860,
                codec: "h264".to_string(),
                audio_codec: Some("aac".to_string()),
                audio_sample_rate: Some(48000),
                bitrate_kbps: 6000,
                file_size_bytes: 47_395_840,
                file_size_formatted: "45.2 MB".to_string(),
            },
            thumbnail_path: None,
            is_fixture: true,
        }),
        transformation_request: TransformationRequest {
            category: "character".to_string(),
            original_character: Some("Fox".to_string()),
            replacement_character: Some("Rabbit".to_string()),
            prompt: "A cute white rabbit wearing a scarf".to_string(),
            negative_prompt: None,
            seed: Some(42),
        },
        transformation_plan: None,
        output_video_path: None,
        is_fixture: true,
    }
}
