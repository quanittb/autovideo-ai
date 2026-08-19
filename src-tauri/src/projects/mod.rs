use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::error::AppError;
use crate::system::StoragePaths;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectStatus {
    Empty,
    Imported,
    Analyzing,
    Ready,
    Processing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceMedia {
    pub media_id: String,
    pub original_file_name: String,
    pub source_path: PathBuf,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub file_size_bytes: u64,
    pub container: String,
    pub video_codec: String,
    pub audio_codec: Option<String>,
    pub has_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreservationConfig {
    pub preserve_motion: bool,
    pub preserve_camera: bool,
    pub preserve_composition: bool,
    pub preserve_original_audio: bool,
}

impl Default for PreservationConfig {
    fn default() -> Self {
        Self {
            preserve_motion: true,
            preserve_camera: true,
            preserve_composition: true,
            preserve_original_audio: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransformationConfig {
    pub category: String, // "character" (MVP), "background", "environment", "style", "object", "custom"
    pub detected_character: Option<String>,
    pub original_character: Option<String>,
    pub replacement_character: Option<String>,
    pub reference_image_uri: Option<String>,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub preservation: PreservationConfig,
    pub seed: Option<u64>,
}

pub type TransformationRequest = TransformationConfig;

impl Default for TransformationConfig {
    fn default() -> Self {
        Self {
            category: "character".to_string(),
            detected_character: Some("Fox".to_string()),
            original_character: Some("Fox".to_string()),
            replacement_character: Some("White Rabbit".to_string()),
            reference_image_uri: None,
            prompt: "A cute white rabbit wearing a warm knitted scarf".to_string(),
            negative_prompt: None,
            preservation: PreservationConfig::default(),
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
pub struct ProjectOutput {
    pub output_id: String,
    pub file_name: String,
    pub file_path: PathBuf,
    pub file_size_bytes: u64,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEditorState {
    pub current_time: f64,
    pub timeline_zoom: f64,
    pub selected_track: Option<String>,
}

impl Default for ProjectEditorState {
    fn default() -> Self {
        Self {
            current_time: 0.0,
            timeline_zoom: 1.0,
            selected_track: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: ProjectStatus,
    pub source_media: Option<SourceMedia>,
    pub transformation_config: TransformationConfig,
    pub transformation_plan: Option<TransformationPlan>,
    pub outputs: Vec<ProjectOutput>,
    pub editor_state: Option<ProjectEditorState>,
    pub is_fixture: bool,
}

impl Project {
    pub fn new(name: &str) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id: format!("proj-{}", Uuid::new_v4()),
            name: name.trim().to_string(),
            created_at: now.clone(),
            updated_at: now,
            status: ProjectStatus::Empty,
            source_media: None,
            transformation_config: TransformationConfig::default(),
            transformation_plan: None,
            outputs: Vec::new(),
            editor_state: Some(ProjectEditorState::default()),
            is_fixture: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: ProjectStatus,
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
            status: p.status.clone(),
            thumbnail_path: None,
            has_output: !p.outputs.is_empty(),
            is_fixture: p.is_fixture,
        }
    }
}

pub struct ProjectManager {
    storage_paths: StoragePaths,
}

impl ProjectManager {
    pub fn new(storage_paths: StoragePaths) -> Self {
        Self { storage_paths }
    }

    pub fn project_dir(&self, id: &str) -> PathBuf {
        self.storage_paths.projects_dir.join(id)
    }

    pub fn project_manifest_path(&self, id: &str) -> PathBuf {
        self.project_dir(id).join("project.json")
    }

    pub fn create_project(&self, name: &str) -> Result<Project, AppError> {
        let valid_name = if name.trim().is_empty() {
            "Untitled Transformation"
        } else {
            name.trim()
        };

        let project = Project::new(valid_name);
        let proj_dir = self.project_dir(&project.id);

        // Create standard project subdirectories
        fs::create_dir_all(&proj_dir).map_err(|e| {
            AppError::project_create_failed(
                "Failed to create project root directory",
                format!("{}: {}", proj_dir.display(), e),
            )
        })?;

        fs::create_dir_all(proj_dir.join("media")).map_err(|e| {
            AppError::project_create_failed(
                "Failed to create project media directory",
                e.to_string(),
            )
        })?;

        fs::create_dir_all(proj_dir.join("cache")).map_err(|e| {
            AppError::project_create_failed(
                "Failed to create project cache directory",
                e.to_string(),
            )
        })?;

        fs::create_dir_all(proj_dir.join("outputs")).map_err(|e| {
            AppError::project_create_failed(
                "Failed to create project outputs directory",
                e.to_string(),
            )
        })?;

        // Persist project.json
        self.save_project_manifest(&project)?;

        Ok(project)
    }

    pub fn get_project(&self, id: &str) -> Result<Project, AppError> {
        let manifest_path = self.project_manifest_path(id);
        if !manifest_path.exists() {
            return Err(AppError::project_not_found(id));
        }

        let content = fs::read_to_string(&manifest_path).map_err(|e| {
            AppError::project_load_failed(
                "Failed to read project.json file",
                format!("{}: {}", manifest_path.display(), e),
            )
        })?;

        let project: Project = serde_json::from_str(&content).map_err(|e| {
            AppError::project_load_failed(
                "Failed to parse project.json manifest",
                format!("{}: {}", manifest_path.display(), e),
            )
        })?;

        Ok(project)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>, AppError> {
        let projects_dir = &self.storage_paths.projects_dir;
        if !projects_dir.exists() {
            fs::create_dir_all(projects_dir).map_err(|e| {
                AppError::project_load_failed(
                    "Failed to initialize projects root directory",
                    e.to_string(),
                )
            })?;
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(projects_dir).map_err(|e| {
            AppError::project_load_failed(
                "Failed to scan projects directory",
                format!("{}: {}", projects_dir.display(), e),
            )
        })?;

        let mut summaries = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest = path.join("project.json");
                if manifest.exists() {
                    if let Ok(content) = fs::read_to_string(&manifest) {
                        if let Ok(project) = serde_json::from_str::<Project>(&content) {
                            summaries.push(ProjectSummary::from(&project));
                        }
                    }
                }
            }
        }

        // Sort summaries by updated_at descending
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(summaries)
    }

    pub fn update_project(&self, project: &Project) -> Result<Project, AppError> {
        let manifest_path = self.project_manifest_path(&project.id);
        if !manifest_path.exists() {
            return Err(AppError::project_not_found(&project.id));
        }

        let mut updated = project.clone();
        updated.updated_at = Utc::now().to_rfc3339();

        self.save_project_manifest(&updated)?;

        Ok(updated)
    }

    pub fn delete_project(&self, id: &str) -> Result<(), AppError> {
        let proj_dir = self.project_dir(id);
        if !proj_dir.exists() {
            return Err(AppError::project_not_found(id));
        }

        fs::remove_dir_all(&proj_dir).map_err(|e| {
            AppError::project_delete_failed(
                format!("Failed to delete project directory {}", proj_dir.display()),
                e.to_string(),
            )
        })?;

        Ok(())
    }

    fn save_project_manifest(&self, project: &Project) -> Result<(), AppError> {
        let manifest_path = self.project_manifest_path(&project.id);
        let serialized = serde_json::to_string_pretty(project).map_err(|e| {
            AppError::project_save_failed("Failed to serialize project struct", e.to_string())
        })?;

        fs::write(&manifest_path, serialized).map_err(|e| {
            AppError::project_save_failed(
                "Failed to write project.json manifest",
                format!("{}: {}", manifest_path.display(), e),
            )
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_storage() -> (StoragePaths, tempfile::TempDir) {
        let temp = tempdir().expect("Failed to create tempdir");
        let base = temp.path().to_path_buf();
        let paths = StoragePaths {
            app_data_dir: base.clone(),
            projects_dir: base.join("projects"),
            models_dir: base.join("models"),
            cache_dir: base.join("cache"),
            logs_dir: base.join("logs"),
            temp_dir: base.join("temp"),
        };
        (paths, temp)
    }

    #[test]
    fn test_create_and_load_project() {
        let (paths, _temp) = create_test_storage();
        let manager = ProjectManager::new(paths);

        let created = manager
            .create_project("Fox to Rabbit Transformation")
            .expect("Failed to create");
        assert_eq!(created.name, "Fox to Rabbit Transformation");
        assert_eq!(created.schema_version, 1);
        assert_eq!(created.status, ProjectStatus::Empty);

        // Verify directories created on disk
        let proj_dir = manager.project_dir(&created.id);
        assert!(proj_dir.join("media").exists());
        assert!(proj_dir.join("cache").exists());
        assert!(proj_dir.join("outputs").exists());
        assert!(proj_dir.join("project.json").exists());

        // Load project from disk
        let loaded = manager.get_project(&created.id).expect("Failed to load");
        assert_eq!(loaded.id, created.id);
        assert_eq!(loaded.name, created.name);
    }

    #[test]
    fn test_list_and_update_projects() {
        let (paths, _temp) = create_test_storage();
        let manager = ProjectManager::new(paths);

        let p1 = manager
            .create_project("Project Alpha")
            .expect("Create p1 failed");
        let _p2 = manager
            .create_project("Project Beta")
            .expect("Create p2 failed");

        let list = manager.list_projects().expect("List projects failed");
        assert_eq!(list.len(), 2);

        // Update project p1
        let mut to_update = p1.clone();
        to_update.status = ProjectStatus::Imported;
        to_update.transformation_config.prompt = "A majestic silver rabbit".to_string();

        let updated = manager.update_project(&to_update).expect("Update failed");
        assert_eq!(updated.status, ProjectStatus::Imported);
        assert_eq!(
            updated.transformation_config.prompt,
            "A majestic silver rabbit"
        );

        let reloaded = manager.get_project(&p1.id).expect("Reload failed");
        assert_eq!(reloaded.status, ProjectStatus::Imported);
        assert_eq!(
            reloaded.transformation_config.prompt,
            "A majestic silver rabbit"
        );
    }

    #[test]
    fn test_delete_and_missing_project() {
        let (paths, _temp) = create_test_storage();
        let manager = ProjectManager::new(paths);

        let created = manager
            .create_project("Temporary Project")
            .expect("Create failed");
        assert!(manager.project_manifest_path(&created.id).exists());

        manager.delete_project(&created.id).expect("Delete failed");
        assert!(!manager.project_dir(&created.id).exists());

        let err = manager.get_project(&created.id).unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::ProjectNotFound);
    }
}
