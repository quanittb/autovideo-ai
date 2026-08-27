use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::Arc;
use tauri::{command, AppHandle, Emitter, Manager};

use crate::ai::generative::backend::GenerativeBackend;
use crate::ai::runtime::AiRuntime;
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
        environment: if cfg!(debug_assertions) {
            "development".to_string()
        } else {
            "production".to_string()
        },
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
                        if crate::media::SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
                        {
                            found_video = Some(p);
                            break;
                        }
                    }
                }
            }
        }
        found_video.ok_or_else(|| {
            AppError::invalid_input(format!(
                "Thư mục '{}' không chứa tệp video hợp lệ (MP4, MOV, AVI, MKV)",
                raw_path.display()
            ))
        })?
    } else {
        raw_path
    };

    let media_service = MediaService::new();
    media_service.probe(&target_path)
}

#[command]
pub fn import_media(
    app: AppHandle,
    project_id: String,
    file_path: String,
) -> Result<Project, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths.clone());

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
                        if crate::media::SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
                        {
                            found_video = Some(p);
                            break;
                        }
                    }
                }
            }
        }
        found_video.ok_or_else(|| {
            AppError::invalid_input(format!(
                "Thư mục '{}' không chứa tệp video hợp lệ (MP4, MOV, AVI, MKV)",
                raw_path.display()
            ))
        })?
    } else {
        raw_path
    };

    let media_service = MediaService::new();
    let source_media = media_service.import_to_project(&proj_dir, &source_file)?;

    // Dynamically authorize newly imported file into asset protocol scope
    if let Ok((canonical_file, _)) = resolve_project_source_preview_path(&target_id, &storage_paths)
    {
        let _ = app.asset_protocol_scope().allow_file(&canonical_file);
    }

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
pub fn extract_media_frames(
    request: FrameExtractionRequest,
) -> Result<FrameExtractionResult, AppError> {
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
pub fn extract_media_audio(
    project_id: String,
    media_id: String,
) -> Result<AudioExtractionResult, AppError> {
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
pub fn validate_media_cache(
    project_id: String,
    media_id: String,
) -> Result<CacheValidationReport, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths);
    let proj_dir = manager.project_dir(&project_id);

    let media_service = MediaService::new();
    media_service.validate_media_cache(&proj_dir, &media_id)
}

#[command]
pub fn open_file_path(path: String) -> Result<(), AppError> {
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
pub fn open_directory(path: String) -> Result<(), AppError> {
    let target = PathBuf::from(&path);
    if !target.exists() {
        return Err(AppError::file_not_found(path));
    }

    let dir_to_open = if target.is_file() {
        target.parent().unwrap_or(&target).to_path_buf()
    } else {
        target.clone()
    };

    #[cfg(target_os = "windows")]
    {
        if target.is_file() {
            let _ = StdCommand::new("explorer")
                .arg(format!("/select,{}", path))
                .spawn();
        } else {
            let _ = StdCommand::new("explorer")
                .arg(dir_to_open.to_string_lossy().as_ref())
                .spawn();
        }
    }

    #[cfg(target_os = "macos")]
    {
        let _ = StdCommand::new("open")
            .arg(dir_to_open.to_string_lossy().as_ref())
            .spawn();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = StdCommand::new("xdg-open")
            .arg(dir_to_open.to_string_lossy().as_ref())
            .spawn();
    }

    Ok(())
}

#[command]
pub fn resolve_project_media(
    app: AppHandle,
    project_id: String,
) -> Result<crate::media::ResolvedMediaAsset, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let (canonical_source, source_media) =
        resolve_project_source_preview_path(&project_id, &storage_paths)
            .map_err(|e| AppError::invalid_input(e))?;

    // Dynamically authorize exact canonical file into Tauri asset protocol scope
    let _ = app.asset_protocol_scope().allow_file(&canonical_source);

    let manager = ProjectManager::new(storage_paths);
    let proj_dir = manager.project_dir(&project_id);

    let media_service = MediaService::new();
    let mut resolved = media_service.resolve_project_media(&proj_dir, &source_media)?;
    resolved.source_path = canonical_source;
    Ok(resolved)
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
pub fn render_test_video(
    request: crate::render::RenderRequest,
) -> Result<crate::render::RenderResult, AppError> {
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

#[command]
pub fn create_pipeline_job(
    app: tauri::AppHandle,
    project_id: String,
    job_type: Option<String>,
    input_files: Option<Vec<String>>,
) -> Result<crate::jobs::Job, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let manager = ProjectManager::new(storage_paths.clone());
    let project = manager.get_project(&project_id)?;

    let inputs = match input_files {
        Some(f) if !f.is_empty() => f,
        _ => {
            if let Some(ref sm) = project.source_media {
                vec![sm.source_path.display().to_string()]
            } else {
                return Err(AppError::invalid_input(
                    "Project has no source media to process",
                ));
            }
        }
    };

    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.create_job_with_app(Some(&app), &project_id, job_type, inputs)
}

#[command]
pub async fn start_pipeline_job(
    app: tauri::AppHandle,
    job_id: String,
) -> Result<crate::jobs::Job, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.start_job(Some(&app), &job_id).await
}

#[command]
pub async fn cancel_pipeline_job(
    app: tauri::AppHandle,
    job_id: String,
) -> Result<crate::jobs::Job, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.cancel_job(Some(&app), &job_id).await
}

#[command]
pub async fn retry_pipeline_job(
    app: tauri::AppHandle,
    job_id: String,
) -> Result<crate::jobs::Job, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.retry_job(Some(&app), &job_id).await
}

#[command]
pub fn delete_pipeline_job(job_id: String) -> Result<(), AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.delete_job(&job_id)
}

#[command]
pub fn get_pipeline_job(job_id: String) -> Result<crate::jobs::Job, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.get_job(&job_id)
}

#[command]
pub fn list_pipeline_jobs(project_id: Option<String>) -> Result<Vec<crate::jobs::Job>, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.list_jobs(project_id.as_deref())
}

#[command]
pub fn get_job_logs(job_id: String) -> Result<Vec<String>, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.get_job_logs(&job_id)
}

#[command]
pub fn get_job_artifacts(job_id: String) -> Result<Vec<crate::jobs::Artifact>, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.get_job_artifacts(&job_id)
}

#[command]
pub fn validate_pipeline_job(job_id: String) -> Result<crate::jobs::JobValidationReport, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    let job = engine.get_job(&job_id)?;
    Ok(engine.validate_job_stage_artifacts(&job))
}

// -------------------------------------------------------------
// PHASE 6A: AI MODEL RUNTIME & REGISTRY COMMANDS
// -------------------------------------------------------------

#[command]
pub fn list_ai_models() -> Result<Vec<crate::ai::AiModelManifest>, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    registry.list_models()
}

#[command]
pub fn get_ai_model(model_id: String) -> Result<crate::ai::AiModelManifest, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    registry.get_model(&model_id)
}

#[command]
pub fn register_ai_model(
    manifest: crate::ai::AiModelManifest,
) -> Result<crate::ai::AiModelManifest, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    registry.register_model(manifest)
}

#[command]
pub fn unregister_ai_model(model_id: String) -> Result<(), AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    registry.unregister_model(&model_id)
}

#[command]
pub fn get_ai_runtime_status() -> Result<crate::ai::RuntimeStatus, AppError> {
    let runtime = crate::ai::onnx::get_global_ai_runtime();
    let r = runtime
        .lock()
        .map_err(|e| AppError::process_failed(format!("Failed to lock AI runtime: {}", e)))?;
    Ok(r.status())
}

#[command]
pub fn get_ai_devices() -> Result<crate::ai::DeviceInfo, AppError> {
    Ok(crate::ai::DeviceInfo::detect())
}

#[command]
pub fn get_ai_providers() -> Result<Vec<crate::ai::ProviderInfo>, AppError> {
    Ok(crate::ai::detect_providers())
}

#[command]
pub fn load_ai_model(
    model_id: String,
    provider: Option<crate::ai::ExecutionProvider>,
) -> Result<crate::ai::OnnxModelMetadata, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    let mut manifest = registry.get_model(&model_id)?;

    if let Some(p) = provider {
        manifest.requirements.preferred_provider = Some(p);
    }

    let runtime = crate::ai::onnx::get_global_ai_runtime();
    let mut r = runtime
        .lock()
        .map_err(|e| AppError::process_failed(format!("Failed to lock AI runtime: {}", e)))?;

    r.load_model(&manifest)?;
    r.inspect_active_model()
}

#[command]
pub fn unload_ai_model() -> Result<(), AppError> {
    let runtime = crate::ai::onnx::get_global_ai_runtime();
    let mut r = runtime
        .lock()
        .map_err(|e| AppError::process_failed(format!("Failed to lock AI runtime: {}", e)))?;
    r.unload_model()
}

#[command]
pub fn inspect_ai_model() -> Result<crate::ai::OnnxModelMetadata, AppError> {
    let runtime = crate::ai::onnx::get_global_ai_runtime();
    let r = runtime
        .lock()
        .map_err(|e| AppError::process_failed(format!("Failed to lock AI runtime: {}", e)))?;
    r.inspect_active_model()
}

