use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::command;

use crate::error::AppError;
use crate::media::{
    AudioExtractionResult, FrameExtractionRequest, FrameExtractionResult, MediaMetadata,
    MediaRuntimeStatus, MediaService,
};
use crate::models::{ModelDescriptor, ModelManager};
use crate::projects::{Project, ProjectManager, ProjectStatus, ProjectSummary};
use crate::system::{HardwareProfile, StoragePaths};

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
pub fn list_projects() -> Result<Vec<ProjectSummary>, AppError> {
    let manager = ProjectManager::new(StoragePaths::default_paths());
    manager.list_projects()
}

#[command]
pub fn get_project(id: String) -> Result<Project, AppError> {
    let manager = ProjectManager::new(StoragePaths::default_paths());
    manager.get_project(&id)
}

#[command]
pub fn create_project(name: String) -> Result<Project, AppError> {
    let manager = ProjectManager::new(StoragePaths::default_paths());
    manager.create_project(&name)
}

#[command]
pub fn update_project(project: Project) -> Result<Project, AppError> {
    let manager = ProjectManager::new(StoragePaths::default_paths());
    manager.update_project(&project)
}

#[command]
pub fn delete_project(id: String) -> Result<(), AppError> {
    let manager = ProjectManager::new(StoragePaths::default_paths());
    manager.delete_project(&id)
}

#[command]
pub fn probe_media(file_path: String) -> Result<MediaMetadata, AppError> {
    let media_service = MediaService::new();
    let path = PathBuf::from(&file_path);
    media_service.probe(&path)
}

#[command]
pub fn import_media(project_id: String, file_path: String) -> Result<Project, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    let mut project = manager.get_project(&project_id)?;

    let media_service = MediaService::new();
    let proj_dir = manager.project_dir(&project_id);
    let source_file = PathBuf::from(&file_path);

    let source_media = media_service.import_to_project(&proj_dir, &source_file)?;

    project.source_media = Some(source_media);
    project.status = ProjectStatus::Imported;

    manager.update_project(&project)
}

#[command]
pub fn get_media_runtime_status() -> Result<MediaRuntimeStatus, AppError> {
    let media_service = MediaService::new();
    Ok(media_service.check_runtime_status())
}

#[command]
pub fn prepare_media(project_id: String, media_id: String) -> Result<String, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    let proj_dir = manager.project_dir(&project_id);

    let media_service = MediaService::new();
    let cache_dir = media_service.prepare_media(&proj_dir, &media_id)?;
    Ok(cache_dir.display().to_string())
}

#[command]
pub fn extract_media_frames(request: FrameExtractionRequest) -> Result<FrameExtractionResult, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    let project = manager.get_project(&request.project_id)?;
    let proj_dir = manager.project_dir(&request.project_id);

    let source_media = project.source_media.ok_or_else(|| {
        AppError::invalid_input("Project does not have an imported source media file")
    })?;

    let media_service = MediaService::new();
    media_service.extract_frames(&proj_dir, &source_media.source_path, &request)
}

#[command]
pub fn extract_media_audio(project_id: String, media_id: String) -> Result<AudioExtractionResult, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    let project = manager.get_project(&project_id)?;
    let proj_dir = manager.project_dir(&project_id);

    let source_media = project.source_media.ok_or_else(|| {
        AppError::invalid_input("Project does not have an imported source media file")
    })?;

    let media_service = MediaService::new();
    media_service.extract_audio(&proj_dir, &source_media.source_path, &media_id)
}
