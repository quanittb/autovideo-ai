use std::path::PathBuf;
use std::process::Command as StdCommand;
use serde::{Deserialize, Serialize};
use tauri::command;

use crate::error::AppError;
use crate::media::{
    AudioExtractionResult, CacheValidationReport, FrameExtractionRequest, FrameExtractionResult,
    MediaMetadata, MediaRuntimeStatus, MediaService,
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
    let raw_path = PathBuf::from(&file_path);
    let target_path = if raw_path.is_dir() {
        let mut found_video: Option<PathBuf> = None;
        if let Ok(entries) = std::fs::read_dir(&raw_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if crate::media::SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                            found_video = Some(p);
                            break;
                        }
                    }
                }
            }
        }
        found_video.ok_or_else(|| {
            AppError::invalid_input(format!("Thư mục '{}' không chứa tệp video hợp lệ (MP4, MOV, AVI, MKV)", raw_path.display()))
        })?
    } else {
        raw_path
    };

    let media_service = MediaService::new();
    media_service.probe(&target_path)
}

#[command]
pub fn import_media(project_id: String, file_path: String) -> Result<Project, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    
    // Auto-create project if the ID is a mock fixture (e.g. proj-fox-rabbit) or missing
    let mut project = match manager.get_project(&project_id) {
        Ok(p) => p,
        Err(_) => {
            let base_name = std::path::Path::new(&file_path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("Imported Project");
            manager.create_project(base_name)?
        }
    };

    let target_id = project.id.clone();
    let proj_dir = manager.project_dir(&target_id);

    let raw_path = PathBuf::from(&file_path);
    let source_file = if raw_path.is_dir() {
        let mut found_video: Option<PathBuf> = None;
        if let Ok(entries) = std::fs::read_dir(&raw_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if crate::media::SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                            found_video = Some(p);
                            break;
                        }
                    }
                }
            }
        }
        found_video.ok_or_else(|| {
            AppError::invalid_input(format!("Thư mục '{}' không chứa tệp video hợp lệ (MP4, MOV, AVI, MKV)", raw_path.display()))
        })?
    } else {
        raw_path
    };

    let media_service = MediaService::new();
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

#[command]
pub fn validate_media_cache(project_id: String, media_id: String) -> Result<CacheValidationReport, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    let proj_dir = manager.project_dir(&project_id);

    let media_service = MediaService::new();
    media_service.validate_media_cache(&proj_dir, &media_id)
}

#[command]
pub fn open_directory(path: String) -> Result<(), AppError> {
    let target = PathBuf::from(&path);
    if !target.exists() {
        return Err(AppError::file_not_found(path));
    }

    #[cfg(target_os = "windows")]
    {
        let _ = StdCommand::new("explorer").arg(&path).spawn();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = StdCommand::new("open").arg(&path).spawn();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = StdCommand::new("xdg-open").arg(&path).spawn();
    }

    Ok(())
}

#[command]
pub fn resolve_project_media(project_id: String) -> Result<crate::media::ResolvedMediaAsset, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    let project = manager.get_project(&project_id)?;
    let proj_dir = manager.project_dir(&project_id);

    let source_media = project.source_media.ok_or_else(|| {
        AppError::invalid_input("Project does not have an imported source media file")
    })?;

    let media_service = MediaService::new();
    media_service.resolve_project_media(&proj_dir, &source_media)
}

#[command]
pub fn persist_editor_state(
    project_id: String,
    editor_state: crate::projects::ProjectEditorState,
) -> Result<(), AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    let mut project = manager.get_project(&project_id)?;

    project.editor_state = Some(editor_state);
    manager.update_project(&project)?;
    Ok(())
}

#[command]
pub fn render_test_video(request: crate::render::RenderRequest) -> Result<crate::render::RenderResult, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    let mut project = manager.get_project(&request.project_id)?;
    let proj_dir = manager.project_dir(&request.project_id);

    let source_media = project.source_media.clone().ok_or_else(|| {
        AppError::invalid_input("Project does not have an imported source media file")
    })?;

    let render_service = crate::render::RenderService::new();
    let result = render_service.render_video(&proj_dir, &source_media, &request)?;

    // Append generated project output to project model
    project.outputs.push(result.project_output.clone());
    let _ = manager.update_project(&project);

    Ok(result)
}