#[command]
pub fn run_ai_inference(
    request: crate::ai::InferenceRequest,
) -> Result<crate::ai::InferenceResult, AppError> {
    let runtime = crate::ai::onnx::get_global_ai_runtime();
    let r = runtime
        .lock()
        .map_err(|e| AppError::process_failed(format!("Failed to lock AI runtime: {}", e)))?;
    r.infer(&request)
}

#[command]
pub fn generate_test_model(
    target_path: Option<String>,
) -> Result<crate::ai::AiModelManifest, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let model_file = match target_path {
        Some(p) => PathBuf::from(p),
        None => storage_paths
            .models_dir
            .join("test-multiplier")
            .join("model.onnx"),
    };

    crate::ai::generate_minimal_onnx_model(&model_file)?;

    let manifest = crate::ai::AiModelManifest::new(
        "model-test-multiplier",
        "Deterministic Math Multiplier (Y = X * 2)",
        "1.0.0",
        crate::ai::ModelFormat::Onnx,
        model_file,
        "Real minimal deterministic ONNX mathematical model for testing and diagnostics.",
        vec![crate::ai::TensorSpec::new(
            "X",
            crate::ai::TensorDataType::Float32,
            vec![
                crate::ai::Dimension::fixed(1),
                crate::ai::Dimension::fixed(4),
            ],
        )],
        vec![crate::ai::TensorSpec::new(
            "Y",
            crate::ai::TensorDataType::Float32,
            vec![
                crate::ai::Dimension::fixed(1),
                crate::ai::Dimension::fixed(4),
            ],
        )],
        crate::ai::ModelRequirements {
            min_memory_mb: Some(32),
            requires_gpu: false,
            preferred_provider: Some(crate::ai::ExecutionProvider::Cpu),
        },
    );

    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    // Unregister first if already registered
    let _ = registry.unregister_model(&manifest.id);
    registry.register_model(manifest)
}

#[command]
pub fn generate_image_test_model(
    target_path: Option<String>,
) -> Result<crate::ai::AiModelManifest, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let model_file = match target_path {
        Some(p) => PathBuf::from(p),
        None => storage_paths
            .models_dir
            .join("test-image-multiplier")
            .join("model.onnx"),
    };

    crate::ai::generate_image_onnx_model(&model_file)?;

    let manifest = crate::ai::AiModelManifest::new(
        "model-test-image-multiplier",
        "Deterministic Image Multiplier (4D [1,3,2,2], Y = X * 2)",
        "1.0.0",
        crate::ai::ModelFormat::Onnx,
        model_file,
        "Real minimal deterministic ONNX image processing model for testing image tensor pipelines.",
        vec![crate::ai::TensorSpec::new(
            "images",
            crate::ai::TensorDataType::Float32,
            vec![
                crate::ai::Dimension::fixed(1),
                crate::ai::Dimension::fixed(3),
                crate::ai::Dimension::fixed(2),
                crate::ai::Dimension::fixed(2),
            ],
        )],
        vec![crate::ai::TensorSpec::new(
            "output",
            crate::ai::TensorDataType::Float32,
            vec![
                crate::ai::Dimension::fixed(1),
                crate::ai::Dimension::fixed(3),
                crate::ai::Dimension::fixed(2),
                crate::ai::Dimension::fixed(2),
            ],
        )],
        crate::ai::ModelRequirements {
            min_memory_mb: Some(32),
            requires_gpu: false,
            preferred_provider: Some(crate::ai::ExecutionProvider::Cpu),
        },
    );

    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    let _ = registry.unregister_model(&manifest.id);
    registry.register_model(manifest)
}

#[command]
pub fn preview_ai_preprocess(
    image_path: String,
    config: crate::ai::PreprocessConfig,
) -> Result<crate::ai::PreprocessResult, AppError> {
    let frame = crate::ai::ImageFrame::decode_from_file(&image_path)?;
    crate::ai::preprocess_image(&frame, &config, "input")
}

#[command]
pub fn validate_ai_preprocess(
    model_id: String,
    config: crate::ai::PreprocessConfig,
) -> Result<crate::ai::PreprocessValidationResult, AppError> {
    let runtime = crate::ai::onnx::get_global_ai_runtime();
    let r = runtime
        .lock()
        .map_err(|e| AppError::process_failed(format!("Failed to lock AI runtime: {}", e)))?;

    let active_model = r.loaded_model_id().ok_or_else(|| {
        AppError::model_not_available(
            &model_id,
            "No AI model currently loaded in session. Please load model first.",
        )
    })?;

    if active_model != model_id {
        return Err(AppError::invalid_input(format!(
            "Model mismatch: loaded '{}', requested '{}'",
            active_model, model_id
        )));
    }

    let meta = r.inspect_active_model()?;
    Ok(crate::ai::validate_preprocess_against_model(
        &config, &meta, None,
    ))
}

#[command]
pub fn run_ai_pipeline(
    model_id: String,
    image_path: String,
    preprocess_config: crate::ai::PreprocessConfig,
    postprocess_config: Option<crate::ai::PostprocessConfig>,
) -> Result<crate::ai::PipelineExecutionReport, AppError> {
    crate::ai::AiInferencePipeline::run_pipeline(
        &image_path,
        &model_id,
        &preprocess_config,
        postprocess_config.as_ref(),
    )
}

#[command]
pub fn decode_ai_mask(
    tensor: crate::ai::AiTensorOutput,
    threshold: Option<f32>,
) -> Result<crate::ai::Mask, AppError> {
    let mask = crate::ai::extract_mask_from_tensor(&tensor)?;
    if let Some(t) = threshold {
        Ok(mask.apply_threshold(t))
    } else {
        Ok(mask)
    }
}

// -------------------------------------------------------------
// PHASE 6D: AI VIDEO FRAME INFERENCE COMMANDS
// -------------------------------------------------------------

#[command]
pub fn create_ai_pipeline_job<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    project_id: String,
    input_files: Vec<String>,
    ai_config: crate::ai::AiJobConfig,
) -> Result<crate::jobs::Job, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.create_ai_job_with_app(
        Some(&app),
        &project_id,
        Some("ai_video_pipeline".to_string()),
        input_files,
        ai_config,
    )
}

#[command]
pub fn get_ai_job_metrics(
    project_id: String,
    job_id: String,
) -> Result<Option<crate::ai::AiJobMetrics>, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    let job = engine.get_job(&job_id)?;
    if job.project_id != project_id {
        return Err(AppError::invalid_input(format!(
            "Job '{}' does not belong to project '{}'",
            job_id, project_id
        )));
    }
    Ok(job.ai_metrics)
}

#[command]
pub fn validate_ai_frame_artifacts(
    project_id: String,
    job_id: String,
) -> Result<serde_json::Value, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let proj_dir = storage_paths.projects_dir.join(&project_id);
    let ai_cache_dir = proj_dir.join("cache").join("ai").join(&job_id);
    let artifact_mgr = crate::ai::AiArtifactManager::new(&ai_cache_dir);

    let engine = crate::jobs::JobEngine::new(storage_paths);
    let job = engine.get_job(&job_id)?;
    let ai_cfg = job
        .ai_config
        .as_ref()
        .ok_or_else(|| AppError::invalid_input("Job does not have AI config"))?;

    let expected_hash = crate::ai::compute_ai_config_hash(
        &ai_cfg.model_id,
        &ai_cfg.preprocessing,
        ai_cfg.postprocessing.as_ref(),
    );

    let mut valid_count = 0;
    let mut total_count = 0;

    let recon_dir = artifact_mgr.reconstruction_frames_dir();
    if recon_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&recon_dir) {
            for entry in entries.flatten() {
                if entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("png"))
                    .unwrap_or(false)
                {
                    total_count += 1;
                    if let Ok(meta) = std::fs::metadata(entry.path()) {
                        if meta.len() > 0 {
                            valid_count += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(serde_json::json!({
        "jobId": job_id,
        "projectId": project_id,
        "configHash": expected_hash,
        "totalFramesFound": total_count,
        "validFrames": valid_count,
        "isConsistent": total_count > 0 && valid_count == total_count,
    }))
}

// =========================================================================
// PHASE 6F: AI MODEL MANAGEMENT IPC COMMANDS
// =========================================================================

#[command]
pub fn list_ai_model_families() -> Result<Vec<crate::ai::AiModelFamily>, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    registry.list_families()
}

#[command]
pub fn list_ai_model_packages() -> Result<Vec<crate::ai::AiModelPackage>, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    registry.list_packages()
}

#[command]
pub fn get_ai_model_package(
    model_id: String,
    version: Option<String>,
) -> Result<crate::ai::AiModelPackage, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    if let Some(v) = version {
        registry.get_package(&model_id, &v)
    } else {
        registry.get_active_package(&model_id)
    }
}

#[command]
pub fn validate_ai_model_package(
    model_id: String,
    version: String,
) -> Result<crate::ai::ModelValidationReport, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    registry.validate_package(&model_id, &version)
}

#[command]
pub fn import_ai_model(
    app: AppHandle,
    source_path: String,
    model_id: String,
    model_name: String,
    version: String,
    display_name: String,
    description: String,
    profile: crate::ai::AiModelProfile,
    requirements: Option<crate::ai::ModelRequirements>,
    supported_providers: Option<Vec<crate::ai::ExecutionProvider>>,
) -> Result<crate::ai::AiModelPackage, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);

    let reqs = requirements.unwrap_or_default();
    let providers = supported_providers.unwrap_or_else(|| {
        vec![
            crate::ai::ExecutionProvider::Cpu,
            crate::ai::ExecutionProvider::DirectML,
        ]
    });

    let package = registry.import_model(
        Path::new(&source_path),
        &model_id,
        &model_name,
        &version,
        &display_name,
        &description,
        profile,
        reqs,
        providers,
    )?;

    let _ = app.emit(
        crate::events::EventNames::AI_MODEL_IMPORTED,
        &crate::events::AiModelImportedEvent {
            model_id: package.model_id.clone(),
            version: package.version.clone(),
            sha256: package.sha256.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    Ok(package)
}

#[command]
pub fn activate_ai_model_version(
    app: AppHandle,
    model_id: String,
    version: String,
) -> Result<crate::ai::AiModelPackage, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);

    let package = registry.activate_version(&model_id, &version)?;

    let _ = app.emit(
        crate::events::EventNames::AI_MODEL_ACTIVATED,
        &crate::events::AiModelActivatedEvent {
            model_id: package.model_id.clone(),
            version: package.version.clone(),
            previous_version: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    Ok(package)
}

#[command]
pub fn rollback_ai_model(
    app: AppHandle,
    model_id: String,
) -> Result<crate::ai::AiModelPackage, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);

    let package = registry.rollback_model(&model_id)?;

    let _ = app.emit(
        crate::events::EventNames::AI_MODEL_ROLLBACK_COMPLETED,
        &crate::events::AiModelRollbackEvent {
            model_id: package.model_id.clone(),
            restored_version: package.version.clone(),
            previous_version: "".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    Ok(package)
}

#[command]
pub fn remove_ai_model_version(
    model_id: String,
    version: String,
) -> Result<crate::ai::AiModelPackage, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    registry.remove_version(&model_id, &version)
}

#[command]
pub fn resolve_production_model(
    model_id: Option<String>,
    version: Option<String>,
    provider: Option<crate::ai::ExecutionProvider>,
) -> Result<crate::ai::ResolvedProductionModel, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir);
    crate::ai::ProductionModelResolver::resolve_model(
        &registry,
        model_id.as_deref(),
        version.as_deref(),
        provider,
    )
}

#[command]
pub fn validate_ai_job_preflight(
    app: AppHandle,
    source_path: String,
    ai_config: crate::ai::AiJobConfig,
) -> Result<crate::ai::AiJobPreflightReport, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let path = Path::new(&source_path);

    let _ = app.emit(
        crate::events::EventNames::AI_PREFLIGHT_STARTED,
        &crate::events::AiPreflightEvent {
            source_path: source_path.clone(),
            model_id: ai_config.model_id.clone(),
            is_valid: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    let report = crate::ai::validate_ai_job_preflight(path, &ai_config, &storage_paths)?;

    let event_name = if report.is_valid {
        crate::events::EventNames::AI_PREFLIGHT_COMPLETED
    } else {
        crate::events::EventNames::AI_PREFLIGHT_FAILED
    };

    let _ = app.emit(
        event_name,
        &crate::events::AiPreflightEvent {
            source_path,
            model_id: ai_config.model_id,
            is_valid: report.is_valid,
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    Ok(report)
}

#[command]
pub fn create_production_ai_job(
    app: AppHandle,
    project_id: String,
    input_files: Vec<String>,
    ai_config: crate::ai::AiJobConfig,
) -> Result<crate::jobs::Job, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.create_ai_job_with_app(Some(&app), &project_id, None, input_files, ai_config)
}

// -------------------------------------------------------------
// PHASE 6H: PRODUCTION EXECUTION, RESOURCE & REPORT COMMANDS
// -------------------------------------------------------------

#[command]
pub fn get_ai_resource_limits() -> Result<crate::ai::AiResourceLimits, AppError> {
    Ok(crate::ai::AiResourceLimits::default_production())
}

#[command]
pub fn get_ai_runtime_resources(
    model_id: Option<String>,
) -> Result<crate::ai::AiRuntimeResources, AppError> {
    let runtime = crate::ai::onnx::get_global_ai_runtime();
    let r = runtime
        .lock()
        .map_err(|e| AppError::process_failed(format!("Failed to lock AI runtime: {}", e)))?;
    let provider_name = format!("{:?}", r.provider());
    let active_model = r.loaded_model_id();
    let is_busy = matches!(r.status().state, crate::ai::RuntimeState::Running);

    Ok(crate::ai::probe_runtime_resources(
        &provider_name,
        active_model.as_deref().or(model_id.as_deref()),
        if is_busy { 1 } else { 0 },
        0,
    ))
}

#[command]
pub fn get_ai_execution_report(
    project_id: String,
    job_id: String,
) -> Result<crate::ai::AiProductionExecutionReport, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let proj_dir = storage_paths.projects_dir.join(&project_id);
    let report_path = proj_dir
        .join("outputs")
        .join(&job_id)
        .join("ai_execution_report.json");

    if !report_path.exists() {
        return Err(AppError::file_not_found(report_path.display().to_string()));
    }

    crate::ai::AiProductionExecutionReport::load_from_file(&report_path)
}

#[command]
pub fn validate_ai_artifacts(
    project_id: String,
    job_id: String,
) -> Result<Vec<crate::ai::AiFrameMetadata>, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let proj_dir = storage_paths.projects_dir.join(&project_id);
    let ai_cache_dir = proj_dir.join("cache").join("ai").join(&job_id);
    let artifact_mgr = crate::ai::AiArtifactManager::new(&ai_cache_dir);

    let engine = crate::jobs::JobEngine::new(storage_paths);
    let job = engine.get_job(&job_id)?;
    let ai_cfg = job
        .ai_config
        .ok_or_else(|| AppError::invalid_input("Job is not configured for AI processing"))?;

    let config_hash = crate::ai::compute_ai_config_hash(
        &ai_cfg.model_id,
        &ai_cfg.preprocessing,
        ai_cfg.postprocessing.as_ref(),
    );

    let mut valid_metas = Vec::new();
    if ai_cache_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&ai_cache_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    if let Some(dir_name) = p.file_name().and_then(|n| n.to_str()) {
                        if let Ok(idx) = dir_name.parse::<usize>() {
                            if let Some(meta) = artifact_mgr.validate_frame_artifact_deep(
                                idx,
                                &ai_cfg.model_id,
                                &config_hash,
                                ai_cfg.model_hash.as_deref(),
                                ai_cfg.profile_hash.as_deref(),
                            ) {
                                valid_metas.push(meta);
                            }
                        }
                    }
                }
            }
        }
    }

    valid_metas.sort_by_key(|m| m.frame_index);
    Ok(valid_metas)
}

// =============================================================================
// Phase 6J — Storage Management & Complete Job History Commands
// =============================================================================

fn dir_size_recursive(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                total += p.metadata().map(|m| m.len()).unwrap_or(0);
            } else if p.is_dir() {
                total += dir_size_recursive(&p);
            }
        }
    }
    total
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsageReport {
    pub projects_bytes: u64,
    pub cache_bytes: u64,
    pub ai_cache_bytes: u64,
    pub models_bytes: u64,
    pub temp_bytes: u64,
    pub logs_bytes: u64,
    pub total_bytes: u64,
}

#[command]
pub fn get_storage_usage() -> Result<StorageUsageReport, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let projects_bytes = dir_size_recursive(&storage_paths.projects_dir);
    let cache_bytes = dir_size_recursive(&storage_paths.cache_dir);
    let models_bytes = dir_size_recursive(&storage_paths.models_dir);
    let temp_bytes = dir_size_recursive(&storage_paths.temp_dir);
    let logs_bytes = dir_size_recursive(&storage_paths.logs_dir);

    let mut ai_cache_bytes = 0;
    if storage_paths.projects_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&storage_paths.projects_dir) {
            for entry in entries.flatten() {
                let ai_dir = entry.path().join("cache").join("ai");
                if ai_dir.exists() {
                    ai_cache_bytes += dir_size_recursive(&ai_dir);
                }
            }
        }
    }

    let total_bytes = projects_bytes + cache_bytes + models_bytes + temp_bytes + logs_bytes;

    Ok(StorageUsageReport {
        projects_bytes,
        cache_bytes,
        ai_cache_bytes,
        models_bytes,
        temp_bytes,
        logs_bytes,
        total_bytes,
    })
}

#[command]
pub fn clear_storage_cache() -> Result<u64, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let before_size = dir_size_recursive(&storage_paths.cache_dir);

    if storage_paths.cache_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&storage_paths.cache_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let _ = std::fs::remove_file(&p);
                } else if p.is_dir() {
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    }

    // Also clean non-active project intermediate caches
    if storage_paths.projects_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&storage_paths.projects_dir) {
            for entry in entries.flatten() {
                let media_cache = entry.path().join("cache").join("media");
                if media_cache.exists() {
                    let _ = std::fs::remove_dir_all(&media_cache);
                }
            }
        }
    }

    let after_size = dir_size_recursive(&storage_paths.cache_dir);
    let freed = before_size.saturating_sub(after_size);
    Ok(freed)
}

#[command]
pub fn cleanup_temp_storage() -> Result<u64, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let before_size = dir_size_recursive(&storage_paths.temp_dir);

    if storage_paths.temp_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&storage_paths.temp_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let _ = std::fs::remove_file(&p);
                } else if p.is_dir() {
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    }

    let after_size = dir_size_recursive(&storage_paths.temp_dir);
    let freed = before_size.saturating_sub(after_size);
    Ok(freed)
}

#[command]
pub fn get_all_job_history() -> Result<Vec<crate::jobs::Job>, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let engine = crate::jobs::JobEngine::new(storage_paths);
    engine.list_jobs(None)
}

fn resolve_sidecar_script_path() -> PathBuf {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest_dir)
            .join("scripts")
            .join("generative_sidecar.py");
        if p.exists() {
            return p;
        }
    }
    let local = PathBuf::from("src-tauri")
        .join("scripts")
        .join("generative_sidecar.py");
    if local.exists() {
        return local;
    }
    let local_scripts = PathBuf::from("scripts").join("generative_sidecar.py");
    if local_scripts.exists() {
        return local_scripts;
    }
    if let Ok(mut exe) = std::env::current_exe() {
        exe.pop();
        let p = exe.join("scripts").join("generative_sidecar.py");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("src-tauri/scripts/generative_sidecar.py")
}

#[command]
pub fn get_generative_capabilities() -> Result<crate::ai::generative::BackendCapabilities, AppError>
{
    let storage_paths = StoragePaths::default_paths();
    let script_path = resolve_sidecar_script_path();
    let backend = crate::ai::generative::PythonSidecarBackend::new(
        PathBuf::from("python"),
        script_path,
        storage_paths.app_data_dir,
        false,
    );
    backend.get_capabilities()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerativePreflightReport {
    pub is_valid: bool,
    pub backend_status: crate::ai::generative::BackendHealthStatus,
    pub capabilities: crate::ai::generative::BackendCapabilities,
    pub pose_model_installed: bool,
    pub depth_model_installed: bool,
    pub segmentation_model_installed: bool,
    pub missing_models: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

#[command]
pub fn check_generative_preflight() -> Result<GenerativePreflightReport, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::registry::ModelRegistry::new(storage_paths.models_dir);

    let pose_installed = registry.get_model("dwpose").is_ok();
    let depth_installed = registry.get_model("depth_anything_v2").is_ok();
    let seg_installed = registry.get_model("birefnet").is_ok();

    let mut missing_models = Vec::new();
    if !pose_installed {
        missing_models.push("dwpose (DWPose Whole-Body Pose)".to_string());
    }
    if !depth_installed {
        missing_models.push("depth_anything_v2 (Depth Anything V2)".to_string());
    }
    if !seg_installed {
        missing_models.push("birefnet (BiRefNet Segmentation)".to_string());
    }

    let script_path = resolve_sidecar_script_path();
    let backend = crate::ai::generative::PythonSidecarBackend::new(
        PathBuf::from("python"),
        script_path,
        storage_paths.app_data_dir,
        false,
    );

    let backend_status = backend.health_check()?;
    let capabilities = backend.get_capabilities()?;

    let mut warnings = Vec::new();
    let errors = Vec::new();

    if !backend_status.cuda_available {
        warnings.push("CUDA GPU acceleration is not active. Generative inference will run on CPU with reduced speed.".to_string());
    }

    if !missing_models.is_empty() {
        warnings.push(format!(
            "Control models not yet installed in local registry: {}. Fallback preprocessors will be used for preview.",
            missing_models.join(", ")
        ));
    }

    let is_valid = backend_status.healthy;

    Ok(GenerativePreflightReport {
        is_valid,
        backend_status,
        capabilities,
        pose_model_installed: pose_installed,
        depth_model_installed: depth_installed,
        segmentation_model_installed: seg_installed,
        missing_models,
        warnings,
        errors,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateKeyframeIpcRequest {
    pub job_id: String,
    pub source_video_path: String,
    pub source_frame_index: usize,
    pub character_reference_paths: Vec<String>,
    pub positive_prompt: String,
    pub negative_prompt: String,
    pub style_preset: String,
    pub steps: u32,
    pub cfg_scale: f32,
    pub denoise_strength: f32,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateKeyframeIpcResponse {
    pub result: crate::ai::generative::KeyframeGenerationResult,
    pub quality: crate::ai::generative::KeyframeQualityReport,
}

#[command]
pub fn generate_keyframe(
    request: GenerateKeyframeIpcRequest,
) -> Result<GenerateKeyframeIpcResponse, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let video_path = PathBuf::from(&request.source_video_path);

    let char_ref = crate::ai::generative::CharacterReference {
        image_paths: request
            .character_reference_paths
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        identity_weight: 0.85,
        appearance_weight: 0.75,
        crop_mode: "FACE_AND_UPPER_BODY".to_string(),
    };

    let env = crate::ai::generative::EnvironmentCondition {
        positive_prompt: request.positive_prompt,
        negative_prompt: request.negative_prompt,
        style_preset: request.style_preset,
    };

    let params = crate::ai::generative::GenerationParams {
        steps: request.steps,
        cfg_scale: request.cfg_scale,
        denoise_strength: request.denoise_strength,
        seed: request.seed,
        width: request.width,
        height: request.height,
        control_weights: std::collections::HashMap::new(),
    };

    let out_dir = storage_paths
        .cache_dir
        .join("keyframes")
        .join(&request.job_id);
    let _ = std::fs::create_dir_all(&out_dir);
    let output_path = out_dir.join("generated_keyframe.png");

    let script_path = resolve_sidecar_script_path();
    let backend = crate::ai::generative::PythonSidecarBackend::new(
        PathBuf::from("python"),
        script_path,
        storage_paths.temp_dir.clone(),
        false,
    );

    let (res, quality) = crate::ai::generative::KeyframeOrchestrator::execute_keyframe_job(
        &request.job_id,
        &video_path,
        request.source_frame_index,
        char_ref,
        env,
        params,
        &backend,
        &storage_paths.temp_dir,
        &output_path,
        None,
    )?;

    Ok(GenerateKeyframeIpcResponse {
        result: res,
        quality,
    })
}

#[command]
pub fn import_control_model(
    model_id: String,
    file_path: String,
    version: Option<String>,
) -> Result<crate::ai::package::AiModelPackage, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let registry = crate::ai::registry::ModelRegistry::new(storage_paths.models_dir);

    let spec = match model_id.as_str() {
        crate::ai::control::MODEL_ID_DWPOSE => crate::ai::control::ControlModelSpec::dwpose_spec(),
        crate::ai::control::MODEL_ID_DEPTH_ANYTHING_V2 => {
            crate::ai::control::ControlModelSpec::depth_anything_v2_spec()
        }
        crate::ai::control::MODEL_ID_BIREFNET => {
            crate::ai::control::ControlModelSpec::birefnet_spec()
        }
        _ => {
            return Err(AppError::invalid_input(format!(
                "Unknown control model id '{}'",
                model_id
            )))
        }
    };

    let pkg = spec.create_package_from_file(PathBuf::from(&file_path), version.as_deref(), true)?;
    registry.register_package(pkg.clone())?;
    registry.activate_version(&model_id, &pkg.version)?;

    Ok(pkg)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateVideoIpcRequest {
    pub job_id: String,
    pub source_video_path: String,
    pub character_reference_paths: Vec<String>,
    pub positive_prompt: String,
    pub negative_prompt: String,
    pub style_preset: String,
    pub steps: u32,
    pub cfg_scale: f32,
    pub denoise_strength: f32,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub context_size: usize,
    pub overlap: usize,
}

#[command]
pub fn generate_video_pipeline(
    request: GenerateVideoIpcRequest,
) -> Result<crate::ai::generative::GenerativeVideoReport, AppError> {
    let storage_paths = StoragePaths::default_paths();
    let video_path = PathBuf::from(&request.source_video_path);

    let char_ref = crate::ai::generative::CharacterReference {
        image_paths: request
            .character_reference_paths
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        identity_weight: 0.85,
        appearance_weight: 0.75,
        crop_mode: "FACE_AND_UPPER_BODY".to_string(),
    };

    let env = crate::ai::generative::EnvironmentCondition {
        positive_prompt: request.positive_prompt,
        negative_prompt: request.negative_prompt,
        style_preset: request.style_preset,
    };

    let params = crate::ai::generative::GenerationParams {
        steps: request.steps,
        cfg_scale: request.cfg_scale,
        denoise_strength: request.denoise_strength,
        seed: request.seed,
        width: request.width,
        height: request.height,
        control_weights: std::collections::HashMap::new(),
    };

    let temporal_config = crate::ai::generative::TemporalConfig {
        context_size: request.context_size,
        overlap: request.overlap,
        enable_seam_blending: true,
        enable_latent_continuity: true,
    };

    let out_dir = storage_paths
        .projects_dir
        .join("exports")
        .join("generative_videos");
    let _ = std::fs::create_dir_all(&out_dir);
    let output_video_path = out_dir.join(format!("{}.mp4", request.job_id));

    let script_path = resolve_sidecar_script_path();
    let backend = crate::ai::generative::PythonSidecarBackend::new(
        PathBuf::from("python"),
        script_path,
        storage_paths.temp_dir.clone(),
        false,
    );

    let job_config = crate::ai::generative::GenerativeVideoJobConfig {
        job_id: request.job_id,
        source_video_path: video_path,
        character_reference: char_ref,
        environment: env,
        params,
        temporal_config,
        output_video_path,
    };

    crate::ai::generative::GenerativeVideoPipeline::execute_pipeline(
        &job_config,
        &backend,
        &storage_paths.temp_dir,
        None,
        |_, _, _| {},
    )
}

// =============================================================================
// Cloud AI Generation Commands (Phase Cloud MVP)
// =============================================================================

#[command]
pub fn get_cloud_cost_estimate(
    request: crate::ai::cloud::CloudJobRequest,
) -> Result<crate::ai::cloud::CostEstimate, String> {
    let task_class = crate::ai::cloud::TaskClass::from_str_strict(&request.task_type)
        .map_err(|e| format!("{}", e))?;
    let registry = crate::ai::cloud::ProviderRegistry::new();
    let decision = crate::ai::cloud::GenerationRouter::route_with_registry(
        task_class,
        crate::ai::cloud::RoutingPreference::CostSaving,
        &request,
        None,
        &registry,
    );
    Ok(decision.estimated_cost)
}

#[command]
pub fn get_generation_route(
    task: crate::ai::cloud::GenerationTask,
    mode: crate::ai::cloud::UserExecutionMode,
    request: crate::ai::cloud::CloudJobRequest,
) -> Result<crate::ai::cloud::RoutingDecision, String> {
    let registry = crate::ai::cloud::ProviderRegistry::new();
    Ok(crate::ai::cloud::GenerationRouter::route_with_registry(
        task, mode, &request, None, &registry,
    ))
}

#[command]
pub async fn start_cloud_generation(
    request: crate::ai::cloud::CloudJobRequest,
    max_cost: Option<f64>,
    lifecycle: tauri::State<'_, Arc<crate::ai::cloud::CloudJobLifecycleService>>,
) -> Result<crate::ai::cloud::CloudJobStatus, String> {
    let service = lifecycle.inner().clone();

    let job = service
        .start_cloud_generation(request, max_cost)
        .await
        .map_err(|e| format!("{}", e))?;

    Ok(job.to_legacy_status())
}

#[command]
pub async fn get_cloud_job_status(
    job_id: String,
    project_id: Option<String>,
    remote_id: Option<String>,
    lifecycle: tauri::State<'_, Arc<crate::ai::cloud::CloudJobLifecycleService>>,
) -> Result<crate::ai::cloud::CloudJobStatus, String> {
    let service = lifecycle.inner().clone();

    if let Some(pid) = &project_id {
        let job = service
            .get_job_status(pid, &job_id)
            .map_err(|e| format!("{}", e))?;
        return Ok(job.to_legacy_status());
    }

    // Search active jobs in store if project_id was not explicitly specified
    let active = service
        .store()
        .list_all_active_jobs()
        .map_err(|e| format!("{}", e))?;
    if let Some(found) = active.into_iter().find(|j| {
        j.job_id == job_id
            || j.internal_job_id == job_id
            || (remote_id.is_some() && j.remote_job_id == remote_id)
    }) {
        return Ok(found.to_legacy_status());
    }

    Err(format!(
        "JOB_NOT_FOUND: Job {} could not be found in persistent storage",
        job_id
    ))
}

#[command]
pub async fn cancel_cloud_generation(
    job_id: Option<String>,
    project_id: Option<String>,
    remote_id: Option<String>,
    lifecycle: tauri::State<'_, Arc<crate::ai::cloud::CloudJobLifecycleService>>,
) -> Result<(), String> {
    let service = lifecycle.inner().clone();

    if let (Some(pid), Some(jid)) = (&project_id, &job_id) {
        service
            .cancel_cloud_generation(pid, jid)
            .await
            .map_err(|e| format!("{}", e))?;
        return Ok(());
    }

    // Look up job in active store if project_id was omitted
    let active = service
        .store()
        .list_all_active_jobs()
        .map_err(|e| format!("{}", e))?;
    let target_job = active.into_iter().find(|j| {
        (job_id.as_deref().is_some()
            && (j.job_id == *job_id.as_deref().unwrap()
                || j.internal_job_id == *job_id.as_deref().unwrap()))
            || (remote_id.as_deref().is_some()
                && j.remote_job_id.as_deref() == remote_id.as_deref())
    });

    if let Some(job) = target_job {
        service
            .cancel_cloud_generation(&job.project_id, &job.internal_job_id)
            .await
            .map_err(|e| format!("{}", e))?;
        return Ok(());
    }

    Err("JOB_NOT_FOUND: Cannot cancel unknown cloud job".to_string())
}

#[command]
pub fn preflight_cloud_transformation(
    request: crate::ai::cloud::CloudJobRequest,
    max_cost: Option<f64>,
) -> Result<crate::ai::cloud::CloudSubmissionPreflight, String> {
    let registry = crate::ai::cloud::ProviderRegistry::new();
    let eval = crate::ai::cloud::evaluate_cloud_submission_preflight(&request, max_cost, &registry)
        .map_err(|e| format!("{}", e))?;
    Ok(crate::ai::cloud::CloudSubmissionPreflight {
        task_class: eval.task_class,
        routing_decision: eval.routing_decision,
        source_facts: eval.source_facts,
        budget_limit: eval.budget_limit,
        budget_approved: eval.budget_approved,
        submittable: eval.submittable,
        blocking_code: eval.blocking_code,
    })
}

#[command]
pub async fn start_cloud_transformation(
    request: crate::ai::cloud::CloudJobRequest,
    max_cost: Option<f64>,
    lifecycle: tauri::State<'_, Arc<crate::ai::cloud::CloudJobLifecycleService>>,
) -> Result<crate::ai::cloud::CloudJobEventPayload, String> {
    let service = lifecycle.inner().clone();
    let job = service
        .start_cloud_generation(request, max_cost)
        .await
        .map_err(|e| format!("{}", e))?;
    Ok(job.to_event_payload())
}

#[command]
pub fn list_cloud_jobs(
    project_id: String,
    lifecycle: tauri::State<'_, Arc<crate::ai::cloud::CloudJobLifecycleService>>,
) -> Result<Vec<crate::ai::cloud::CloudJobEventPayload>, String> {
    let store = lifecycle.store();
    let jobs = store
        .list_jobs_in_project(&project_id)
        .map_err(|e| format!("{}", e))?;
    Ok(jobs.into_iter().map(|j| j.to_event_payload()).collect())
}

pub fn resolve_project_media_by_id(
    project_id: &str,
    media_id: Option<&str>,
    storage_paths: &StoragePaths,
) -> Result<(PathBuf, crate::projects::SourceMedia), String> {
    crate::ai::cloud::validate_identifier(project_id, "projectId").map_err(|e| format!("{}", e))?;

    let manager = ProjectManager::new(storage_paths.clone());
    let project = manager
        .get_project(project_id)
        .map_err(|e| format!("{}", e))?;

    let target_media = match media_id {
        Some(mid) if !mid.trim().is_empty() => {
            if let Some(ref sm) = project.source_media {
                if sm.media_id == mid {
                    sm.clone()
                } else if let Some(derived) = project
                    .derived_media_assets
                    .iter()
                    .find(|d| d.media.media_id == mid)
                {
                    derived.media.clone()
                } else {
                    return Err(format!(
                        "MEDIA_NOT_FOUND: Media ID '{}' not found in project '{}'",
                        mid, project_id
                    ));
                }
            } else if let Some(derived) = project
                .derived_media_assets
                .iter()
                .find(|d| d.media.media_id == mid)
            {
                derived.media.clone()
            } else {
                return Err(format!(
                    "MEDIA_NOT_FOUND: Media ID '{}' not found in project '{}'",
                    mid, project_id
                ));
            }
        }
        _ => {
            if let Some(ref ed) = project.editor_state {
                if let Some(ref active_id) = ed.active_media_id {
                    if let Some(ref sm) = project.source_media {
                        if sm.media_id == *active_id {
                            sm.clone()
                        } else if let Some(derived) = project
                            .derived_media_assets
                            .iter()
                            .find(|d| d.media.media_id == *active_id)
                        {
                            derived.media.clone()
                        } else {
                            sm.clone()
                        }
                    } else if let Some(derived) = project
                        .derived_media_assets
                        .iter()
                        .find(|d| d.media.media_id == *active_id)
                    {
                        derived.media.clone()
                    } else {
                        return Err("PROJECT_HAS_NO_MEDIA: Project does not have media".to_string());
                    }
                } else if let Some(ref sm) = project.source_media {
                    sm.clone()
                } else {
                    return Err(
                        "PROJECT_HAS_NO_SOURCE_MEDIA: Project does not have imported source media"
                            .to_string(),
                    );
                }
            } else if let Some(ref sm) = project.source_media {
                sm.clone()
            } else {
                return Err(
                    "PROJECT_HAS_NO_SOURCE_MEDIA: Project does not have imported source media"
                        .to_string(),
                );
            }
        }
    };

    let raw_path = PathBuf::from(&target_media.source_path);
    let candidate = if raw_path.is_file() {
        raw_path
    } else {
        let fallback = storage_paths
            .projects_dir
            .join(project_id)
            .join("media")
            .join(&target_media.original_file_name);
        if fallback.is_file() {
            fallback
        } else {
            let derived_fallback = storage_paths
                .projects_dir
                .join(project_id)
                .join("media")
                .join("derived")
                .join(&target_media.original_file_name);
            if derived_fallback.is_file() {
                derived_fallback
            } else {
                return Err(format!(
                    "SOURCE_FILE_NOT_FOUND: Media file could not be found at {}",
                    target_media.source_path.display()
                ));
            }
        }
    };

    let canonical_file = candidate
        .canonicalize()
        .map_err(|e| format!("CANONICALIZE_FAILED: {}", e))?;
    let media_root = storage_paths.projects_dir.join(project_id).join("media");
    let canonical_media_root = media_root
        .canonicalize()
        .map_err(|e| format!("PROJECT_MEDIA_DIR_NOT_FOUND: {}", e))?;

    if !canonical_file.starts_with(&canonical_media_root) {
        return Err(
            "SECURITY_VIOLATION: Media file is outside project media directory".to_string(),
        );
    }
    if !canonical_file.is_file() {
        return Err("INVALID_TARGET: Media path is not a regular file".to_string());
    }

    Ok((canonical_file, target_media))
}

pub fn resolve_project_source_preview_path(
    project_id: &str,
    storage_paths: &StoragePaths,
) -> Result<(PathBuf, crate::projects::SourceMedia), String> {
    resolve_project_media_by_id(project_id, None, storage_paths)
}

#[command]
pub fn authorize_project_media_preview(
    app: AppHandle,
    project_id: String,
    media_id: Option<String>,
) -> Result<String, String> {
    let storage_paths = StoragePaths::default_paths();
    let (canonical_file, _) =
        resolve_project_media_by_id(&project_id, media_id.as_deref(), &storage_paths)?;

    let _ = app.asset_protocol_scope().allow_file(&canonical_file);
    Ok(canonical_file.to_string_lossy().to_string())
}

pub fn resolve_cloud_artifact_preview_path(
    project_id: &str,
    internal_job_id: &str,
    store: &crate::ai::cloud::PersistentCloudJobStore,
) -> Result<(PathBuf, crate::ai::cloud::PersistentCloudJob), String> {
    crate::ai::cloud::validate_identifier(project_id, "projectId").map_err(|e| format!("{}", e))?;
    crate::ai::cloud::validate_identifier(internal_job_id, "internalJobId")
        .map_err(|e| format!("{}", e))?;

    let job = store
        .load_job(project_id, internal_job_id)
        .map_err(|e| format!("{}", e))?;

    if job.state != crate::ai::cloud::CloudJobState::Completed {
        return Err(format!(
            "PREVIEW_NOT_ELIGIBLE: Cloud job {} is in state {:?}, completed state required for preview",
            internal_job_id, job.state
        ));
    }

    let final_path = job
        .output
        .final_path
        .as_ref()
        .ok_or_else(|| "NO_FINAL_PATH: Job output has no final artifact path".to_string())?;

    if !final_path.is_file() {
        return Err(format!(
            "ARTIFACT_NOT_FOUND: Artifact file does not exist at {}",
            final_path.display()
        ));
    }

    let canonical_file = final_path
        .canonicalize()
        .map_err(|e| format!("CANONICALIZE_FAILED: {}", e))?;
    let artifacts_root = store
        .project_artifacts_dir(project_id)
        .map_err(|e| format!("{}", e))?;
    let canonical_artifacts_root = artifacts_root
        .canonicalize()
        .map_err(|e| format!("ARTIFACTS_ROOT_NOT_FOUND: {}", e))?;

    if !canonical_file.starts_with(&canonical_artifacts_root) {
        return Err(
            "SECURITY_VIOLATION: Artifact file is outside project cloud artifacts directory"
                .to_string(),
        );
    }

    Ok((canonical_file, job))
}

#[command]
pub fn authorize_preview_asset(
    project_id: String,
    asset_kind: crate::ai::cloud::PreviewAssetKind,
    internal_job_id: Option<String>,
    app: AppHandle,
    lifecycle: tauri::State<'_, Arc<crate::ai::cloud::CloudJobLifecycleService>>,
) -> Result<crate::ai::cloud::AuthorizedAssetPreview, String> {
    match asset_kind {
        crate::ai::cloud::PreviewAssetKind::ProjectSource => {
            let (canonical_file, source_media) =
                resolve_project_source_preview_path(&project_id, &StoragePaths::default_paths())?;

            let _ = app.asset_protocol_scope().allow_file(&canonical_file);

            Ok(crate::ai::cloud::AuthorizedAssetPreview {
                local_path: canonical_file.to_string_lossy().to_string(),
                container: source_media.container,
                video_codec: source_media.video_codec,
                alpha_validated: false,
                audio_required: false,
                actual_has_audio: Some(source_media.has_audio),
            })
        }
        crate::ai::cloud::PreviewAssetKind::CloudArtifact => {
            let jid = internal_job_id.ok_or_else(|| {
                "INTERNAL_JOB_ID_REQUIRED: internal_job_id is required for CloudArtifact preview"
                    .to_string()
            })?;

            let (canonical_file, job) =
                resolve_cloud_artifact_preview_path(&project_id, &jid, lifecycle.store())?;

            let _ = app.asset_protocol_scope().allow_file(&canonical_file);

            let (container, video_codec, alpha_val, audio_req) = match &job.artifact_descriptor {
                Some(desc) => (
                    desc.container.extension().to_string(),
                    match desc.video_codec {
                        crate::ai::cloud::ArtifactVideoCodec::H264 => "h264".to_string(),
                        crate::ai::cloud::ArtifactVideoCodec::Vp9 => "vp9".to_string(),
                    },
                    desc.require_alpha && job.state == crate::ai::cloud::CloudJobState::Completed,
                    desc.require_audio,
                ),
                None => ("mp4".to_string(), "h264".to_string(), false, false),
            };

            Ok(crate::ai::cloud::AuthorizedAssetPreview {
                local_path: canonical_file.to_string_lossy().to_string(),
                container,
                video_codec,
                alpha_validated: alpha_val,
                audio_required: audio_req,
                actual_has_audio: None,
            })
        }
    }
}

#[command]
pub fn revoke_preview_asset(
    project_id: String,
    asset_kind: crate::ai::cloud::PreviewAssetKind,
    internal_job_id: Option<String>,
    app: AppHandle,
    lifecycle: tauri::State<'_, Arc<crate::ai::cloud::CloudJobLifecycleService>>,
) -> Result<(), String> {
    match asset_kind {
        crate::ai::cloud::PreviewAssetKind::ProjectSource => {
            if let Ok((canonical_file, _)) =
                resolve_project_source_preview_path(&project_id, &StoragePaths::default_paths())
            {
                let _ = app.asset_protocol_scope().forbid_file(&canonical_file);
            }
        }
        crate::ai::cloud::PreviewAssetKind::CloudArtifact => {
            if let Some(ref jid) = internal_job_id {
                if let Ok((canonical_file, _)) =
                    resolve_cloud_artifact_preview_path(&project_id, jid, lifecycle.store())
                {
                    let _ = app.asset_protocol_scope().forbid_file(&canonical_file);
                }
            }
        }
    }
    Ok(())
}

#[command]
pub fn open_cloud_artifact(
    project_id: String,
    internal_job_id: String,
    lifecycle: tauri::State<'_, Arc<crate::ai::cloud::CloudJobLifecycleService>>,
) -> Result<(), String> {
    crate::ai::cloud::validate_identifier(&project_id, "projectId")
        .map_err(|e| format!("{}", e))?;
    crate::ai::cloud::validate_identifier(&internal_job_id, "internalJobId")
        .map_err(|e| format!("{}", e))?;

    let job = lifecycle
        .store()
        .load_job(&project_id, &internal_job_id)
        .map_err(|e| format!("{}", e))?;

    if job.state != crate::ai::cloud::CloudJobState::Completed {
        return Err(format!(
            "CANNOT_OPEN: Job {} is not in COMPLETED state (state: {:?})",
            internal_job_id, job.state
        ));
    }

    let final_path = job
        .output
        .final_path
        .ok_or_else(|| "NO_FINAL_PATH: Job output has no final artifact path".to_string())?;

    if !final_path.is_file() {
        return Err(format!(
            "FILE_NOT_FOUND: Artifact file not found at {}",
            final_path.display()
        ));
    }

    let canonical = final_path
        .canonicalize()
        .map_err(|e| format!("CANONICALIZE_FAILED: {}", e))?;
    let artifacts_root = lifecycle
        .store()
        .project_cloud_jobs_dir(&project_id)
        .map_err(|e| format!("{}", e))?
        .canonicalize()
        .map_err(|e| format!("ROOT_NOT_FOUND: {}", e))?;

    if !canonical.starts_with(&artifacts_root) {
        return Err("SECURITY_VIOLATION: Path outside project cloud directory".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let _ = StdCommand::new("explorer").arg(&canonical).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = StdCommand::new("open").arg(&canonical).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = StdCommand::new("xdg-open").arg(&canonical).spawn();
    }

    Ok(())
}

#[command]
pub fn open_cloud_artifact_folder(
    project_id: String,
    internal_job_id: String,
    lifecycle: tauri::State<'_, Arc<crate::ai::cloud::CloudJobLifecycleService>>,
) -> Result<(), String> {
    crate::ai::cloud::validate_identifier(&project_id, "projectId")
        .map_err(|e| format!("{}", e))?;
    crate::ai::cloud::validate_identifier(&internal_job_id, "internalJobId")
        .map_err(|e| format!("{}", e))?;

    let artifacts_root = lifecycle
        .store()
        .project_artifacts_dir(&project_id)
        .map_err(|e| format!("{}", e))?;

    if !artifacts_root.exists() {
        let _ = fs::create_dir_all(&artifacts_root);
    }

    let canonical = artifacts_root
        .canonicalize()
        .map_err(|e| format!("CANONICALIZE_FAILED: {}", e))?;

    #[cfg(target_os = "windows")]
    {
        let _ = StdCommand::new("explorer").arg(&canonical).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = StdCommand::new("open").arg(&canonical).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = StdCommand::new("xdg-open").arg(&canonical).spawn();
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// Phase 19 Segmented Cloud Generation Commands
// -----------------------------------------------------------------------------

pub fn resolve_segmented_cloud_artifact_preview_path(
    project_id: &str,
    parent_id: &str,
    store: &crate::ai::cloud::SegmentedCloudJobStore,
) -> Result<(PathBuf, crate::ai::cloud::SegmentedCloudJobManifest), String> {
    crate::ai::cloud::validate_identifier(project_id, "projectId").map_err(|e| format!("{}", e))?;
    crate::ai::cloud::validate_identifier(parent_id, "parentId").map_err(|e| format!("{}", e))?;

    let manifest = store
        .load_manifest(project_id, parent_id)
        .map_err(|e| format!("{}", e))?;

    if manifest.state != crate::ai::cloud::SegmentedJobState::Completed {
        return Err(format!(
            "PREVIEW_NOT_ELIGIBLE: Segmented cloud job {} is in state {:?}, completed state required for preview",
            parent_id, manifest.state
        ));
    }

    let final_output = manifest
        .final_output
        .as_ref()
        .ok_or_else(|| "NO_FINAL_OUTPUT: Manifest has no final output record".to_string())?;

    let final_path = final_output
        .final_path
        .as_ref()
        .ok_or_else(|| "NO_FINAL_PATH: Manifest output has no final artifact path".to_string())?;

    if !final_path.is_file() {
        return Err(format!(
            "ARTIFACT_NOT_FOUND: Artifact file does not exist at {}",
            final_path.display()
        ));
    }

    let canonical_file = final_path
        .canonicalize()
        .map_err(|e| format!("CANONICALIZE_FAILED: {}", e))?;
    let artifacts_root = store
        .storage_paths
        .projects_dir
        .join(project_id)
        .join("cloud-jobs")
        .join("artifacts");
    let canonical_artifacts_root = artifacts_root
        .canonicalize()
        .map_err(|e| format!("ARTIFACTS_ROOT_NOT_FOUND: {}", e))?;

    if !canonical_file.starts_with(&canonical_artifacts_root) {
        return Err(
            "SECURITY_VIOLATION: Artifact file is outside project cloud artifacts directory"
                .to_string(),
        );
    }

    Ok((canonical_file, manifest))
}

#[command]
pub fn preflight_segmented_cloud_transformation(
    request: crate::ai::cloud::CloudJobRequest,
    max_cost: Option<f64>,
    orchestrator: tauri::State<'_, Arc<crate::ai::cloud::SegmentedCloudJobOrchestrator>>,
) -> Result<crate::ai::cloud::SegmentedCloudSubmissionPreflight, String> {
    orchestrator
        .preflight_segmented_transformation(&request, max_cost)
        .map_err(|e| format!("{}", e))
}

#[command]
pub async fn start_segmented_cloud_transformation(
    request: crate::ai::cloud::CloudJobRequest,
    max_cost: Option<f64>,
    orchestrator: tauri::State<'_, Arc<crate::ai::cloud::SegmentedCloudJobOrchestrator>>,
) -> Result<crate::ai::cloud::SegmentedCloudJobSnapshot, String> {
    orchestrator
        .start_segmented_transformation(request, max_cost)
        .await
        .map_err(|e| format!("{}", e))
}

#[command]
pub fn list_segmented_cloud_jobs(
    project_id: String,
    orchestrator: tauri::State<'_, Arc<crate::ai::cloud::SegmentedCloudJobOrchestrator>>,
) -> Result<Vec<crate::ai::cloud::SegmentedCloudJobSnapshot>, String> {
    crate::ai::cloud::validate_identifier(&project_id, "projectId")
        .map_err(|e| format!("{}", e))?;
    orchestrator
        .list_segmented_jobs_in_project(&project_id)
        .map_err(|e| format!("{}", e))
}

#[command]
pub async fn cancel_segmented_cloud_job(
    project_id: String,
    parent_id: String,
    orchestrator: tauri::State<'_, Arc<crate::ai::cloud::SegmentedCloudJobOrchestrator>>,
) -> Result<crate::ai::cloud::SegmentedCloudJobSnapshot, String> {
    crate::ai::cloud::validate_identifier(&project_id, "projectId")
        .map_err(|e| format!("{}", e))?;
    crate::ai::cloud::validate_identifier(&parent_id, "parentId").map_err(|e| format!("{}", e))?;
    orchestrator
        .cancel_segmented_transformation(&project_id, &parent_id)
        .await
        .map_err(|e| format!("{}", e))
}

#[command]
pub async fn approve_segmented_cloud_budget(
    project_id: String,
    parent_id: String,
    max_cost: f64,
    orchestrator: tauri::State<'_, Arc<crate::ai::cloud::SegmentedCloudJobOrchestrator>>,
) -> Result<crate::ai::cloud::SegmentedCloudJobSnapshot, String> {
    crate::ai::cloud::validate_identifier(&project_id, "projectId")
        .map_err(|e| format!("{}", e))?;
    crate::ai::cloud::validate_identifier(&parent_id, "parentId").map_err(|e| format!("{}", e))?;
    orchestrator
        .approve_segmented_budget(&project_id, &parent_id, max_cost)
        .await
        .map_err(|e| format!("{}", e))
}

#[command]
pub fn authorize_segmented_preview_asset(
    project_id: String,
    parent_id: String,
    app: AppHandle,
    store: tauri::State<'_, Arc<crate::ai::cloud::SegmentedCloudJobStore>>,
) -> Result<crate::ai::cloud::AuthorizedAssetPreview, String> {
    let (canonical_file, manifest) =
        resolve_segmented_cloud_artifact_preview_path(&project_id, &parent_id, &store)?;

    let _ = app.asset_protocol_scope().allow_file(&canonical_file);

    Ok(crate::ai::cloud::AuthorizedAssetPreview {
        local_path: canonical_file.to_string_lossy().to_string(),
        container: "webm".to_string(),
        video_codec: "vp9".to_string(),
        alpha_validated: manifest.state == crate::ai::cloud::SegmentedJobState::Completed,
        audio_required: manifest.source_facts.has_audio,
        actual_has_audio: Some(manifest.source_facts.has_audio),
    })
}

#[command]
pub fn revoke_segmented_preview_asset(
    project_id: String,
    parent_id: String,
    app: AppHandle,
    store: tauri::State<'_, Arc<crate::ai::cloud::SegmentedCloudJobStore>>,
) -> Result<(), String> {
    if let Ok((canonical_file, _)) =
        resolve_segmented_cloud_artifact_preview_path(&project_id, &parent_id, &store)
    {
        let _ = app.asset_protocol_scope().forbid_file(&canonical_file);
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Flow Subsystem IPC Commands
// -----------------------------------------------------------------------------

#[command]
pub async fn optimize_prompt(
    request: crate::ai::flow::OptimizePromptRequest,
    gemini_mgr: tauri::State<'_, Arc<crate::ai::flow::GeminiCredentialManager>>,
) -> Result<crate::ai::flow::OptimizePromptResponse, String> {
    let optimizer = crate::ai::flow::GeminiPromptOptimizer::new(gemini_mgr.secret_store().clone());
    optimizer.optimize_prompt(request).await
}

#[command]
pub fn get_gemini_status(
    gemini_mgr: tauri::State<'_, Arc<crate::ai::flow::GeminiCredentialManager>>,
) -> crate::ai::flow::GeminiCredentialStatus {
    gemini_mgr.get_status()
}

#[command]
pub fn set_gemini_api_key(
    key: String,
    gemini_mgr: tauri::State<'_, Arc<crate::ai::flow::GeminiCredentialManager>>,
) -> Result<(), String> {
    gemini_mgr.set_key(&key)
}

#[command]
pub fn clear_gemini_api_key(
    gemini_mgr: tauri::State<'_, Arc<crate::ai::flow::GeminiCredentialManager>>,
) -> Result<(), String> {
    gemini_mgr.clear_key()
}

#[command]
pub async fn test_gemini_api_key(
    gemini_mgr: tauri::State<'_, Arc<crate::ai::flow::GeminiCredentialManager>>,
) -> Result<crate::ai::flow::GeminiCredentialStatus, String> {
    gemini_mgr.test_api_key().await
}

#[command]
pub fn list_flow_profiles(
    session_mgr: tauri::State<'_, Arc<crate::ai::flow::FlowBrowserSessionManager>>,
) -> Vec<crate::ai::flow::FlowProfileSnapshot> {
    let manager = crate::ai::flow::FlowProfileManager::new(
        crate::system::StoragePaths::default_paths().app_data_dir,
    );
    let mut profiles = manager.list_profiles();
    for p in &mut profiles {
        let is_open = session_mgr.is_session_open(&p.profile_id);
        p.manual_browser_open = is_open;
        p.browser_session_open = is_open;
        if is_open {
            p.is_locked = true;
            p.status = "UNKNOWN".to_string();
        }
    }
    profiles
}

#[command]
pub fn create_flow_profile(
    profile_id: String,
    name: String,
    session_mgr: tauri::State<'_, Arc<crate::ai::flow::FlowBrowserSessionManager>>,
) -> Result<crate::ai::flow::FlowProfileSnapshot, String> {
    let manager = crate::ai::flow::FlowProfileManager::new(
        crate::system::StoragePaths::default_paths().app_data_dir,
    );
    let mut snapshot = manager.create_profile(&profile_id, &name)?;
    let is_open = session_mgr.is_session_open(&profile_id);
    snapshot.manual_browser_open = is_open;
    snapshot.browser_session_open = is_open;
    if is_open {
        snapshot.is_locked = true;
    }
    Ok(snapshot)
}

#[command]
pub fn delete_flow_profile(
    profile_id: String,
    session_mgr: tauri::State<'_, Arc<crate::ai::flow::FlowBrowserSessionManager>>,
) -> Result<(), String> {
    if session_mgr.is_session_open(&profile_id) {
        return Err(
            "PROFILE_LOCKED: Cannot delete profile while browser session is active".to_string(),
        );
    }
    let manager = crate::ai::flow::FlowProfileManager::new(
        crate::system::StoragePaths::default_paths().app_data_dir,
    );
    manager.delete_profile(&profile_id, false)
}

#[command]
pub fn open_flow_profile_browser(
    profile_id: String,
    session_mgr: tauri::State<'_, Arc<crate::ai::flow::FlowBrowserSessionManager>>,
) -> Result<String, String> {
    let paths = crate::system::StoragePaths::default_paths();
    let manager = crate::ai::flow::FlowProfileManager::new(paths.app_data_dir.clone());
    let profile_dir = manager.get_profile_dir(&profile_id)?;
    session_mgr.open_session(&profile_id, &profile_dir, &paths)
}

#[command]
pub fn close_flow_profile_browser(
    profile_id: String,
    session_mgr: tauri::State<'_, Arc<crate::ai::flow::FlowBrowserSessionManager>>,
) -> Result<(), String> {
    session_mgr.close_session(&profile_id)
}

#[command]
pub async fn verify_flow_profile_login(
    profile_id: String,
    session_mgr: tauri::State<'_, Arc<crate::ai::flow::FlowBrowserSessionManager>>,
) -> Result<String, String> {
    let paths = crate::system::StoragePaths::default_paths();
    let manager = crate::ai::flow::FlowProfileManager::new(paths.app_data_dir.clone());
    let profile_dir = manager.get_profile_dir(&profile_id)?;
    session_mgr
        .verify_login(&profile_id, &profile_dir, &paths)
        .await
}

#[command]
pub async fn refresh_flow_profile_status(
    profile_id: String,
    session_mgr: tauri::State<'_, Arc<crate::ai::flow::FlowBrowserSessionManager>>,
) -> Result<String, String> {
    verify_flow_profile_login(profile_id, session_mgr).await
}

#[command]
pub async fn preflight_flow_generation(
    request: crate::ai::flow::FlowGenerationRequest,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<crate::ai::flow::FlowGenerationPreflight, String> {
    let paths = crate::system::StoragePaths::default_paths();
    let media_id_opt = if request.source_media_id.trim().is_empty() {
        None
    } else {
        Some(request.source_media_id.as_str())
    };

    let (canonical_source, _) =
        resolve_project_media_by_id(&request.project_id, media_id_opt, &paths)?;

    if !canonical_source.exists() {
        return Err(format!("SOURCE_MEDIA_NOT_FOUND: {:?}", canonical_source));
    }

    flow_service
        .preflight_flow_generation(request, canonical_source)
        .await
}

#[command]
pub async fn start_flow_generation(
    request: crate::ai::flow::FlowGenerationRequest,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<crate::ai::flow::FlowJobSnapshot, String> {
    let paths = crate::system::StoragePaths::default_paths();
    let media_id_opt = if request.source_media_id.trim().is_empty() {
        None
    } else {
        Some(request.source_media_id.as_str())
    };

    let (canonical_source, _) =
        resolve_project_media_by_id(&request.project_id, media_id_opt, &paths)?;

    if !canonical_source.exists() {
        return Err(format!("SOURCE_MEDIA_NOT_FOUND: {:?}", canonical_source));
    }

    flow_service
        .start_flow_generation(request, canonical_source)
        .await
}

#[command]
pub async fn cancel_flow_generation(
    project_id: String,
    parent_id: String,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<crate::ai::flow::FlowJobSnapshot, String> {
    flow_service
        .cancel_flow_generation(&project_id, &parent_id)
        .await
}

#[command]
pub fn get_flow_job_status(
    project_id: String,
    parent_id: String,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<crate::ai::flow::FlowJobSnapshot, String> {
    flow_service.get_flow_job_status(&project_id, &parent_id)
}

#[command]
pub fn list_flow_jobs(
    project_id: String,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<Vec<crate::ai::flow::FlowJobSnapshot>, String> {
    flow_service.list_flow_jobs(&project_id)
}

#[command]
pub async fn refresh_flow_credit_balance(
    profile_id: String,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<crate::ai::flow::FlowProfileCreditStatus, String> {
    flow_service.refresh_flow_credit_balance(&profile_id).await
}

#[command]
pub fn get_flow_model_capabilities(
    profile_id: String,
    operation_context: Option<crate::ai::flow::FlowCapabilityContext>,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<crate::ai::flow::FlowModelCapabilitiesSnapshot, String> {
    let ctx =
        operation_context.unwrap_or(crate::ai::flow::FlowCapabilityContext::UploadedVideoEdit);
    Ok(flow_service.get_flow_model_capabilities(&profile_id, ctx))
}

#[command]
pub fn open_flow_output_artifact(
    project_id: String,
    parent_id: String,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<String, String> {
    let manifest = flow_service
        .orchestrator
        .store()
        .load_manifest(&project_id, &parent_id)?;
    let final_record = manifest.final_output.ok_or_else(|| {
        "ARTIFACT_NOT_READY: Flow generation output artifact has not been created".to_string()
    })?;

    let flow_job_dir = flow_service
        .orchestrator
        .store()
        .parent_flow_job_dir(&project_id, &parent_id)?;
    let canonical_job_dir = flow_job_dir
        .canonicalize()
        .map_err(|e| format!("CANONICALIZE_FAILED: {}", e))?;
    let canonical_target = final_record
        .final_path
        .canonicalize()
        .map_err(|e| format!("OUTPUT_NOT_FOUND: {}", e))?;

    if !canonical_target.starts_with(&canonical_job_dir) {
        return Err(
            "SECURITY_VIOLATION: Output artifact is outside flow job directory".to_string(),
        );
    }

    let target_str = canonical_target.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        let _ = StdCommand::new("explorer").arg(&target_str).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = StdCommand::new("open").arg(&target_str).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = StdCommand::new("xdg-open").arg(&target_str).spawn();
    }

    Ok(target_str)
}

#[command]
pub fn reveal_flow_output_in_folder(
    project_id: String,
    parent_id: String,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<String, String> {
    let manifest = flow_service
        .orchestrator
        .store()
        .load_manifest(&project_id, &parent_id)?;
    let final_record = manifest.final_output.ok_or_else(|| {
        "ARTIFACT_NOT_READY: Flow generation output artifact has not been created".to_string()
    })?;

    let flow_job_dir = flow_service
        .orchestrator
        .store()
        .parent_flow_job_dir(&project_id, &parent_id)?;
    let canonical_job_dir = flow_job_dir
        .canonicalize()
        .map_err(|e| format!("CANONICALIZE_FAILED: {}", e))?;
    let canonical_target = final_record
        .final_path
        .canonicalize()
        .map_err(|e| format!("OUTPUT_NOT_FOUND: {}", e))?;

    if !canonical_target.starts_with(&canonical_job_dir) {
        return Err(
            "SECURITY_VIOLATION: Output artifact is outside flow job directory".to_string(),
        );
    }

    let target_str = canonical_target.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    {
        let _ = StdCommand::new("explorer")
            .arg(format!("/select,{}", target_str))
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = StdCommand::new("open").arg("-R").arg(&target_str).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = StdCommand::new("xdg-open")
            .arg(
                canonical_target
                    .parent()
                    .unwrap_or(&canonical_target)
                    .to_string_lossy()
                    .as_ref(),
            )
            .spawn();
    }

    Ok(target_str)
}

#[command]
pub fn use_flow_output_in_project(
    project_id: String,
    parent_id: String,
    flow_service: tauri::State<'_, Arc<crate::ai::flow::FlowRuntimeService>>,
) -> Result<crate::projects::UseFlowOutputResult, String> {
    flow_service.use_flow_output_in_project(&project_id, &parent_id)
}
