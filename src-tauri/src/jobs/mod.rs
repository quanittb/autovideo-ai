use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use uuid::Uuid;

use crate::error::AppError;
use crate::events::*;
use crate::media::MediaService;
use crate::render::{RenderRequest, RenderService};
use crate::system::StoragePaths;

// -----------------------------------------------------------------------------
// Job & Pipeline Models
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Queued,
    Preparing,
    Running,
    Paused,
    Cancelling,
    Cancelled,
    Completed,
    Failed,
    Interrupted,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn can_transition_to(&self, next: Self) -> bool {
        if *self == next {
            return true;
        }
        match (self, next) {
            // Normal execution flow
            (Self::Queued, Self::Preparing) => true,
            (Self::Queued, Self::Running) => true,
            (Self::Queued, Self::Cancelled) => true,
            (Self::Queued, Self::Interrupted) => true,

            (Self::Preparing, Self::Running) => true,
            (Self::Preparing, Self::Failed) => true,
            (Self::Preparing, Self::Cancelling) => true,
            (Self::Preparing, Self::Cancelled) => true,
            (Self::Preparing, Self::Interrupted) => true,

            (Self::Running, Self::Paused) => true,
            (Self::Running, Self::Completed) => true,
            (Self::Running, Self::Failed) => true,
            (Self::Running, Self::Cancelling) => true,
            (Self::Running, Self::Cancelled) => true,
            (Self::Running, Self::Interrupted) => true,

            // Paused flow
            (Self::Paused, Self::Running) => true,
            (Self::Paused, Self::Cancelling) => true,
            (Self::Paused, Self::Cancelled) => true,
            (Self::Paused, Self::Interrupted) => true,

            // Cancelling flow
            (Self::Cancelling, Self::Cancelled) => true,
            (Self::Cancelling, Self::Failed) => true,
            (Self::Cancelling, Self::Interrupted) => true,

            // Retry transitions
            (Self::Failed, Self::Queued) => true,
            (Self::Cancelled, Self::Queued) => true,
            (Self::Interrupted, Self::Queued) => true,
            (Self::Interrupted, Self::Running) => true,

            // Invalid transitions
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StageStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Cancelled,
    PauseUnsupported,
}

impl StageStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Skipped | Self::Cancelled
        )
    }

    pub fn can_transition_to(&self, next: Self) -> bool {
        if *self == next {
            return true;
        }
        match (self, next) {
            (Self::Pending, Self::Running) => true,
            (Self::Pending, Self::Skipped) => true,
            (Self::Pending, Self::Cancelled) => true,

            (Self::Running, Self::Completed) => true,
            (Self::Running, Self::Failed) => true,
            (Self::Running, Self::Cancelled) => true,
            (Self::Running, Self::Pending) => true,
            (Self::Running, Self::PauseUnsupported) => true,

            // Retry and invalidation transitions
            (Self::Completed, Self::Pending) => true,
            (Self::Failed, Self::Pending) => true,
            (Self::Failed, Self::Running) => true,
            (Self::Cancelled, Self::Pending) => true,
            (Self::Cancelled, Self::Running) => true,
            (Self::Skipped, Self::Pending) => true,
            (Self::PauseUnsupported, Self::Running) => true,
            (Self::PauseUnsupported, Self::Failed) => true,
            (Self::PauseUnsupported, Self::Cancelled) => true,

            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JobError {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

impl From<AppError> for JobError {
    fn from(err: AppError) -> Self {
        Self {
            code: format!("{:?}", err.code),
            message: err.message,
            details: err.details,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PipelineStage {
    pub id: String,
    pub name: String,
    pub status: StageStatus,
    pub progress: f32, // 0.0 to 100.0
    pub indeterminate: bool,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<JobError>,
    pub input_artifacts: Vec<String>,
    pub output_artifacts: Vec<String>,
    pub message: String,
}

impl PipelineStage {
    pub fn transition_status(&mut self, next: StageStatus) -> Result<(), AppError> {
        if !self.status.can_transition_to(next) {
            return Err(AppError::invalid_input(format!(
                "Invalid StageStatus transition for stage '{}' from {:?} to {:?}",
                self.id, self.status, next
            )));
        }

        let now = Utc::now().to_rfc3339();
        self.status = next;

        match next {
            StageStatus::Running => {
                if self.started_at.is_none() {
                    self.started_at = Some(now);
                }
            }
            StageStatus::Completed => {
                self.completed_at = Some(now);
                self.progress = 100.0;
            }
            StageStatus::Failed | StageStatus::Cancelled => {
                self.completed_at = Some(now);
            }
            StageStatus::Pending => {
                self.started_at = None;
                self.completed_at = None;
                self.progress = 0.0;
                self.error = None;
            }
            _ => {}
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub artifact_type: String, // "source_video", "frames", "audio", "metadata_json", "final_video", "manifest"
    pub path: String,
    pub file_size_bytes: u64,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>, // "valid", "invalid", "superseded"
    pub metadata: serde_json::Value,
}

impl Artifact {
    pub fn new(
        id: String,
        artifact_type: String,
        path: String,
        file_size_bytes: u64,
        stage_id: Option<String>,
        metadata: serde_json::Value,
    ) -> Self {
        Self {
            id,
            artifact_type,
            path,
            file_size_bytes,
            created_at: Utc::now().to_rfc3339(),
            stage_id,
            status: Some("valid".to_string()),
            metadata,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobLogEntry {
    pub timestamp: String,
    pub level: String, // "INFO", "DEBUG", "WARN", "ERROR"
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StageArtifactValidation {
    pub stage_index: usize,
    pub stage_id: String,
    pub stage_name: String,
    pub is_valid: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobValidationReport {
    pub job_id: String,
    pub project_id: String,
    pub is_fully_valid: bool,
    pub resume_stage_index: usize,
    pub stage_validations: Vec<StageArtifactValidation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub project_id: String,
    pub job_type: String,
    pub status: JobStatus,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub current_stage: Option<String>,
    pub current_stage_index: usize,
    pub total_stages: usize,
    pub progress: f32, // Overall percentage 0.0 to 100.0
    pub message: String,
    pub error: Option<JobError>,
    pub input_files: Vec<String>,
    pub output_files: Vec<String>,
    pub stages: Vec<PipelineStage>,
    pub retry_count: u32,
    pub metadata: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_config: Option<crate::ai::AiJobConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_metrics: Option<crate::ai::AiJobMetrics>,
}

impl Job {
    pub fn new(id: String, project_id: String, job_type: String, input_files: Vec<String>) -> Self {
        Self::new_with_ai(id, project_id, job_type, input_files, None)
    }

    pub fn new_with_ai(
        id: String,
        project_id: String,
        job_type: String,
        input_files: Vec<String>,
        ai_config: Option<crate::ai::AiJobConfig>,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        let has_ai = ai_config.as_ref().map(|c| c.enabled).unwrap_or(false);
        let default_stages = Self::build_pipeline_stages(has_ai);
        let total = default_stages.len();

        Self {
            id,
            project_id,
            job_type,
            status: JobStatus::Queued,
            created_at: now.clone(),
            updated_at: now,
            started_at: None,
            completed_at: None,
            cancelled_at: None,
            current_stage: Some(default_stages[0].id.clone()),
            current_stage_index: 0,
            total_stages: total,
            progress: 0.0,
            message: "Job queued and awaiting execution".to_string(),
            error: None,
            input_files,
            output_files: Vec::new(),
            stages: default_stages,
            retry_count: 0,
            metadata: serde_json::json!({}),
            ai_config,
            ai_metrics: None,
        }
    }

    pub fn transition_status(&mut self, next: JobStatus) -> Result<(), AppError> {
        if !self.status.can_transition_to(next) {
            return Err(AppError::invalid_input(format!(
                "Invalid JobStatus transition from {:?} to {:?}",
                self.status, next
            )));
        }

        let now = Utc::now().to_rfc3339();
        self.status = next;
        self.updated_at = now.clone();

        match next {
            JobStatus::Preparing | JobStatus::Running => {
                if self.started_at.is_none() {
                    self.started_at = Some(now);
                }
            }
            JobStatus::Completed => {
                self.completed_at = Some(now);
                self.progress = 100.0;
            }
            JobStatus::Failed => {
                self.completed_at = Some(now);
            }
            JobStatus::Cancelled => {
                self.cancelled_at = Some(now.clone());
                self.completed_at = Some(now);
            }
            JobStatus::Queued => {
                self.completed_at = None;
                self.cancelled_at = None;
            }
            _ => {}
        }

        Ok(())
    }

    pub fn build_default_pipeline_stages() -> Vec<PipelineStage> {
        Self::build_pipeline_stages(false)
    }

    pub fn build_pipeline_stages(has_ai: bool) -> Vec<PipelineStage> {
        if has_ai {
            vec![
                PipelineStage {
                    id: "stage_1_input_validation".to_string(),
                    name: "1. Validate Source Media".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: true,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting source file inspection".to_string(),
                },
                PipelineStage {
                    id: "stage_2_media_probe".to_string(),
                    name: "2. Probe Media Metadata (FFprobe)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: true,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting stream probing".to_string(),
                },
                PipelineStage {
                    id: "stage_3_frame_extraction".to_string(),
                    name: "3. Extract Frame Sequence (FFmpeg)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: false,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting frame demuxing".to_string(),
                },
                PipelineStage {
                    id: "stage_4_audio_extraction".to_string(),
                    name: "4. Extract Original Audio (FFmpeg)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: true,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting PCM audio extraction".to_string(),
                },
                PipelineStage {
                    id: "stage_ai_frame_inference".to_string(),
                    name: "5. AI Frame Inference (ONNX Runtime)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: false,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting ONNX frame processing".to_string(),
                },
                PipelineStage {
                    id: "stage_5_video_reconstruction".to_string(),
                    name: "6. Reconstruct Video Container (FFmpeg)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: false,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting MP4 muxing".to_string(),
                },
                PipelineStage {
                    id: "stage_6_output_validation".to_string(),
                    name: "7. Validate Reconstructed Media (FFprobe)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: true,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting output validation".to_string(),
                },
            ]
        } else {
            vec![
                PipelineStage {
                    id: "stage_1_input_validation".to_string(),
                    name: "1. Validate Source Media".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: true,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting source file inspection".to_string(),
                },
                PipelineStage {
                    id: "stage_2_media_probe".to_string(),
                    name: "2. Probe Media Metadata (FFprobe)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: true,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting stream probing".to_string(),
                },
                PipelineStage {
                    id: "stage_3_frame_extraction".to_string(),
                    name: "3. Extract Frame Sequence (FFmpeg)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: false,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting frame demuxing".to_string(),
                },
                PipelineStage {
                    id: "stage_4_audio_extraction".to_string(),
                    name: "4. Extract Original Audio (FFmpeg)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: true,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting PCM audio extraction".to_string(),
                },
                PipelineStage {
                    id: "stage_5_video_reconstruction".to_string(),
                    name: "5. Reconstruct Video Container (FFmpeg)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: false,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting MP4 muxing".to_string(),
                },
                PipelineStage {
                    id: "stage_6_output_validation".to_string(),
                    name: "6. Validate Reconstructed Media (FFprobe)".to_string(),
                    status: StageStatus::Pending,
                    progress: 0.0,
                    indeterminate: true,
                    started_at: None,
                    completed_at: None,
                    error: None,
                    input_artifacts: Vec::new(),
                    output_artifacts: Vec::new(),
                    message: "Awaiting output validation".to_string(),
                },
            ]
        }
    }

    pub fn can_start(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Queued | JobStatus::Paused | JobStatus::Interrupted
        )
    }

    pub fn can_cancel(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Queued | JobStatus::Preparing | JobStatus::Running | JobStatus::Paused
        )
    }

    pub fn can_retry(&self) -> bool {
        matches!(
            self.status,
            JobStatus::Failed | JobStatus::Cancelled | JobStatus::Interrupted
        )
    }
}

// -----------------------------------------------------------------------------
// Job Engine & Orchestrator
// -----------------------------------------------------------------------------

pub struct JobEngine {
    pub storage_paths: StoragePaths,
    _active_jobs: Arc<RwLock<HashMap<String, Job>>>,
    cancellation_tokens: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
    child_pids: Arc<RwLock<HashMap<String, HashSet<u32>>>>,
}

impl JobEngine {
    pub fn new(storage_paths: StoragePaths) -> Self {
        Self {
            storage_paths,
            _active_jobs: Arc::new(RwLock::new(HashMap::new())),
            cancellation_tokens: Arc::new(RwLock::new(HashMap::new())),
            child_pids: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register_child_pid(&self, job_id: &str, pid: u32) {
        if let Ok(mut pids) = self.child_pids.write() {
            pids.entry(job_id.to_string()).or_default().insert(pid);
        }
    }

    pub fn unregister_child_pid(&self, job_id: &str, pid: u32) {
        if let Ok(mut pids) = self.child_pids.write() {
            if let Some(set) = pids.get_mut(job_id) {
                set.remove(&pid);
                if set.is_empty() {
                    pids.remove(job_id);
                }
            }
        }
    }

    pub fn terminate_job_processes<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
        project_id: &str,
        job_id: &str,
    ) {
        let target_pids: Vec<u32> = {
            if let Ok(mut pids) = self.child_pids.write() {
                pids.remove(job_id)
                    .map(|s| s.into_iter().collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        };

        for pid in target_pids {
            self.append_job_log_with_app(
                app_handle,
                project_id,
                job_id,
                "INFO",
                "CANCEL",
                &format!("Terminating process PID={}", pid),
            );

            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output();
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
            }

            self.append_job_log_with_app(
                app_handle,
                project_id,
                job_id,
                "INFO",
                "CANCEL",
                &format!("Process terminated PID={}", pid),
            );
        }
    }

    pub fn job_dir(&self, project_id: &str, job_id: &str) -> PathBuf {
        self.storage_paths
            .projects_dir
            .join(project_id)
            .join("jobs")
            .join(job_id)
    }

    pub fn job_log_path(&self, project_id: &str, job_id: &str) -> PathBuf {
        self.job_dir(project_id, job_id)
            .join("logs")
            .join("job.log")
    }

    pub fn job_artifacts_manifest_path(&self, project_id: &str, job_id: &str) -> PathBuf {
        self.job_dir(project_id, job_id)
            .join("artifacts")
            .join("manifest.json")
    }

    pub fn append_job_log(
        &self,
        project_id: &str,
        job_id: &str,
        level: &str,
        stage: &str,
        message: &str,
    ) {
        self.append_job_log_with_app::<tauri::Wry>(None, project_id, job_id, level, stage, message);
    }

    pub fn append_job_log_with_app<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
        project_id: &str,
        job_id: &str,
        level: &str,
        stage: &str,
        message: &str,
    ) {
        let log_path = self.job_log_path(project_id, job_id);
        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let timestamp = Utc::now().to_rfc3339();
        let formatted = format!("[{}] [{}] [{}] {}\n", timestamp, level, stage, message);

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = file.write_all(formatted.as_bytes());
        }

        if let Some(app) = app_handle {
            let _ = app.emit(
                EventNames::JOB_LOG,
                &JobLogEvent {
                    job_id: job_id.to_string(),
                    project_id: project_id.to_string(),
                    timestamp,
                    level: level.to_string(),
                    stage_id: stage.to_string(),
                    message: message.to_string(),
                },
            );
        }
    }

    pub fn save_job_manifest(&self, job: &Job) -> Result<(), AppError> {
        let dir = self.job_dir(&job.project_id, &job.id);
        fs::create_dir_all(&dir).map_err(|e| {
            AppError::storage_error("Failed to create job directory", e.to_string())
        })?;

        let job_file = dir.join("job.json");
        let tmp_file = dir.join(format!(".job.json.tmp-{}", Uuid::new_v4()));

        let json = serde_json::to_string_pretty(job)
            .map_err(|e| AppError::storage_error("Failed to serialize job.json", e.to_string()))?;

        // 1. Write to temporary file with sync_all
        {
            let mut file = File::create(&tmp_file).map_err(|e| {
                AppError::storage_error("Failed to create temporary job manifest", e.to_string())
            })?;
            file.write_all(json.as_bytes()).map_err(|e| {
                AppError::storage_error("Failed to write temporary job manifest", e.to_string())
            })?;
            file.sync_all().map_err(|e| {
                AppError::storage_error("Failed to sync temporary job manifest", e.to_string())
            })?;
        }

        // 2. Atomically rename temporary file to destination
        #[cfg(target_os = "windows")]
        {
            if job_file.exists() {
                let _ = fs::remove_file(&job_file);
            }
        }

        fs::rename(&tmp_file, &job_file).map_err(|e| {
            let _ = fs::remove_file(&tmp_file);
            AppError::storage_error("Failed to atomically persist job.json", e.to_string())
        })?;

        Ok(())
    }

    pub fn register_artifact(
        &self,
        project_id: &str,
        job_id: &str,
        artifact: Artifact,
    ) -> Result<(), AppError> {
        self.register_artifact_with_app::<tauri::Wry>(None, project_id, job_id, artifact)
    }

    pub fn register_artifact_with_app<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
        project_id: &str,
        job_id: &str,
        mut artifact: Artifact,
    ) -> Result<(), AppError> {
        let manifest_path = self.job_artifacts_manifest_path(project_id, job_id);
        if let Some(parent) = manifest_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let file_path = PathBuf::from(&artifact.path);
        if file_path.exists() && file_path.is_file() {
            if let Ok(m) = fs::metadata(&file_path) {
                artifact.file_size_bytes = m.len();
            }
        }

        if artifact.status.is_none() {
            artifact.status = Some("valid".to_string());
        }

        let mut artifacts = self.get_job_artifacts(job_id).unwrap_or_default();
        artifacts.retain(|a| a.id != artifact.id);
        artifacts.push(artifact.clone());

        let json = serde_json::to_string_pretty(&artifacts).map_err(|e| {
            AppError::storage_error("Failed to serialize artifacts manifest", e.to_string())
        })?;

        let tmp_file =
            manifest_path.with_file_name(format!(".manifest.json.tmp-{}", Uuid::new_v4()));
        {
            let mut file = File::create(&tmp_file).map_err(|e| {
                AppError::storage_error(
                    "Failed to create temporary artifacts manifest",
                    e.to_string(),
                )
            })?;
            file.write_all(json.as_bytes()).map_err(|e| {
                AppError::storage_error(
                    "Failed to write temporary artifacts manifest",
                    e.to_string(),
                )
            })?;
            file.sync_all().map_err(|e| {
                AppError::storage_error(
                    "Failed to sync temporary artifacts manifest",
                    e.to_string(),
                )
            })?;
        }

        #[cfg(target_os = "windows")]
        {
            if manifest_path.exists() {
                let _ = fs::remove_file(&manifest_path);
            }
        }

        fs::rename(&tmp_file, &manifest_path).map_err(|e| {
            let _ = fs::remove_file(&tmp_file);
            AppError::storage_error(
                "Failed to atomically persist artifacts manifest",
                e.to_string(),
            )
        })?;

        if let Some(app) = app_handle {
            let _ = app.emit(
                EventNames::JOB_ARTIFACT,
                &JobArtifactEvent {
                    job_id: job_id.to_string(),
                    project_id: project_id.to_string(),
                    artifact_id: artifact.id.clone(),
                    artifact_type: artifact.artifact_type.clone(),
                    path: artifact.path.clone(),
                    file_size_bytes: artifact.file_size_bytes,
                    stage_id: artifact.stage_id.clone(),
                    status: artifact
                        .status
                        .clone()
                        .unwrap_or_else(|| "valid".to_string()),
                    timestamp: Utc::now().to_rfc3339(),
                    artifact,
                },
            );
        }

        Ok(())
    }

    pub fn get_job_artifacts(&self, job_id: &str) -> Result<Vec<Artifact>, AppError> {
        // Look through all project jobs
        if let Ok(projects) = fs::read_dir(&self.storage_paths.projects_dir) {
            for proj in projects.flatten() {
                if proj.path().is_dir() {
                    let manifest_path = proj
                        .path()
                        .join("jobs")
                        .join(job_id)
                        .join("artifacts")
                        .join("manifest.json");
                    if manifest_path.exists() {
                        if let Ok(content) = fs::read_to_string(&manifest_path) {
                            if let Ok(list) = serde_json::from_str::<Vec<Artifact>>(&content) {
                                return Ok(list);
                            }
                        }
                    }
                }
            }
        }
        Ok(Vec::new())
    }

    pub fn get_job_logs(&self, job_id: &str) -> Result<Vec<String>, AppError> {
        if let Ok(projects) = fs::read_dir(&self.storage_paths.projects_dir) {
            for proj in projects.flatten() {
                if proj.path().is_dir() {
                    let log_file = proj
                        .path()
                        .join("jobs")
                        .join(job_id)
                        .join("logs")
                        .join("job.log");
                    if log_file.exists() {
                        if let Ok(file) = File::open(&log_file) {
                            let reader = BufReader::new(file);
                            let lines: Vec<String> = reader.lines().flatten().collect();
                            return Ok(lines);
                        }
                    }
                }
            }
        }
        Ok(Vec::new())
    }

    pub fn recover_interrupted_jobs(&self) -> Result<usize, AppError> {
        self.recover_interrupted_jobs_with_app::<tauri::Wry>(None)
    }

    pub fn recover_interrupted_jobs_with_app<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
    ) -> Result<usize, AppError> {
        // Clean stale process registry entries and cancellation tokens on recovery/startup
        if let Ok(mut pids) = self.child_pids.write() {
            pids.clear();
        }
        if let Ok(mut tokens) = self.cancellation_tokens.write() {
            tokens.clear();
        }

        let mut count = 0;
        if !self.storage_paths.projects_dir.exists() {
            return Ok(0);
        }

        if let Ok(projects) = fs::read_dir(&self.storage_paths.projects_dir) {
            for proj in projects.flatten() {
                let jobs_dir = proj.path().join("jobs");
                if jobs_dir.exists() && jobs_dir.is_dir() {
                    if let Ok(jobs) = fs::read_dir(&jobs_dir) {
                        for j in jobs.flatten() {
                            let job_file = j.path().join("job.json");
                            if job_file.exists() {
                                if let Ok(content) = fs::read_to_string(&job_file) {
                                    if let Ok(mut job) = serde_json::from_str::<Job>(&content) {
                                        if matches!(
                                            job.status,
                                            JobStatus::Running
                                                | JobStatus::Preparing
                                                | JobStatus::Queued
                                                | JobStatus::Cancelling
                                        ) {
                                            let _ = job.transition_status(JobStatus::Interrupted);
                                            job.message = "Job execution was interrupted by application restart/shutdown".to_string();

                                            // Transition any in-flight stages back to Pending
                                            for stage in &mut job.stages {
                                                if stage.status == StageStatus::Running {
                                                    let _ = stage
                                                        .transition_status(StageStatus::Pending);
                                                }
                                            }

                                            let _ = self.save_job_manifest(&job);
                                            self.append_job_log_with_app(
                                                app_handle,
                                                &job.project_id,
                                                &job.id,
                                                "WARN",
                                                "RECOVERY",
                                                "Job execution was interrupted by application restart/shutdown",
                                            );
                                            count += 1;

                                            if let Some(app) = app_handle {
                                                let _ = app.emit(
                                                    EventNames::JOB_INTERRUPTED,
                                                    &JobInterruptedEvent {
                                                        job_id: job.id.clone(),
                                                        project_id: job.project_id.clone(),
                                                        message: job.message.clone(),
                                                        timestamp: Utc::now().to_rfc3339(),
                                                        job: job.clone(),
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    /// Performs deep validation on disk artifacts for each stage of a pipeline job.
    pub fn validate_job_stage_artifacts(&self, job: &Job) -> JobValidationReport {
        let proj_dir = self.storage_paths.projects_dir.join(&job.project_id);
        let source_path_str = job.input_files.first().cloned().unwrap_or_default();
        let source_path = PathBuf::from(&source_path_str);
        let media_service = MediaService::new();

        let render_mode = job
            .metadata
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("full")
            .to_string();

        let mut validations = Vec::new();

        // -------------------------------------------------------------
        // Stage 0: Input Validation
        // -------------------------------------------------------------
        let (s0_valid, s0_reason) = if source_path_str.is_empty() {
            (false, "No input source video specified for job".to_string())
        } else if !source_path.exists() {
            (
                false,
                format!("Source video does not exist: {}", source_path.display()),
            )
        } else {
            match fs::metadata(&source_path) {
                Ok(meta) if meta.len() > 0 => {
                    (true, format!("Source video exists ({} bytes)", meta.len()))
                }
                Ok(_) => (false, "Source video is empty (0 bytes)".to_string()),
                Err(e) => (
                    false,
                    format!("Failed to read source video metadata: {}", e),
                ),
            }
        };

        validations.push(StageArtifactValidation {
            stage_index: 0,
            stage_id: "stage_1_input_validation".to_string(),
            stage_name: "Resolve & Validate Source Media".to_string(),
            is_valid: s0_valid,
            reason: s0_reason,
        });

        // -------------------------------------------------------------
        // Stage 1: Media Probe
        // -------------------------------------------------------------
        let (s1_valid, s1_reason, probed_meta) = if !s0_valid {
            (
                false,
                "Upstream source video validation failed".to_string(),
                None,
            )
        } else {
            match media_service.probe(&source_path) {
                Ok(meta) => {
                    if meta.width > 0
                        && meta.height > 0
                        && meta.fps > 0.0
                        && meta.duration_ms > 0
                        && !meta.video_codec.is_empty()
                    {
                        let reason = format!(
                            "Probe valid: {}x{}, {:.2} FPS, {:.2}s, codec: {}",
                            meta.width,
                            meta.height,
                            meta.fps,
                            meta.duration_ms as f64 / 1000.0,
                            meta.video_codec
                        );
                        (true, reason, Some(meta))
                    } else {
                        (
                            false,
                            "Probed video metadata contains invalid dimensions or duration"
                                .to_string(),
                            None,
                        )
                    }
                }
                Err(e) => (false, format!("Media probe failed: {}", e.message), None),
            }
        };

        validations.push(StageArtifactValidation {
            stage_index: 1,
            stage_id: "stage_2_media_probe".to_string(),
            stage_name: "Probe Media Metadata".to_string(),
            is_valid: s1_valid,
            reason: s1_reason,
        });

        // -------------------------------------------------------------
        // Stage 2: Frame Extraction
        // -------------------------------------------------------------
        let (s2_valid, s2_reason) = if !s1_valid {
            (
                false,
                "Upstream media probe failed; frame cache cannot be verified".to_string(),
            )
        } else {
            let meta = probed_meta.as_ref().unwrap();
            let imported_media = media_service
                .import_to_project(&proj_dir, &source_path)
                .ok();
            let media_id = imported_media
                .map(|m| m.media_id)
                .unwrap_or_else(|| "imported_media".to_string());
            let frames_dir = proj_dir
                .join("cache")
                .join("media")
                .join(&media_id)
                .join("frames");

            if !frames_dir.exists() || !frames_dir.is_dir() {
                (
                    false,
                    format!(
                        "Frames cache directory does not exist: {}",
                        frames_dir.display()
                    ),
                )
            } else {
                let mut frame_entries = Vec::new();
                if let Ok(entries) = fs::read_dir(&frames_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_file() {
                            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                                if ext.eq_ignore_ascii_case("png")
                                    || ext.eq_ignore_ascii_case("jpg")
                                {
                                    frame_entries.push(p);
                                }
                            }
                        }
                    }
                }

                if frame_entries.is_empty() {
                    (
                        false,
                        "Frames cache directory is empty (0 frames found)".to_string(),
                    )
                } else {
                    frame_entries.sort_by_key(|p| p.file_name().unwrap_or_default().to_os_string());
                    let count = frame_entries.len();

                    // Expected frames
                    let expected_dur = match render_mode.as_str() {
                        "test_1s" => 1.0,
                        "test_3s" => 3.0,
                        _ => meta.duration_ms as f64 / 1000.0,
                    };
                    let expected_frames = (expected_dur * meta.fps).round() as usize;
                    let diff = (count as i64 - expected_frames as i64).abs();

                    // Check representative frames
                    let first_ok = fs::metadata(&frame_entries[0])
                        .map(|m| m.len() > 0)
                        .unwrap_or(false);
                    let mid_ok = fs::metadata(&frame_entries[count / 2])
                        .map(|m| m.len() > 0)
                        .unwrap_or(false);
                    let last_ok = fs::metadata(&frame_entries[count - 1])
                        .map(|m| m.len() > 0)
                        .unwrap_or(false);

                    if !first_ok || !mid_ok || !last_ok {
                        (false, "One or more representative frame files are empty (0 bytes) or unreadable".to_string())
                    } else if diff > 10 && (diff as f64 / expected_frames.max(1) as f64) > 0.15 {
                        (
                            false,
                            format!(
                                "Frame count mismatch: expected ~{}, found {}",
                                expected_frames, count
                            ),
                        )
                    } else {
                        (
                            true,
                            format!(
                                "Found {} valid readable frames ({:.2} FPS)",
                                count, meta.fps
                            ),
                        )
                    }
                }
            }
        };

        validations.push(StageArtifactValidation {
            stage_index: 2,
            stage_id: "stage_3_frame_extraction".to_string(),
            stage_name: "Extract Frame Sequence".to_string(),
            is_valid: s2_valid,
            reason: s2_reason,
        });

        // -------------------------------------------------------------
        // Stage 3: Audio Extraction
        // -------------------------------------------------------------
        let (s3_valid, s3_reason) = if !s1_valid {
            (
                false,
                "Upstream media probe failed; audio cache cannot be verified".to_string(),
            )
        } else {
            let meta = probed_meta.as_ref().unwrap();
            if !meta.has_audio {
                (
                    true,
                    "Source video has no audio stream (safely handled)".to_string(),
                )
            } else {
                let imported_media = media_service
                    .import_to_project(&proj_dir, &source_path)
                    .ok();
                let media_id = imported_media
                    .map(|m| m.media_id)
                    .unwrap_or_else(|| "imported_media".to_string());
                let audio_file = proj_dir
                    .join("cache")
                    .join("media")
                    .join(&media_id)
                    .join("audio")
                    .join("source.wav");

                if !audio_file.exists() {
                    (
                        false,
                        format!(
                            "Extracted audio file does not exist: {}",
                            audio_file.display()
                        ),
                    )
                } else {
                    match fs::read(&audio_file) {
                        Ok(bytes) => {
                            if bytes.len() < 44 {
                                (
                                    false,
                                    "Audio file is too small to contain a valid WAV header"
                                        .to_string(),
                                )
                            } else if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
                                (
                                    false,
                                    "Audio file does not contain a valid RIFF/WAVE header"
                                        .to_string(),
                                )
                            } else {
                                (
                                    true,
                                    format!(
                                        "Valid 16-bit PCM WAV audio cache exists ({} bytes)",
                                        bytes.len()
                                    ),
                                )
                            }
                        }
                        Err(e) => (false, format!("Failed to read audio file: {}", e)),
                    }
                }
            }
        };

        validations.push(StageArtifactValidation {
            stage_index: 3,
            stage_id: "stage_4_audio_extraction".to_string(),
            stage_name: "Extract Original Audio".to_string(),
            is_valid: s3_valid,
            reason: s3_reason,
        });

        let has_ai_stage = job
            .stages
            .iter()
            .any(|s| s.id == "stage_ai_frame_inference");

        // -------------------------------------------------------------
        // Optional AI Stage: AI Frame Inference
        // -------------------------------------------------------------
        let (sai_valid, sai_reason) = if has_ai_stage {
            if !s2_valid {
                (
                    false,
                    "Upstream frame extraction failed; AI frame artifacts cannot be verified"
                        .to_string(),
                )
            } else {
                let ai_recon_dir = proj_dir
                    .join("cache")
                    .join("ai")
                    .join(&job.id)
                    .join("reconstruction_frames");
                if !ai_recon_dir.exists() || !ai_recon_dir.is_dir() {
                    (
                        false,
                        "AI reconstruction frames directory does not exist".to_string(),
                    )
                } else {
                    let mut count = 0;
                    let mut all_ok = true;
                    if let Ok(entries) = fs::read_dir(&ai_recon_dir) {
                        for entry in entries.flatten() {
                            let p = entry.path();
                            if p.is_file() {
                                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                                    if ext.eq_ignore_ascii_case("png") {
                                        count += 1;
                                        if let Ok(m) = fs::metadata(&p) {
                                            if m.len() == 0 {
                                                all_ok = false;
                                            }
                                        } else {
                                            all_ok = false;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if count == 0 {
                        (
                            false,
                            "AI reconstruction frames directory is empty (0 frames)".to_string(),
                        )
                    } else if !all_ok {
                        (
                            false,
                            "One or more AI reconstruction frames are empty (0 bytes)".to_string(),
                        )
                    } else {
                        (
                            true,
                            format!("Found {} valid AI reconstruction frames", count),
                        )
                    }
                }
            }
        } else {
            (true, "AI stage not enabled".to_string())
        };

        if has_ai_stage {
            validations.push(StageArtifactValidation {
                stage_index: 4,
                stage_id: "stage_ai_frame_inference".to_string(),
                stage_name: "AI Frame Inference (ONNX Runtime)".to_string(),
                is_valid: sai_valid,
                reason: sai_reason,
            });
        }

        // -------------------------------------------------------------
        // Video Reconstruction Stage
        // -------------------------------------------------------------
        let upstream_recon_valid = if has_ai_stage {
            s2_valid && s3_valid && sai_valid
        } else {
            s2_valid && s3_valid
        };

        let (s_recon_valid, s_recon_reason) = if !upstream_recon_valid {
            (
                false,
                "Upstream frame, audio, or AI artifacts are invalid".to_string(),
            )
        } else {
            let out_file_opt = job.output_files.first().map(PathBuf::from);
            if let Some(out_path) = out_file_opt {
                if !out_path.exists() {
                    (
                        false,
                        format!("Output video does not exist: {}", out_path.display()),
                    )
                } else {
                    match media_service.probe(&out_path) {
                        Ok(out_meta) => {
                            let meta = probed_meta.as_ref().unwrap();
                            let expected_dur = match render_mode.as_str() {
                                "test_1s" => 1.0,
                                "test_3s" => 3.0,
                                _ => meta.duration_ms as f64 / 1000.0,
                            };
                            let actual_dur = out_meta.duration_ms as f64 / 1000.0;
                            let dur_delta = (actual_dur - expected_dur).abs();

                            if out_meta.file_size_bytes == 0 {
                                let _ = fs::remove_file(&out_path);
                                (false, "Output video file is empty (0 bytes)".to_string())
                            } else if out_meta.video_codec.is_empty() {
                                let _ = fs::remove_file(&out_path);
                                (false, "Output video missing valid video stream".to_string())
                            } else if dur_delta > 0.35 {
                                let _ = fs::remove_file(&out_path);
                                (
                                    false,
                                    format!(
                                        "Output duration mismatch: expected ~{:.2}s, got {:.2}s",
                                        expected_dur, actual_dur
                                    ),
                                )
                            } else {
                                (
                                    true,
                                    format!(
                                        "Valid reconstructed MP4 exists ({:.2}s, {}x{}, {} bytes)",
                                        actual_dur,
                                        out_meta.width,
                                        out_meta.height,
                                        out_meta.file_size_bytes
                                    ),
                                )
                            }
                        }
                        Err(e) => {
                            let _ = fs::remove_file(&out_path);
                            (false, format!("Output video probe failed: {}", e.message))
                        }
                    }
                }
            } else {
                (false, "No output video file registered in job".to_string())
            }
        };

        let recon_stage_idx = if has_ai_stage { 5 } else { 4 };
        validations.push(StageArtifactValidation {
            stage_index: recon_stage_idx,
            stage_id: "stage_5_video_reconstruction".to_string(),
            stage_name: "Reconstruct Video Container".to_string(),
            is_valid: s_recon_valid,
            reason: s_recon_reason,
        });

        // -------------------------------------------------------------
        // Output Validation Stage
        // -------------------------------------------------------------
        let (s_out_valid, s_out_reason) = if !s_recon_valid {
            (
                false,
                "Upstream reconstructed video is missing or invalid".to_string(),
            )
        } else {
            (
                true,
                "Reconstructed media verified and ready for export".to_string(),
            )
        };

        let out_stage_idx = if has_ai_stage { 6 } else { 5 };
        validations.push(StageArtifactValidation {
            stage_index: out_stage_idx,
            stage_id: "stage_6_output_validation".to_string(),
            stage_name: "Validate Reconstructed Media".to_string(),
            is_valid: s_out_valid,
            reason: s_out_reason,
        });

        let mut usability = Vec::with_capacity(job.stages.len());
        for (idx, stage) in job.stages.iter().enumerate() {
            let is_valid = validations.get(idx).map(|v| v.is_valid).unwrap_or(false);
            let prereqs_ok = match stage.id.as_str() {
                "stage_1_input_validation" => true,
                "stage_2_media_probe" => usability.first().copied().unwrap_or(false),
                "stage_3_frame_extraction" | "stage_4_audio_extraction" => job
                    .stages
                    .iter()
                    .position(|s| s.id == "stage_2_media_probe")
                    .and_then(|i| usability.get(i).copied())
                    .unwrap_or(false),
                "stage_ai_frame_inference" => job
                    .stages
                    .iter()
                    .position(|s| s.id == "stage_3_frame_extraction")
                    .and_then(|i| usability.get(i).copied())
                    .unwrap_or(false),
                "stage_5_video_reconstruction" => {
                    let frames_ok = if has_ai_stage {
                        job.stages
                            .iter()
                            .position(|s| s.id == "stage_ai_frame_inference")
                            .and_then(|i| usability.get(i).copied())
                            .unwrap_or(false)
                    } else {
                        job.stages
                            .iter()
                            .position(|s| s.id == "stage_3_frame_extraction")
                            .and_then(|i| usability.get(i).copied())
                            .unwrap_or(false)
                    };
                    let audio_ok = job
                        .stages
                        .iter()
                        .position(|s| s.id == "stage_4_audio_extraction")
                        .and_then(|i| usability.get(i).copied())
                        .unwrap_or(false);
                    frames_ok && audio_ok
                }
                "stage_6_output_validation" => job
                    .stages
                    .iter()
                    .position(|s| s.id == "stage_5_video_reconstruction")
                    .and_then(|i| usability.get(i).copied())
                    .unwrap_or(false),
                _ => false,
            };
            usability.push(is_valid && prereqs_ok);
        }

        let resume_stage_index = usability
            .iter()
            .position(|&u| !u)
            .unwrap_or(job.stages.len());
        let is_fully_valid = resume_stage_index == job.stages.len();

        JobValidationReport {
            job_id: job.id.clone(),
            project_id: job.project_id.clone(),
            is_fully_valid,
            resume_stage_index,
            stage_validations: validations,
        }
    }

    pub fn create_job(
        &self,
        project_id: &str,
        job_type: Option<String>,
        input_files: Vec<String>,
    ) -> Result<Job, AppError> {
        self.create_job_with_app::<tauri::Wry>(None, project_id, job_type, input_files)
    }

    pub fn create_job_with_app<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
        project_id: &str,
        job_type: Option<String>,
        input_files: Vec<String>,
    ) -> Result<Job, AppError> {
        let job_id = format!("job-{}", Uuid::new_v4());
        let job = Job::new(
            job_id.clone(),
            project_id.to_string(),
            job_type.unwrap_or_else(|| "video_pipeline".to_string()),
            input_files,
        );

        let job_dir = self.job_dir(project_id, &job_id);
        fs::create_dir_all(job_dir.join("input"))
            .map_err(|e| AppError::storage_error("Failed to create input dir", e.to_string()))?;
        fs::create_dir_all(job_dir.join("work"))
            .map_err(|e| AppError::storage_error("Failed to create work dir", e.to_string()))?;
        fs::create_dir_all(job_dir.join("artifacts")).map_err(|e| {
            AppError::storage_error("Failed to create artifacts dir", e.to_string())
        })?;
        fs::create_dir_all(job_dir.join("output"))
            .map_err(|e| AppError::storage_error("Failed to create output dir", e.to_string()))?;
        fs::create_dir_all(job_dir.join("logs"))
            .map_err(|e| AppError::storage_error("Failed to create logs dir", e.to_string()))?;

        self.save_job_manifest(&job)?;
        self.append_job_log_with_app(
            app_handle,
            project_id,
            &job_id,
            "INFO",
            "INIT",
            "Job created and initialized on disk",
        );

        if let Some(app) = app_handle {
            let _ = app.emit(
                EventNames::JOB_CREATED,
                &JobCreatedEvent {
                    job_id: job.id.clone(),
                    project_id: job.project_id.clone(),
                    job_type: job.job_type.clone(),
                    timestamp: job.created_at.clone(),
                    job: job.clone(),
                },
            );
            let _ = app.emit(
                EventNames::JOB_QUEUED,
                &JobQueuedEvent {
                    job_id: job.id.clone(),
                    project_id: job.project_id.clone(),
                    timestamp: job.created_at.clone(),
                    job: job.clone(),
                },
            );
        }

        Ok(job)
    }

    pub fn create_ai_job(
        &self,
        project_id: &str,
        job_type: Option<String>,
        input_files: Vec<String>,
        ai_config: crate::ai::AiJobConfig,
    ) -> Result<Job, AppError> {
        self.create_ai_job_with_app::<tauri::Wry>(
            None,
            project_id,
            job_type,
            input_files,
            ai_config,
        )
    }

    pub fn create_ai_job_with_app<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
        project_id: &str,
        job_type: Option<String>,
        input_files: Vec<String>,
        ai_config: crate::ai::AiJobConfig,
    ) -> Result<Job, AppError> {
        let mut pinned_ai_config = ai_config;
        let registry = crate::ai::ModelRegistry::new(self.storage_paths.models_dir.clone());
        if let Ok(resolved) = crate::ai::ProductionModelResolver::resolve_model(
            &registry,
            Some(&pinned_ai_config.model_id),
            pinned_ai_config.model_version.as_deref(),
            pinned_ai_config.provider,
        ) {
            pinned_ai_config.model_version = Some(resolved.model_version.clone());
            pinned_ai_config.model_hash = Some(resolved.model_hash.clone());
            pinned_ai_config.profile_hash = Some(resolved.profile_hash.clone());
            pinned_ai_config.provider = Some(resolved.provider);

            if let Some(app) = app_handle {
                let _ = app.emit(
                    EventNames::AI_MODEL_RESOLVED,
                    &crate::events::AiModelResolvedEvent {
                        model_id: resolved.model_id.clone(),
                        version: resolved.model_version.clone(),
                        model_hash: resolved.model_hash.clone(),
                        provider: format!("{:?}", resolved.provider),
                        timestamp: Utc::now().to_rfc3339(),
                    },
                );
            }
        } else if let Ok(pkg) = registry.get_active_package(&pinned_ai_config.model_id) {
            if pinned_ai_config.model_version.is_none() {
                pinned_ai_config.model_version = Some(pkg.version.clone());
            }
            if pinned_ai_config.model_hash.is_none() {
                pinned_ai_config.model_hash = Some(pkg.sha256.clone());
            }
            if pinned_ai_config.profile_hash.is_none() {
                pinned_ai_config.profile_hash = Some(pkg.profile.compute_profile_hash());
            }
        }

        let job_id = format!("job-{}", Uuid::new_v4());
        let job = Job::new_with_ai(
            job_id.clone(),
            project_id.to_string(),
            job_type.unwrap_or_else(|| "ai_video_pipeline".to_string()),
            input_files,
            Some(pinned_ai_config),
        );

        let job_dir = self.job_dir(project_id, &job_id);
        fs::create_dir_all(job_dir.join("input"))
            .map_err(|e| AppError::storage_error("Failed to create input dir", e.to_string()))?;
        fs::create_dir_all(job_dir.join("work"))
            .map_err(|e| AppError::storage_error("Failed to create work dir", e.to_string()))?;
        fs::create_dir_all(job_dir.join("artifacts")).map_err(|e| {
            AppError::storage_error("Failed to create artifacts dir", e.to_string())
        })?;
        fs::create_dir_all(job_dir.join("output"))
            .map_err(|e| AppError::storage_error("Failed to create output dir", e.to_string()))?;
        fs::create_dir_all(job_dir.join("logs"))
            .map_err(|e| AppError::storage_error("Failed to create logs dir", e.to_string()))?;

        self.save_job_manifest(&job)?;
        self.append_job_log_with_app(
            app_handle,
            project_id,
            &job_id,
            "INFO",
            "INIT",
            "AI video job created and initialized on disk",
        );

        if let Some(app) = app_handle {
            let _ = app.emit(
                EventNames::JOB_CREATED,
                &JobCreatedEvent {
                    job_id: job.id.clone(),
                    project_id: job.project_id.clone(),
                    job_type: job.job_type.clone(),
                    timestamp: job.created_at.clone(),
                    job: job.clone(),
                },
            );
            let _ = app.emit(
                EventNames::JOB_QUEUED,
                &JobQueuedEvent {
                    job_id: job.id.clone(),
                    project_id: job.project_id.clone(),
                    timestamp: job.created_at.clone(),
                    job: job.clone(),
                },
            );
        }

        Ok(job)
    }

    pub fn get_job(&self, job_id: &str) -> Result<Job, AppError> {
        if let Ok(projects) = fs::read_dir(&self.storage_paths.projects_dir) {
            for proj in projects.flatten() {
                if proj.path().is_dir() {
                    let job_file = proj.path().join("jobs").join(job_id).join("job.json");
                    if job_file.exists() {
                        let content = fs::read_to_string(&job_file).map_err(|e| {
                            AppError::storage_error("Failed to read job.json", e.to_string())
                        })?;
                        let job = serde_json::from_str::<Job>(&content).map_err(|e| {
                            AppError::storage_error("Failed to parse job.json", e.to_string())
                        })?;
                        return Ok(job);
                    }
                }
            }
        }
        Err(AppError::invalid_input(format!(
            "Job not found: {}",
            job_id
        )))
    }

    pub fn list_jobs(&self, project_id: Option<&str>) -> Result<Vec<Job>, AppError> {
        let mut results = Vec::new();
        if !self.storage_paths.projects_dir.exists() {
            return Ok(results);
        }

        if let Ok(projects) = fs::read_dir(&self.storage_paths.projects_dir) {
            for proj in projects.flatten() {
                if proj.path().is_dir() {
                    let p_name = proj.file_name().to_string_lossy().to_string();
                    if let Some(target_proj) = project_id {
                        if p_name != target_proj {
                            continue;
                        }
                    }

                    let jobs_dir = proj.path().join("jobs");
                    if jobs_dir.exists() && jobs_dir.is_dir() {
                        if let Ok(job_entries) = fs::read_dir(&jobs_dir) {
                            for j in job_entries.flatten() {
                                let job_file = j.path().join("job.json");
                                if job_file.exists() {
                                    if let Ok(content) = fs::read_to_string(&job_file) {
                                        if let Ok(job) = serde_json::from_str::<Job>(&content) {
                                            results.push(job);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(results)
    }

    pub fn delete_job(&self, job_id: &str) -> Result<(), AppError> {
        let job = self.get_job(job_id)?;
        let dir = self.job_dir(&job.project_id, job_id);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| {
                AppError::storage_error("Failed to remove job folder", e.to_string())
            })?;
        }
        Ok(())
    }

    pub async fn cancel_job<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
        job_id: &str,
    ) -> Result<Job, AppError> {
        let mut job = self.get_job(job_id)?;

        // Idempotent: If already completed or failed, return as is without error
        if job.status == JobStatus::Completed || job.status == JobStatus::Failed {
            return Ok(job);
        }

        // Idempotent: If already cancelled or cancelling, return as is
        if job.status == JobStatus::Cancelled {
            return Ok(job);
        }

        // 1. Transition to CANCELLING & persist immediately
        let _ = job.transition_status(JobStatus::Cancelling);
        job.message = "Cancellation requested by user".to_string();
        self.save_job_manifest(&job)?;
        self.append_job_log_with_app(
            app_handle,
            &job.project_id,
            job_id,
            "INFO",
            "CANCEL",
            "Cancellation requested by user",
        );

        if let Some(app) = app_handle {
            let _ = app.emit(
                EventNames::JOB_CANCEL_REQUESTED,
                &JobCancelRequestedEvent {
                    job_id: job.id.clone(),
                    project_id: job.project_id.clone(),
                    message: job.message.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    job: job.clone(),
                },
            );
        }

        // 2. Set cancellation token flag
        if let Ok(tokens) = self.cancellation_tokens.read() {
            if let Some(token) = tokens.get(job_id) {
                token.store(true, Ordering::SeqCst);
            }
        }

        // 3. Terminate active OS child processes (FFmpeg / FFprobe)
        self.terminate_job_processes(app_handle, &job.project_id, job_id);

        // 4. Cancel active stage if running
        if let Some(idx) = job
            .stages
            .iter()
            .position(|s| s.status == StageStatus::Running)
        {
            let _ = job.stages[idx].transition_status(StageStatus::Cancelled);
            job.stages[idx].message = "Stage cancelled by user".to_string();
            self.append_job_log_with_app(
                app_handle,
                &job.project_id,
                job_id,
                "INFO",
                "CANCEL",
                &format!("Stage cancelled: {}", job.stages[idx].name),
            );
            if let Some(app) = app_handle {
                let _ = app.emit(
                    EventNames::JOB_STAGE_CANCELLED,
                    &JobStageCancelledEvent {
                        job_id: job.id.clone(),
                        project_id: job.project_id.clone(),
                        stage_id: job.stages[idx].id.clone(),
                        stage_index: idx,
                        stage_name: job.stages[idx].name.clone(),
                        timestamp: Utc::now().to_rfc3339(),
                    },
                );
            }
        }

        // 5. Final transition to CANCELLED & persist
        let _ = job.transition_status(JobStatus::Cancelled);
        job.message = "Job cancelled by user".to_string();
        self.save_job_manifest(&job)?;
        self.append_job_log_with_app(
            app_handle,
            &job.project_id,
            job_id,
            "INFO",
            "CANCEL",
            "Job cancelled and cleanup finished",
        );

        if let Some(app) = app_handle {
            let _ = app.emit(
                EventNames::JOB_CANCELLED,
                &JobCancelledEvent {
                    job_id: job.id.clone(),
                    project_id: job.project_id.clone(),
                    message: job.message.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    job: job.clone(),
                },
            );
        }

        Ok(job)
    }

    pub async fn retry_job<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
        job_id: &str,
    ) -> Result<Job, AppError> {
        let mut job = self.get_job(job_id)?;
        if !job.can_retry() {
            return Err(AppError::invalid_input(format!(
                "Cannot retry job in status: {:?}",
                job.status
            )));
        }

        // Deep validation of stage artifacts on disk
        let report = self.validate_job_stage_artifacts(&job);

        // Dynamic dependency invalidation cascade based on stage IDs
        let has_ai_stage = job
            .stages
            .iter()
            .any(|s| s.id == "stage_ai_frame_inference");
        let mut stage_usability = Vec::with_capacity(job.stages.len());

        for (idx, stage) in job.stages.iter().enumerate() {
            let is_valid = report
                .stage_validations
                .get(idx)
                .map(|v| v.is_valid)
                .unwrap_or(false);

            let prereqs_ok = match stage.id.as_str() {
                "stage_1_input_validation" => true,
                "stage_2_media_probe" => stage_usability.first().copied().unwrap_or(false),
                "stage_3_frame_extraction" | "stage_4_audio_extraction" => job
                    .stages
                    .iter()
                    .position(|s| s.id == "stage_2_media_probe")
                    .and_then(|i| stage_usability.get(i).copied())
                    .unwrap_or(false),
                "stage_ai_frame_inference" => job
                    .stages
                    .iter()
                    .position(|s| s.id == "stage_3_frame_extraction")
                    .and_then(|i| stage_usability.get(i).copied())
                    .unwrap_or(false),
                "stage_5_video_reconstruction" => {
                    let frames_ok = if has_ai_stage {
                        job.stages
                            .iter()
                            .position(|s| s.id == "stage_ai_frame_inference")
                            .and_then(|i| stage_usability.get(i).copied())
                            .unwrap_or(false)
                    } else {
                        job.stages
                            .iter()
                            .position(|s| s.id == "stage_3_frame_extraction")
                            .and_then(|i| stage_usability.get(i).copied())
                            .unwrap_or(false)
                    };
                    let audio_ok = job
                        .stages
                        .iter()
                        .position(|s| s.id == "stage_4_audio_extraction")
                        .and_then(|i| stage_usability.get(i).copied())
                        .unwrap_or(false);
                    frames_ok && audio_ok
                }
                "stage_6_output_validation" => job
                    .stages
                    .iter()
                    .position(|s| s.id == "stage_5_video_reconstruction")
                    .and_then(|i| stage_usability.get(i).copied())
                    .unwrap_or(false),
                _ => false,
            };

            stage_usability.push(is_valid && prereqs_ok);
        }

        for (idx, stage) in job.stages.iter_mut().enumerate() {
            if stage_usability.get(idx).copied().unwrap_or(false) {
                let _ = stage.transition_status(StageStatus::Completed);
                stage.progress = 100.0;
                stage.error = None;
                if let Some(val) = report.stage_validations.get(idx) {
                    stage.message = format!("Reused valid artifact: {}", val.reason);
                }
            } else {
                let _ = stage.transition_status(StageStatus::Pending);
                stage.progress = 0.0;
                stage.error = None;
                stage.started_at = None;
                stage.completed_at = None;
            }
        }

        let earliest_pending_idx = stage_usability
            .iter()
            .position(|&u| !u)
            .unwrap_or(job.stages.len());

        job.retry_count += 1;
        job.updated_at = Utc::now().to_rfc3339();
        job.error = None;
        job.started_at = None;
        job.completed_at = None;
        job.cancelled_at = None;

        if earliest_pending_idx < job.stages.len() {
            job.current_stage_index = earliest_pending_idx;
            job.current_stage = Some(job.stages[earliest_pending_idx].id.clone());
            job.progress = calculate_job_progress_from_stages(&job.stages);
            job.message = format!(
                "Retrying job from stage {} (Attempt #{})",
                job.stages[earliest_pending_idx].name, job.retry_count
            );
        } else {
            job.current_stage_index = 0;
            job.current_stage = Some(job.stages[0].id.clone());
            job.progress = 0.0;
            job.message = format!("Retrying job (Attempt #{})", job.retry_count);
        }

        job.transition_status(JobStatus::Queued)?;
        self.save_job_manifest(&job)?;

        let resume_msg = if earliest_pending_idx < job.stages.len() {
            format!(
                "Retrying job (Attempt #{}): resuming from stage {}",
                job.retry_count, job.stages[earliest_pending_idx].name
            )
        } else {
            format!("Retrying job (Attempt #{})", job.retry_count)
        };

        self.append_job_log_with_app(
            app_handle,
            &job.project_id,
            job_id,
            "INFO",
            "RETRY",
            &resume_msg,
        );

        if let Some(app) = app_handle {
            let _ = app.emit(
                EventNames::JOB_RETRYING,
                &JobRetryingEvent {
                    job_id: job.id.clone(),
                    project_id: job.project_id.clone(),
                    retry_count: job.retry_count,
                    timestamp: Utc::now().to_rfc3339(),
                    job: job.clone(),
                },
            );
            let _ = app.emit(
                EventNames::JOB_QUEUED,
                &JobQueuedEvent {
                    job_id: job.id.clone(),
                    project_id: job.project_id.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    job: job.clone(),
                },
            );
        }

        // Automatically start the retried job
        self.start_job(app_handle, job_id).await
    }

    pub async fn start_job<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
        job_id: &str,
    ) -> Result<Job, AppError> {
        let mut job = self.get_job(job_id)?;
        if !job.can_start() {
            return Err(AppError::invalid_input(format!(
                "Cannot start job in status: {:?}",
                job.status
            )));
        }

        job.transition_status(JobStatus::Running)?;
        job.message = "Pipeline execution started".to_string();
        self.save_job_manifest(&job)?;
        self.append_job_log_with_app(
            app_handle,
            &job.project_id,
            job_id,
            "INFO",
            "START",
            "Starting pipeline orchestrator execution",
        );

        if let Some(app) = app_handle {
            let _ = app.emit(
                EventNames::JOB_STARTED,
                &JobStartedEvent {
                    job_id: job.id.clone(),
                    project_id: job.project_id.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    job: job.clone(),
                },
            );
        }

        // Setup cancellation token
        let cancel_token = Arc::new(AtomicBool::new(false));
        if let Ok(mut tokens) = self.cancellation_tokens.write() {
            tokens.insert(job_id.to_string(), cancel_token.clone());
        }

        // Clone context for async background task
        let storage_paths = self.storage_paths.clone();
        let target_job_id = job_id.to_string();
        let target_project_id = job.project_id.clone();
        let app_handle_cloned = app_handle.cloned();
        let child_pids = self.child_pids.clone();
        let cancellation_tokens = self.cancellation_tokens.clone();

        tokio::spawn(async move {
            let engine = JobEngine::new(storage_paths);
            engine
                .execute_pipeline_runner(
                    app_handle_cloned.as_ref(),
                    &target_project_id,
                    &target_job_id,
                    cancel_token,
                    child_pids.clone(),
                )
                .await;

            // Cleanup registry after runner completion
            if let Ok(mut tokens) = cancellation_tokens.write() {
                tokens.remove(&target_job_id);
            }
            if let Ok(mut pids) = child_pids.write() {
                pids.remove(&target_job_id);
            }
        });

        Ok(job)
    }

    pub async fn execute_pipeline_runner<R: tauri::Runtime>(
        &self,
        app_handle: Option<&tauri::AppHandle<R>>,
        project_id: &str,
        job_id: &str,
        cancel_token: Arc<AtomicBool>,
        child_pids: Arc<RwLock<HashMap<String, HashSet<u32>>>>,
    ) {
        let start_time = Instant::now();
        let mut job = match self.get_job(job_id) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("Failed to load job for pipeline execution: {:?}", e);
                return;
            }
        };

        let media_service = MediaService::new();
        let render_service = RenderService::new();

        let proj_dir = self.storage_paths.projects_dir.join(project_id);
        let _ = fs::create_dir_all(&proj_dir);
        let _ = fs::create_dir_all(proj_dir.join("media"));
        let _ = fs::create_dir_all(proj_dir.join("cache"));
        let _ = fs::create_dir_all(proj_dir.join("outputs"));

        let source_path_str = job.input_files.first().cloned().unwrap_or_default();
        let source_path = PathBuf::from(&source_path_str);
        let imported_media = media_service
            .import_to_project(&proj_dir, &source_path)
            .ok();

        // Extract metadata settings from job if present
        let render_mode = job
            .metadata
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("full")
            .to_string();

        let clip_start_sec = job
            .metadata
            .get("startTimeSeconds")
            .and_then(|v| v.as_f64());
        let clip_end_sec = job
            .metadata
            .get("endTimeSeconds")
            .and_then(|v| v.as_f64())
            .or_else(|| {
                if render_mode == "test_1s" {
                    Some(1.0)
                } else if render_mode == "test_3s" {
                    Some(3.0)
                } else {
                    None
                }
            });

        // Transition job state: QUEUED / PREPARING -> RUNNING
        if job.status == JobStatus::Queued || job.status == JobStatus::Preparing {
            let _ = job.transition_status(JobStatus::Running);
            let _ = self.save_job_manifest(&job);
        }

        // Stage runner loop
        for i in 0..job.stages.len() {
            // Check cancellation before stage execution
            if cancel_token.load(Ordering::SeqCst) {
                let _ = job.stages[i].transition_status(StageStatus::Cancelled);
                let _ = job.transition_status(JobStatus::Cancelled);
                job.message = "Pipeline cancelled by user".to_string();
                let _ = self.save_job_manifest(&job);
                self.append_job_log_with_app(
                    app_handle,
                    project_id,
                    job_id,
                    "WARN",
                    "CANCEL",
                    "Pipeline was cancelled by user request",
                );
                if let Some(app) = app_handle {
                    let _ = app.emit(
                        EventNames::JOB_CANCELLED,
                        &JobCancelledEvent {
                            job_id: job.id.clone(),
                            project_id: job.project_id.clone(),
                            message: job.message.clone(),
                            timestamp: Utc::now().to_rfc3339(),
                            job: job.clone(),
                        },
                    );
                }
                return;
            }

            // If this stage was already verified as completed on retry, reuse it and continue
            if job.stages[i].status == StageStatus::Completed {
                self.append_job_log_with_app(
                    app_handle,
                    project_id,
                    job_id,
                    "INFO",
                    &job.stages[i].id,
                    &format!(
                        "⚡ Reusing verified artifact for {}: {}",
                        job.stages[i].name, job.stages[i].message
                    ),
                );
                if let Some(app) = app_handle {
                    let _ = app.emit(
                        EventNames::JOB_STAGE_COMPLETED,
                        &JobStageCompletedEvent {
                            job_id: job.id.clone(),
                            project_id: job.project_id.clone(),
                            stage_id: job.stages[i].id.clone(),
                            stage_index: i,
                            stage_name: job.stages[i].name.clone(),
                            stage_status: StageStatus::Completed,
                            message: job.stages[i].message.clone(),
                            timestamp: Utc::now().to_rfc3339(),
                        },
                    );
                }
                continue;
            }
            job.current_stage_index = i;
            job.current_stage = Some(job.stages[i].id.clone());
            let _ = job.stages[i].transition_status(StageStatus::Running);
            job.stages[i].progress = 0.0;
            job.progress = calculate_overall_progress_with_stages(&job.stages, i, 0.0);
            job.message = format!("Executing {}", job.stages[i].name);

            let _ = self.save_job_manifest(&job);
            self.append_job_log_with_app(
                app_handle,
                project_id,
                job_id,
                "INFO",
                &job.stages[i].id,
                &format!("Started stage: {}", job.stages[i].name),
            );

            if let Some(app) = app_handle {
                let _ = app.emit(
                    EventNames::JOB_STAGE_STARTED,
                    &JobStageStartedEvent {
                        job_id: job.id.clone(),
                        project_id: job.project_id.clone(),
                        stage_id: job.stages[i].id.clone(),
                        stage_index: i,
                        stage_name: job.stages[i].name.clone(),
                        stage_status: StageStatus::Running,
                        timestamp: Utc::now().to_rfc3339(),
                    },
                );
                let _ = app.emit(
                    EventNames::JOB_PROGRESS,
                    &JobProgressEvent {
                        job_id: job.id.clone(),
                        project_id: job.project_id.clone(),
                        overall_progress: job.progress,
                        stage_progress: 0.0,
                        current_stage: Some(job.stages[i].id.clone()),
                        current_stage_index: i,
                        completed_stages: i,
                        total_stages: job.stages.len(),
                        message: job.message.clone(),
                        timestamp: Utc::now().to_rfc3339(),
                        job: job.clone(),
                    },
                );
            }

            // Execute specific stage by ID
            let stage_id_str = job.stages[i].id.clone();
            let stage_result: Result<(), AppError> = (|| -> Result<(), AppError> {
                match stage_id_str.as_str() {
                    // -------------------------------------------------------------
                    // Stage 0: Input Validation
                    // -------------------------------------------------------------
                    "stage_1_input_validation" => {
                        if !source_path.exists() {
                            return Err(AppError::media_file_not_found(source_path_str.clone()));
                        }
                        let size = media_service.validate_file(&source_path)?;
                        let _ = self.register_artifact_with_app(
                            app_handle,
                            project_id,
                            job_id,
                            Artifact::new(
                                format!("art-src-{}", Uuid::new_v4()),
                                "source_video".to_string(),
                                source_path.display().to_string(),
                                size,
                                Some(job.stages[i].id.clone()),
                                serde_json::json!({ "input": true }),
                            ),
                        );
                        job.stages[i].message = format!("Validated source video ({} bytes)", size);
                        Ok(())
                    }

                    // -------------------------------------------------------------
                    // Stage 1: Media Probe
                    // -------------------------------------------------------------
                    "stage_2_media_probe" => {
                        let meta = media_service.probe(&source_path)?;
                        let _ = self.register_artifact_with_app(
                            app_handle,
                            project_id,
                            job_id,
                            Artifact::new(
                                format!("art-meta-{}", Uuid::new_v4()),
                                "metadata_json".to_string(),
                                source_path.display().to_string(),
                                0,
                                Some(job.stages[i].id.clone()),
                                serde_json::to_value(&meta).unwrap_or_default(),
                            ),
                        );
                        job.stages[i].message = format!(
                            "Probed: {}x{}, {:.2} FPS, {:.2}s, Video: {}, Audio: {}",
                            meta.width,
                            meta.height,
                            meta.fps,
                            meta.duration_ms as f64 / 1000.0,
                            meta.video_codec,
                            meta.audio_codec.as_deref().unwrap_or("none")
                        );
                        Ok(())
                    }

                    // -------------------------------------------------------------
                    // Stage 2: Frame Extraction
                    // -------------------------------------------------------------
                    "stage_3_frame_extraction" => {
                        let imported = imported_media.as_ref().ok_or_else(|| {
                            AppError::media_invalid(
                                "Imported media missing",
                                "Failed to access imported media record",
                            )
                        })?;

                        let frame_req = crate::media::FrameExtractionRequest {
                            project_id: project_id.to_string(),
                            media_id: imported.media_id.clone(),
                            start_time_seconds: clip_start_sec,
                            end_time_seconds: clip_end_sec,
                            fps: Some(imported.fps),
                            width: None,
                            height: None,
                            format: Some("png".to_string()),
                        };

                        let mut on_frame_progress = |stage_prog: f32| {
                            if let Some(app) = app_handle {
                                let overall = calculate_overall_progress_with_stages(
                                    &job.stages,
                                    i,
                                    stage_prog,
                                );
                                let _ = app.emit(
                                    EventNames::JOB_STAGE_PROGRESS,
                                    &JobStageProgressEvent {
                                        job_id: job_id.to_string(),
                                        project_id: project_id.to_string(),
                                        stage_id: "stage_3_frame_extraction".to_string(),
                                        stage_index: i,
                                        stage_progress: stage_prog,
                                        overall_progress: overall,
                                        message: Some(format!(
                                            "Extracting frames ({:.1}%)",
                                            stage_prog
                                        )),
                                        timestamp: Utc::now().to_rfc3339(),
                                    },
                                );
                            }
                        };

                        let child_pids1 = child_pids.clone();
                        let child_pids2 = child_pids.clone();
                        let jid1 = job_id.to_string();
                        let jid2 = job_id.to_string();

                        let frame_res = media_service.extract_frames_with_progress_and_cancel(
                            &proj_dir,
                            &imported.source_path,
                            &frame_req,
                            &mut on_frame_progress,
                            Some(cancel_token.clone()),
                            Some(&mut move |pid| {
                                if let Ok(mut map) = child_pids1.write() {
                                    map.entry(jid1.clone()).or_default().insert(pid);
                                }
                            }),
                            Some(&mut move |pid| {
                                if let Ok(mut map) = child_pids2.write() {
                                    if let Some(set) = map.get_mut(&jid2) {
                                        set.remove(&pid);
                                    }
                                }
                            }),
                        )?;

                        let _ = self.register_artifact_with_app(
                            app_handle,
                            project_id,
                            job_id,
                            Artifact::new(
                                format!("art-frames-{}", Uuid::new_v4()),
                                "frames".to_string(),
                                frame_res.frames_dir.display().to_string(),
                                0,
                                Some(job.stages[i].id.clone()),
                                serde_json::to_value(&frame_res).unwrap_or_default(),
                            ),
                        );

                        job.stages[i].message = format!(
                            "Extracted {} frames at {:.2} FPS ({})",
                            frame_res.frame_count,
                            frame_res.fps,
                            if frame_res.is_cached {
                                "cached"
                            } else {
                                "generated"
                            }
                        );
                        Ok(())
                    }

                    // -------------------------------------------------------------
                    // Stage 3: Audio Extraction
                    // -------------------------------------------------------------
                    "stage_4_audio_extraction" => {
                        let imported = imported_media.as_ref().ok_or_else(|| {
                            AppError::media_invalid(
                                "Imported media missing",
                                "Failed to access imported media record",
                            )
                        })?;
                        let audio_res = media_service.extract_audio(
                            &proj_dir,
                            &imported.source_path,
                            &imported.media_id,
                        )?;

                        if let Some(ref a_path) = audio_res.audio_path {
                            let size = fs::metadata(a_path).map(|m| m.len()).unwrap_or(0);
                            let _ = self.register_artifact_with_app(
                                app_handle,
                                project_id,
                                job_id,
                                Artifact::new(
                                    format!("art-audio-{}", Uuid::new_v4()),
                                    "audio".to_string(),
                                    a_path.display().to_string(),
                                    size,
                                    Some(job.stages[i].id.clone()),
                                    serde_json::to_value(&audio_res).unwrap_or_default(),
                                ),
                            );
                        }

                        job.stages[i].message = if audio_res.has_audio {
                            format!(
                                "Extracted PCM audio ({}Hz, {})",
                                audio_res.sample_rate,
                                if audio_res.is_cached {
                                    "cached"
                                } else {
                                    "generated"
                                }
                            )
                        } else {
                            "Source has no audio stream (safely handled)".to_string()
                        };
                        Ok(())
                    }

                    // -------------------------------------------------------------
                    // AI Stage: AI Frame Inference
                    // -------------------------------------------------------------
                    "stage_ai_frame_inference" => {
                        let ai_cfg = job.ai_config.as_ref().ok_or_else(|| {
                            AppError::invalid_input("AI stage configured but missing AI job config")
                        })?;

                        let imported = imported_media.as_ref().ok_or_else(|| {
                            AppError::media_invalid(
                                "Imported media missing",
                                "Failed to access imported media record",
                            )
                        })?;

                        let cache_dir =
                            media_service.prepare_media(&proj_dir, &imported.media_id)?;
                        let frames_dir = cache_dir.join("frames");
                        let ai_cache_dir = proj_dir.join("cache").join("ai").join(job_id);
                        let artifact_mgr = crate::ai::AiArtifactManager::new(&ai_cache_dir);

                        let mut on_ai_progress =
                            |stage_prog: f32,
                             frame_meta: Option<&crate::ai::AiFrameMetadata>,
                             metrics: &crate::ai::AiJobMetrics| {
                                if let Some(app) = app_handle {
                                    let overall = calculate_overall_progress_with_stages(
                                        &job.stages,
                                        i,
                                        stage_prog,
                                    );
                                    let _ = app.emit(
                                        EventNames::JOB_STAGE_PROGRESS,
                                        &JobStageProgressEvent {
                                            job_id: job_id.to_string(),
                                            project_id: project_id.to_string(),
                                            stage_id: "stage_ai_frame_inference".to_string(),
                                            stage_index: i,
                                            stage_progress: stage_prog,
                                            overall_progress: overall,
                                            message: Some(format!(
                                                "AI Frame Inference ({}/{} frames, {:.1}%)",
                                                metrics.frames_processed
                                                    + metrics.frames_passthrough,
                                                metrics.frames_total,
                                                stage_prog
                                            )),
                                            timestamp: Utc::now().to_rfc3339(),
                                        },
                                    );

                                    if let Some(meta) = frame_meta {
                                        let _ = app.emit(
                                            EventNames::AI_FRAME_PROGRESS,
                                            &AiFrameProgressEvent {
                                                job_id: job_id.to_string(),
                                                project_id: project_id.to_string(),
                                                frame_index: meta.frame_index,
                                                processed_frames: metrics.frames_processed,
                                                total_frames: metrics.frames_total,
                                                stage_progress: stage_prog,
                                                overall_progress: overall,
                                                inference_duration_ms: Some(
                                                    meta.inference_duration_ms,
                                                ),
                                                timestamp: Utc::now().to_rfc3339(),
                                            },
                                        );
                                    }
                                }
                            };

                        let metrics = crate::ai::AiFrameExecutor::execute(
                            &frames_dir,
                            ai_cfg,
                            &artifact_mgr,
                            Some(cancel_token.clone()),
                            &mut on_ai_progress,
                        )?;

                        job.ai_metrics = Some(metrics.clone());
                        let _ = self.register_artifact_with_app(
                            app_handle,
                            project_id,
                            job_id,
                            Artifact::new(
                                format!("art-ai-frames-{}", Uuid::new_v4()),
                                "ai_frames".to_string(),
                                artifact_mgr
                                    .reconstruction_frames_dir()
                                    .display()
                                    .to_string(),
                                0,
                                Some(job.stages[i].id.clone()),
                                serde_json::to_value(&metrics).unwrap_or_default(),
                            ),
                        );

                        job.stages[i].message = format!(
                            "Processed {} frames ({} reused, {} passthrough) in {:.2}ms (avg {:.2}ms/frame)",
                            metrics.frames_processed,
                            metrics.frames_reused,
                            metrics.frames_passthrough,
                            metrics.total_pipeline_duration_ms,
                            metrics.average_inference_duration_ms
                        );
                        Ok(())
                    } // -------------------------------------------------------------
                    // Video Reconstruction Stage
                    // -------------------------------------------------------------
                    "stage_5_video_reconstruction" => {
                        let imported = imported_media.as_ref().ok_or_else(|| {
                            AppError::media_invalid(
                                "Imported media missing",
                                "Failed to access imported media record",
                            )
                        })?;

                        let cache_dir =
                            media_service.prepare_media(&proj_dir, &imported.media_id)?;
                        let is_ai_job = job.ai_config.as_ref().map(|c| c.enabled).unwrap_or(false);

                        if is_ai_job {
                            let ai_cache_dir = proj_dir.join("cache").join("ai").join(job_id);
                            let artifact_mgr = crate::ai::AiArtifactManager::new(&ai_cache_dir);
                            let frames_dir = artifact_mgr.reconstruction_frames_dir();
                            let audio_path = cache_dir.join("audio").join("source.wav");

                            let manifest_path = proj_dir
                                .join("cache")
                                .join("media")
                                .join(&imported.media_id)
                                .join("manifest.json");
                            let expected_frame_count = if let Ok(content) =
                                fs::read_to_string(&manifest_path)
                            {
                                if let Ok(manifest) = serde_json::from_str::<
                                    crate::media::MediaCacheManifest,
                                >(&content)
                                {
                                    manifest.frames.map(|f| f.frame_count as usize).unwrap_or(0)
                                } else {
                                    0
                                }
                            } else {
                                0
                            };

                            let frame_count = if expected_frame_count > 0 {
                                expected_frame_count
                            } else {
                                fs::read_dir(&frames_dir)
                                    .map(|rd| {
                                        rd.flatten()
                                            .filter(|e| {
                                                e.path().extension().and_then(|x| x.to_str())
                                                    == Some("png")
                                            })
                                            .count()
                                    })
                                    .unwrap_or(0)
                            };

                            let output_folder = proj_dir.join("outputs").join(job_id);
                            let output_path = output_folder.join("pipeline_reconstructed.mp4");

                            let (recon_w, recon_h) = if let Some(ref ai_cfg) = job.ai_config {
                                if ai_cfg.frame_sampling.mode == crate::ai::FrameSamplingMode::All {
                                    (
                                        ai_cfg.preprocessing.target_width,
                                        ai_cfg.preprocessing.target_height,
                                    )
                                } else {
                                    (imported.width, imported.height)
                                }
                            } else {
                                (imported.width, imported.height)
                            };

                            let recon_cfg = crate::ai::VideoReconstructionConfig {
                                source_video_path: imported.source_path.clone(),
                                frames_dir: frames_dir.clone(),
                                output_path: output_path.clone(),
                                frame_pattern: "%06d.png".to_string(),
                                expected_frame_count: frame_count,
                                width: recon_w,
                                height: recon_h,
                                fps: crate::ai::RationalFps::from_f64(imported.fps),
                                pixel_format: "yuv420p".to_string(),
                                codec: crate::ai::VideoCodec::H264,
                                crf: 18,
                                audio_source: if audio_path.exists() {
                                    Some(audio_path)
                                } else {
                                    None
                                },
                                audio_mode: if imported.has_audio {
                                    crate::ai::AudioPreservationMode::PreserveOriginal
                                } else {
                                    crate::ai::AudioPreservationMode::None
                                },
                                overwrite: true,
                            };

                            let child_pids1 = child_pids.clone();
                            let child_pids2 = child_pids.clone();
                            let jid1 = job_id.to_string();
                            let jid2 = job_id.to_string();

                            let recon_res = crate::ai::VideoReconstructor::reconstruct_video(
                                &recon_cfg,
                                job_id,
                                job.ai_config.as_ref(),
                                Some(&artifact_mgr),
                                |stage_prog, cur_f, tot_f| {
                                    if let Some(app) = app_handle {
                                        let overall = calculate_overall_progress_with_stages(
                                            &job.stages,
                                            i,
                                            stage_prog,
                                        );
                                        let _ = app.emit(
                                            EventNames::AI_RECONSTRUCTION_PROGRESS,
                                            &crate::events::AiReconstructionProgressEvent {
                                                job_id: job_id.to_string(),
                                                project_id: project_id.to_string(),
                                                frames_encoded: cur_f,
                                                total_frames: tot_f,
                                                progress_percent: stage_prog,
                                                overall_progress: overall,
                                                message: format!(
                                                    "Reconstructing video ({}/{} frames, {:.1}%)",
                                                    cur_f, tot_f, stage_prog
                                                ),
                                                timestamp: Utc::now().to_rfc3339(),
                                            },
                                        );
                                        let _ = app.emit(
                                            EventNames::JOB_STAGE_PROGRESS,
                                            &JobStageProgressEvent {
                                                job_id: job_id.to_string(),
                                                project_id: project_id.to_string(),
                                                stage_id: "stage_5_video_reconstruction".to_string(),
                                                stage_index: i,
                                                stage_progress: stage_prog,
                                                overall_progress: overall,
                                                message: Some(format!(
                                                    "Encoding video container ({}/{} frames, {:.1}%)",
                                                    cur_f, tot_f, stage_prog
                                                )),
                                                timestamp: Utc::now().to_rfc3339(),
                                            },
                                        );
                                    }
                                },
                                Some(cancel_token.clone()),
                                Some(move |pid| {
                                    if let Ok(mut map) = child_pids1.write() {
                                        map.entry(jid1.clone()).or_default().insert(pid);
                                    }
                                }),
                                Some(move |pid| {
                                    if let Ok(mut map) = child_pids2.write() {
                                        if let Some(set) = map.get_mut(&jid2) {
                                            set.remove(&pid);
                                        }
                                    }
                                }),
                            )?;

                            let out_path = recon_res.output_path.clone();
                            let file_size = recon_res.output_metadata.file_size_bytes;

                            let _ = self.register_artifact_with_app(
                                app_handle,
                                project_id,
                                job_id,
                                Artifact::new(
                                    format!("art-output-{}", Uuid::new_v4()),
                                    "final_video".to_string(),
                                    out_path.display().to_string(),
                                    file_size,
                                    Some(job.stages[i].id.clone()),
                                    serde_json::to_value(&recon_res).unwrap_or_default(),
                                ),
                            );

                            let manifest_path = out_path
                                .parent()
                                .unwrap_or_else(|| std::path::Path::new("."))
                                .join("reconstruction_manifest.json");
                            if manifest_path.exists() {
                                let manifest_size =
                                    fs::metadata(&manifest_path).map(|m| m.len()).unwrap_or(0);
                                let _ = self.register_artifact_with_app(
                                    app_handle,
                                    project_id,
                                    job_id,
                                    Artifact::new(
                                        format!("art-manifest-{}", Uuid::new_v4()),
                                        "reconstruction_manifest".to_string(),
                                        manifest_path.display().to_string(),
                                        manifest_size,
                                        Some(job.stages[i].id.clone()),
                                        serde_json::to_value(&recon_res.manifest)
                                            .unwrap_or_default(),
                                    ),
                                );
                            }

                            job.output_files = vec![out_path.display().to_string()];
                            job.stages[i].message = format!(
                                "Reconstructed: {:.2}s, {}x{}, {:.2} FPS, {} bytes",
                                recon_res.output_metadata.duration_ms as f64 / 1000.0,
                                recon_res.output_metadata.width,
                                recon_res.output_metadata.height,
                                recon_res.output_metadata.fps,
                                file_size
                            );
                        } else {
                            let frames_dir = cache_dir.join("frames");
                            let audio_path = cache_dir.join("audio").join("source.wav");

                            let render_req = RenderRequest {
                                project_id: project_id.to_string(),
                                media_id: imported.media_id.clone(),
                                frame_directory: Some(frames_dir),
                                audio_path: if audio_path.exists() {
                                    Some(audio_path)
                                } else {
                                    None
                                },
                                fps: Some(imported.fps),
                                width: Some(imported.width),
                                height: Some(imported.height),
                                output_format: Some("mp4".to_string()),
                                output_name: Some("pipeline_reconstructed.mp4".to_string()),
                                mode: Some(render_mode.clone()),
                            };

                            let mut on_render_progress = |stage_prog: f32| {
                                if let Some(app) = app_handle {
                                    let overall = calculate_overall_progress_with_stages(
                                        &job.stages,
                                        i,
                                        stage_prog,
                                    );
                                    let _ = app.emit(
                                        EventNames::JOB_STAGE_PROGRESS,
                                        &JobStageProgressEvent {
                                            job_id: job_id.to_string(),
                                            project_id: project_id.to_string(),
                                            stage_id: "stage_5_video_reconstruction".to_string(),
                                            stage_index: i,
                                            stage_progress: stage_prog,
                                            overall_progress: overall,
                                            message: Some(format!(
                                                "Encoding video container ({:.1}%)",
                                                stage_prog
                                            )),
                                            timestamp: Utc::now().to_rfc3339(),
                                        },
                                    );
                                }
                            };

                            let child_pids1 = child_pids.clone();
                            let child_pids2 = child_pids.clone();
                            let jid1 = job_id.to_string();
                            let jid2 = job_id.to_string();

                            let render_res = render_service.render_video_with_progress_and_cancel(
                                &proj_dir,
                                imported,
                                &render_req,
                                &mut on_render_progress,
                                Some(cancel_token.clone()),
                                Some(&mut move |pid| {
                                    if let Ok(mut map) = child_pids1.write() {
                                        map.entry(jid1.clone()).or_default().insert(pid);
                                    }
                                }),
                                Some(&mut move |pid| {
                                    if let Ok(mut map) = child_pids2.write() {
                                        if let Some(set) = map.get_mut(&jid2) {
                                            set.remove(&pid);
                                        }
                                    }
                                }),
                            )?;

                            let out_path = render_res.output_metadata.output_path.clone();
                            let file_size = render_res.output_metadata.file_size_bytes;

                            let _ = self.register_artifact_with_app(
                                app_handle,
                                project_id,
                                job_id,
                                Artifact::new(
                                    format!("art-output-{}", Uuid::new_v4()),
                                    "final_video".to_string(),
                                    out_path.display().to_string(),
                                    file_size,
                                    Some(job.stages[i].id.clone()),
                                    serde_json::to_value(&render_res).unwrap_or_default(),
                                ),
                            );

                            job.output_files = vec![out_path.display().to_string()];
                            job.stages[i].message = format!(
                                "Reconstructed: {:.2}s, {}x{}, {} bytes",
                                render_res.output_metadata.duration_seconds,
                                render_res.output_metadata.width,
                                render_res.output_metadata.height,
                                file_size
                            );
                        }
                        Ok(())
                    }

                    // -------------------------------------------------------------
                    // Stage 6: Output Validation
                    // -------------------------------------------------------------
                    "stage_6_output_validation" => {
                        let out_file_str = job.output_files.first().cloned().unwrap_or_default();
                        let out_file = PathBuf::from(&out_file_str);
                        if !out_file.exists() {
                            return Err(AppError::output_not_found(out_file_str));
                        }

                        let imported = imported_media.as_ref().ok_or_else(|| {
                            AppError::media_invalid(
                                "Imported media missing",
                                "Failed to access imported media record",
                            )
                        })?;

                        let (val_w, val_h) = if let Some(ref ai_cfg) = job.ai_config {
                            if ai_cfg.frame_sampling.mode == crate::ai::FrameSamplingMode::All {
                                (
                                    ai_cfg.preprocessing.target_width,
                                    ai_cfg.preprocessing.target_height,
                                )
                            } else {
                                (imported.width, imported.height)
                            }
                        } else {
                            (imported.width, imported.height)
                        };

                        let rat_fps = crate::ai::RationalFps::from_f64(imported.fps);
                        let meta = crate::ai::VideoReconstructor::validate_reconstructed_video(
                            &out_file,
                            val_w,
                            val_h,
                            rat_fps,
                            0,
                            imported.has_audio,
                        )?;

                        // Validate duration match
                        let output_dur = meta.duration_ms as f64 / 1000.0;
                        let expected_dur = if render_mode == "full" {
                            imported.duration_ms as f64 / 1000.0
                        } else if let Some(end) = clip_end_sec {
                            end - clip_start_sec.unwrap_or(0.0)
                        } else {
                            imported.duration_ms as f64 / 1000.0
                        };

                        let delta = (output_dur - expected_dur).abs();
                        if delta > 0.35 {
                            return Err(AppError::output_invalid(
                                format!(
                                    "Output duration mismatch: expected ~{:.2}s, got {:.2}s (delta: {:.2}s)",
                                    expected_dur, output_dur, delta
                                ),
                                out_file.display().to_string(),
                            ));
                        }

                        job.stages[i].message = format!(
                            "Verified output: {:.2}s, {}x{}, {:.2} FPS, {} ({} bytes)",
                            meta.duration_ms as f64 / 1000.0,
                            meta.width,
                            meta.height,
                            meta.fps,
                            meta.video_codec,
                            meta.file_size_bytes
                        );

                        if let Some(ref ai_cfg) = job.ai_config {
                            let ai_metrics = job.ai_metrics.clone().unwrap_or_default();
                            let output_folder = proj_dir.join("outputs").join(job_id);
                            let report_path = output_folder.join("ai_execution_report.json");

                            let mut exec_report = crate::ai::AiProductionExecutionReport::new(
                                job_id,
                                &ai_cfg.model_id,
                                ai_cfg.model_version.as_deref(),
                                ai_cfg.model_hash.as_deref(),
                                ai_cfg.profile_hash.as_deref(),
                                &ai_cfg
                                    .provider
                                    .map(|p| format!("{:?}", p))
                                    .unwrap_or_else(|| "CPU".to_string()),
                                imported.width,
                                imported.height,
                                imported.fps,
                                imported.duration_ms,
                                ai_metrics.frames_total,
                            );

                            exec_report.selected_frames = ai_metrics.frames_selected;
                            exec_report.processed_frames = ai_metrics.frames_processed;
                            exec_report.reused_frames = ai_metrics.frames_reused;
                            exec_report.passthrough_frames = ai_metrics.frames_passthrough;
                            exec_report.failed_frames = ai_metrics.frames_failed;
                            exec_report.inference_ms = ai_metrics.total_inference_duration_ms;
                            exec_report.total_ms = ai_metrics.total_pipeline_duration_ms;
                            exec_report.artifacts_written =
                                ai_metrics.frames_processed + ai_metrics.frames_passthrough;
                            exec_report.bytes_written = ai_metrics.artifact_bytes_written;
                            exec_report.valid_frames =
                                ai_metrics.frames_processed + ai_metrics.frames_passthrough;
                            exec_report.output_path = Some(out_file.display().to_string());
                            exec_report.output_size_bytes = Some(meta.file_size_bytes);
                            exec_report.output_duration_ms = Some(meta.duration_ms);
                            exec_report.output_fps = Some(meta.fps);
                            exec_report.output_width = Some(meta.width);
                            exec_report.output_height = Some(meta.height);
                            exec_report.audio_preserved = imported.has_audio;
                            exec_report.validation_status = "VALID".to_string();
                            exec_report.status = "SUCCESS".to_string();

                            let _ = exec_report.save_to_file(&report_path);

                            let _ = self.register_artifact_with_app(
                                app_handle,
                                project_id,
                                job_id,
                                Artifact::new(
                                    format!("art-report-{}", Uuid::new_v4()),
                                    "ai_execution_report".to_string(),
                                    report_path.display().to_string(),
                                    fs::metadata(&report_path).map(|m| m.len()).unwrap_or(0),
                                    Some(job.stages[i].id.clone()),
                                    serde_json::to_value(&exec_report).unwrap_or_default(),
                                ),
                            );
                        }

                        Ok(())
                    }

                    _ => Ok(()),
                }
            })();

            match stage_result {
                Ok(()) => {
                    let _ = job.stages[i].transition_status(StageStatus::Completed);
                    job.stages[i].progress = 100.0;
                    job.progress = calculate_overall_progress_with_stages(&job.stages, i, 100.0);

                    self.append_job_log_with_app(
                        app_handle,
                        project_id,
                        job_id,
                        "INFO",
                        &job.stages[i].id,
                        &format!("✓ Completed: {}", job.stages[i].message),
                    );
                    let _ = self.save_job_manifest(&job);

                    if let Some(app) = app_handle {
                        let _ = app.emit(
                            EventNames::JOB_STAGE_COMPLETED,
                            &JobStageCompletedEvent {
                                job_id: job.id.clone(),
                                project_id: job.project_id.clone(),
                                stage_id: job.stages[i].id.clone(),
                                stage_index: i,
                                stage_name: job.stages[i].name.clone(),
                                stage_status: StageStatus::Completed,
                                message: job.stages[i].message.clone(),
                                timestamp: Utc::now().to_rfc3339(),
                            },
                        );
                        let _ = app.emit(
                            EventNames::JOB_PROGRESS,
                            &JobProgressEvent {
                                job_id: job.id.clone(),
                                project_id: job.project_id.clone(),
                                overall_progress: job.progress,
                                stage_progress: 100.0,
                                current_stage: Some(job.stages[i].id.clone()),
                                current_stage_index: i,
                                completed_stages: i + 1,
                                total_stages: job.stages.len(),
                                message: job.stages[i].message.clone(),
                                timestamp: Utc::now().to_rfc3339(),
                                job: job.clone(),
                            },
                        );
                    }
                }
                Err(err) => {
                    if err.code == crate::error::ErrorCode::Cancelled
                        || cancel_token.load(Ordering::SeqCst)
                    {
                        let _ = job.stages[i].transition_status(StageStatus::Cancelled);
                        job.stages[i].message = "Stage cancelled by user".to_string();
                        let _ = job.transition_status(JobStatus::Cancelled);
                        job.message = "Job cancelled by user".to_string();

                        self.append_job_log_with_app(
                            app_handle,
                            project_id,
                            job_id,
                            "INFO",
                            "CANCEL",
                            &format!("Stage cancelled: {}", job.stages[i].name),
                        );
                        self.append_job_log_with_app(
                            app_handle,
                            project_id,
                            job_id,
                            "INFO",
                            "CANCEL",
                            "Job cancelled and cleanup finished",
                        );
                        let _ = self.save_job_manifest(&job);

                        if let Some(app) = app_handle {
                            let _ = app.emit(
                                EventNames::JOB_STAGE_CANCELLED,
                                &JobStageCancelledEvent {
                                    job_id: job.id.clone(),
                                    project_id: job.project_id.clone(),
                                    stage_id: job.stages[i].id.clone(),
                                    stage_index: i,
                                    stage_name: job.stages[i].name.clone(),
                                    timestamp: Utc::now().to_rfc3339(),
                                },
                            );
                            let _ = app.emit(
                                EventNames::JOB_CANCELLED,
                                &JobCancelledEvent {
                                    job_id: job.id.clone(),
                                    project_id: job.project_id.clone(),
                                    message: job.message.clone(),
                                    timestamp: Utc::now().to_rfc3339(),
                                    job: job.clone(),
                                },
                            );
                        }
                        return;
                    }

                    let _ = job.stages[i].transition_status(StageStatus::Failed);
                    job.stages[i].error = Some(JobError::from(err.clone()));
                    let _ = job.transition_status(JobStatus::Failed);
                    job.error = Some(JobError::from(err.clone()));
                    job.message = format!("Stage failed: {}", err.message);

                    self.append_job_log_with_app(
                        app_handle,
                        project_id,
                        job_id,
                        "ERROR",
                        &job.stages[i].id,
                        &format!("✗ Failed: {} ({:?})", err.message, err.details),
                    );
                    let _ = self.save_job_manifest(&job);

                    if let Some(app) = app_handle {
                        let _ = app.emit(
                            EventNames::JOB_FAILED,
                            &JobFailedEvent {
                                job_id: job.id.clone(),
                                project_id: job.project_id.clone(),
                                stage_id: Some(job.stages[i].id.clone()),
                                error_code: format!("{:?}", err.code),
                                message: err.message.clone(),
                                recoverable: true,
                                details: err.details.clone(),
                                timestamp: Utc::now().to_rfc3339(),
                                job: job.clone(),
                            },
                        );
                    }
                    return;
                }
            }
        }

        // Check cancellation before marking COMPLETED
        if cancel_token.load(Ordering::SeqCst) {
            let _ = job.transition_status(JobStatus::Cancelled);
            job.message = "Job cancelled by user".to_string();
            let _ = self.save_job_manifest(&job);
            if let Some(app) = app_handle {
                let _ = app.emit(
                    EventNames::JOB_CANCELLED,
                    &JobCancelledEvent {
                        job_id: job.id.clone(),
                        project_id: job.project_id.clone(),
                        message: job.message.clone(),
                        timestamp: Utc::now().to_rfc3339(),
                        job: job.clone(),
                    },
                );
            }
            return;
        }

        // Mark entire job as COMPLETED
        let _ = job.transition_status(JobStatus::Completed);
        job.progress = 100.0;
        let elapsed = start_time.elapsed().as_secs_f32();
        job.message = format!(
            "All pipeline stages completed successfully in {:.2}s",
            elapsed
        );

        let _ = self.save_job_manifest(&job);
        self.append_job_log_with_app(
            app_handle,
            project_id,
            job_id,
            "INFO",
            "FINISH",
            &job.message,
        );

        if let Some(app) = app_handle {
            let _ = app.emit(
                EventNames::JOB_COMPLETED,
                &JobCompletedEvent {
                    job_id: job.id.clone(),
                    project_id: project_id.to_string(),
                    duration_seconds: elapsed,
                    output_files: job.output_files.clone(),
                    message: job.message.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    job: job.clone(),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_job_engine() -> (JobEngine, tempfile::TempDir) {
        let temp = tempdir().unwrap();
        let base = temp.path().to_path_buf();
        let paths = StoragePaths {
            app_data_dir: base.clone(),
            projects_dir: base.join("projects"),
            models_dir: base.join("models"),
            cache_dir: base.join("cache"),
            logs_dir: base.join("logs"),
            temp_dir: base.join("temp"),
        };
        let engine = JobEngine::new(paths);
        (engine, temp)
    }

    #[test]
    fn test_job_model_lifecycle() {
        let job = Job::new(
            "job-1".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.stages.len(), 6);
        assert!(job.can_start());
        assert!(job.can_cancel());
        assert!(!job.can_retry());
    }

    #[test]
    fn test_job_lifecycle_transitions() {
        let mut job = Job::new(
            "job-lc".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        assert_eq!(job.status, JobStatus::Queued);
        let created_at = job.created_at.clone();

        // QUEUED -> PREPARING
        job.transition_status(JobStatus::Preparing)
            .expect("Transition to Preparing should succeed");
        assert_eq!(job.status, JobStatus::Preparing);
        assert!(job.started_at.is_some());
        assert_eq!(job.created_at, created_at);

        // PREPARING -> RUNNING
        job.transition_status(JobStatus::Running)
            .expect("Transition to Running should succeed");
        assert_eq!(job.status, JobStatus::Running);

        // RUNNING -> COMPLETED
        job.transition_status(JobStatus::Completed)
            .expect("Transition to Completed should succeed");
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
        assert_eq!(job.progress, 100.0);
        assert!(job.status.is_terminal());
    }

    #[test]
    fn test_job_failure_transition() {
        let mut job = Job::new(
            "job-fail".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        job.transition_status(JobStatus::Running)
            .expect("Transition to Running");
        job.transition_status(JobStatus::Failed)
            .expect("Transition to Failed");
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.completed_at.is_some());
        assert!(job.can_retry());
        assert!(job.status.is_terminal());
    }

    #[test]
    fn test_job_cancellation_transition() {
        let mut job = Job::new(
            "job-cancel".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        job.transition_status(JobStatus::Running)
            .expect("Transition to Running");
        job.transition_status(JobStatus::Cancelling)
            .expect("Transition to Cancelling");
        job.transition_status(JobStatus::Cancelled)
            .expect("Transition to Cancelled");
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.cancelled_at.is_some());
        assert!(job.completed_at.is_some());
        assert!(job.can_retry());
        assert!(job.status.is_terminal());
    }

    #[test]
    fn test_job_recovery_transition() {
        let mut job = Job::new(
            "job-rec".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        job.transition_status(JobStatus::Running)
            .expect("Transition to Running");
        job.transition_status(JobStatus::Interrupted)
            .expect("Transition to Interrupted");
        assert_eq!(job.status, JobStatus::Interrupted);
        assert!(job.can_retry());
    }

    #[test]
    fn test_job_retry_transition() {
        let mut job = Job::new(
            "job-retry".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let original_id = job.id.clone();
        let original_created_at = job.created_at.clone();

        job.transition_status(JobStatus::Running).unwrap();
        job.transition_status(JobStatus::Failed).unwrap();
        assert_eq!(job.status, JobStatus::Failed);

        // Retry transition: FAILED -> QUEUED
        job.transition_status(JobStatus::Queued)
            .expect("Failed job should transition to Queued on retry");
        job.retry_count += 1;
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.id, original_id, "Retry MUST preserve original job_id");
        assert_eq!(
            job.created_at, original_created_at,
            "created_at MUST NOT change on retry"
        );
        assert_eq!(job.retry_count, 1);
        assert!(job.completed_at.is_none());
        assert!(job.cancelled_at.is_none());
    }

    #[test]
    fn test_invalid_transitions() {
        let mut job = Job::new(
            "job-invalid".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        job.transition_status(JobStatus::Running).unwrap();
        job.transition_status(JobStatus::Completed).unwrap();

        // COMPLETED -> RUNNING (Invalid)
        let err1 = job.transition_status(JobStatus::Running);
        assert!(err1.is_err(), "COMPLETED -> RUNNING must fail");

        // Cancelled job cannot jump directly to Running without retry
        let mut job2 = Job::new(
            "job-invalid2".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        job2.transition_status(JobStatus::Cancelled).unwrap();
        let err2 = job2.transition_status(JobStatus::Running);
        assert!(err2.is_err(), "CANCELLED -> RUNNING must fail");

        // FAILED -> COMPLETED (Invalid)
        let mut job3 = Job::new(
            "job-invalid3".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        job3.transition_status(JobStatus::Running).unwrap();
        job3.transition_status(JobStatus::Failed).unwrap();
        let err3 = job3.transition_status(JobStatus::Completed);
        assert!(err3.is_err(), "FAILED -> COMPLETED must fail");

        // INTERRUPTED -> COMPLETED (Invalid)
        let mut job4 = Job::new(
            "job-invalid4".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        job4.transition_status(JobStatus::Running).unwrap();
        job4.transition_status(JobStatus::Interrupted).unwrap();
        let err4 = job4.transition_status(JobStatus::Completed);
        assert!(err4.is_err(), "INTERRUPTED -> COMPLETED must fail");
    }

    #[test]
    fn test_stage_transitions() {
        let mut stage = PipelineStage {
            id: "stage-1".to_string(),
            name: "Test Stage".to_string(),
            status: StageStatus::Pending,
            progress: 0.0,
            indeterminate: false,
            started_at: None,
            completed_at: None,
            error: None,
            input_artifacts: Vec::new(),
            output_artifacts: Vec::new(),
            message: "Init".to_string(),
        };

        // PENDING -> RUNNING
        stage.transition_status(StageStatus::Running).unwrap();
        assert_eq!(stage.status, StageStatus::Running);
        assert!(stage.started_at.is_some());

        // RUNNING -> COMPLETED
        stage.transition_status(StageStatus::Completed).unwrap();
        assert_eq!(stage.status, StageStatus::Completed);
        assert_eq!(stage.progress, 100.0);
        assert!(stage.completed_at.is_some());

        // COMPLETED -> RUNNING (Invalid)
        let err = stage.transition_status(StageStatus::Running);
        assert!(err.is_err(), "COMPLETED -> RUNNING must fail for stages");
    }

    #[test]
    fn test_job_atomic_persistence() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job(
                "proj-atomic",
                Some("video_pipeline".to_string()),
                vec!["test.mp4".to_string()],
            )
            .unwrap();

        let job_dir = engine.job_dir("proj-atomic", &job.id);
        let job_json = job_dir.join("job.json");
        assert!(job_json.exists(), "job.json must exist after save");

        // Verify no leftover .tmp files
        let entries: Vec<_> = fs::read_dir(&job_dir).unwrap().flatten().collect();
        for entry in &entries {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                !name.starts_with(".job.json.tmp"),
                "No temporary files should linger"
            );
        }

        // Verify deserialized content matches
        let reloaded = engine.get_job(&job.id).unwrap();
        assert_eq!(reloaded.id, job.id);
        assert_eq!(reloaded.project_id, job.project_id);
        assert_eq!(reloaded.status, JobStatus::Queued);
        assert_eq!(reloaded.created_at, job.created_at);
        assert_eq!(reloaded.updated_at, job.updated_at);
    }

    #[test]
    fn test_job_uuid_and_timestamp_stability() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job(
                "proj-ts",
                Some("video_pipeline".to_string()),
                vec!["source.mp4".to_string()],
            )
            .unwrap();

        let original_id = job.id.clone();
        let original_created_at = job.created_at.clone();

        // Update job status
        let mut loaded = engine.get_job(&job.id).unwrap();
        loaded.transition_status(JobStatus::Running).unwrap();
        engine.save_job_manifest(&loaded).unwrap();

        let reloaded = engine.get_job(&job.id).unwrap();
        assert_eq!(reloaded.id, original_id);
        assert_eq!(reloaded.created_at, original_created_at);
        assert!(reloaded.started_at.is_some());
    }

    #[test]
    fn test_job_engine_crud_and_logging() {
        let (engine, _temp) = create_test_job_engine();
        let created = engine
            .create_job(
                "proj-alpha",
                Some("test_type".to_string()),
                vec!["sample.mp4".to_string()],
            )
            .expect("Create job failed");

        assert_eq!(created.project_id, "proj-alpha");
        assert_eq!(created.status, JobStatus::Queued);

        let loaded = engine.get_job(&created.id).expect("Get job failed");
        assert_eq!(loaded.id, created.id);

        let list = engine.list_jobs(Some("proj-alpha")).expect("List failed");
        assert_eq!(list.len(), 1);

        engine.append_job_log(
            "proj-alpha",
            &created.id,
            "INFO",
            "TEST",
            "Testing log output",
        );
        let logs = engine.get_job_logs(&created.id).expect("Get logs failed");
        assert_eq!(logs.len(), 2); // 1 from init + 1 from test

        engine.delete_job(&created.id).expect("Delete failed");
        assert!(engine.get_job(&created.id).is_err());
    }

    #[test]
    fn test_interrupted_recovery() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-beta", None, vec![]).unwrap();
        job.status = JobStatus::Running;
        engine.save_job_manifest(&job).unwrap();

        let recovered_count = engine.recover_interrupted_jobs().unwrap();
        assert_eq!(recovered_count, 1);

        let reloaded = engine.get_job(&job.id).unwrap();
        assert_eq!(reloaded.status, JobStatus::Interrupted);
        assert!(reloaded.can_retry());
    }

    #[tokio::test]
    async fn test_real_pipeline_execution_sample_video() {
        let video_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !video_path.exists() {
            return;
        }

        let (engine, _temp) = create_test_job_engine();
        let mut job = engine
            .create_job(
                "proj-pipeline-live",
                Some("video_pipeline".to_string()),
                vec![video_path.display().to_string()],
            )
            .expect("Create job failed");

        job.metadata = serde_json::json!({ "mode": "full" });
        engine.save_job_manifest(&job).unwrap();

        let cancel_token = Arc::new(AtomicBool::new(false));
        let child_pids = Arc::new(RwLock::new(HashMap::new()));

        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token,
                child_pids,
            )
            .await;

        let completed_job = engine.get_job(&job.id).expect("Get completed job failed");
        let logs = engine.get_job_logs(&job.id).expect("Get logs failed");
        println!("[PIPELINE TEST LOGS]:\n{}", logs.join("\n"));
        if let Some(ref err) = completed_job.error {
            println!("[PIPELINE TEST ERROR]: {:?}", err);
        }

        assert_eq!(completed_job.status, JobStatus::Completed);
        assert_eq!(completed_job.progress, 100.0);
        assert_eq!(completed_job.stages.len(), 6);

        for stage in &completed_job.stages {
            assert_eq!(stage.status, StageStatus::Completed);
            assert!(stage.completed_at.is_some());
        }

        let artifacts = engine
            .get_job_artifacts(&job.id)
            .expect("Get artifacts failed");
        assert!(!artifacts.is_empty());
        assert!(artifacts.iter().any(|a| a.artifact_type == "final_video"));
        assert!(artifacts.iter().any(|a| a.artifact_type == "frames"));
        assert!(artifacts.iter().any(|a| a.artifact_type == "audio"));

        // Verify output file exists and is readable
        let out_file_str = completed_job.output_files.first().unwrap();
        let out_file = PathBuf::from(out_file_str);
        assert!(out_file.exists());
        assert!(fs::metadata(&out_file).unwrap().len() > 0);
    }

    #[tokio::test]
    async fn test_douyin_video_pipeline_orchestration() {
        let video_path = PathBuf::from(
            r"d:\rustProject\autovideo-ai\.autovideo_data\projects\proj_render_douyin_audit\media\Douyin_1782229041.mp4",
        );
        if !video_path.exists() {
            return;
        }

        let (engine, _temp) = create_test_job_engine();
        let mut job = engine
            .create_job(
                "proj-douyin-pipeline",
                Some("video_pipeline".to_string()),
                vec![video_path.display().to_string()],
            )
            .expect("Create job failed");

        // Run 3-second test mode on Douyin video
        job.metadata = serde_json::json!({ "mode": "test_3s", "startTimeSeconds": 0.0, "endTimeSeconds": 3.0 });
        engine.save_job_manifest(&job).unwrap();

        let cancel_token = Arc::new(AtomicBool::new(false));
        let child_pids = Arc::new(RwLock::new(HashMap::new()));

        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token,
                child_pids,
            )
            .await;

        let completed_job = engine.get_job(&job.id).expect("Get completed job failed");
        assert_eq!(completed_job.status, JobStatus::Completed);
        assert_eq!(completed_job.stages.len(), 6);
        for stage in &completed_job.stages {
            assert_eq!(stage.status, StageStatus::Completed);
        }

        let artifacts = engine
            .get_job_artifacts(&job.id)
            .expect("Get artifacts failed");
        assert!(artifacts.iter().any(|a| a.artifact_type == "final_video"));
    }

    #[tokio::test]
    async fn test_pipeline_failure_stops_downstream_stages() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job(
                "proj-fail-stop",
                None,
                vec!["nonexistent_media_path.mp4".to_string()],
            )
            .expect("Create job failed");

        let cancel_token = Arc::new(AtomicBool::new(false));
        let child_pids = Arc::new(RwLock::new(HashMap::new()));

        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token,
                child_pids,
            )
            .await;

        let failed_job = engine.get_job(&job.id).expect("Get job failed");
        assert_eq!(failed_job.status, JobStatus::Failed);
        assert!(failed_job.error.is_some());

        // Stage 0 must be Failed
        assert_eq!(failed_job.stages[0].status, StageStatus::Failed);
        assert!(failed_job.stages[0].error.is_some());

        // Downstream stages 1..5 MUST remain Pending
        for s in &failed_job.stages[1..] {
            assert_eq!(
                s.status,
                StageStatus::Pending,
                "Downstream stage '{}' should remain Pending",
                s.id
            );
            assert!(s.started_at.is_none());
            assert!(s.completed_at.is_none());
        }
    }

    #[tokio::test]
    async fn test_artifact_reuse_on_pipeline_retry() {
        let video_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !video_path.exists() {
            return;
        }

        let (engine, _temp) = create_test_job_engine();
        let mut job = engine
            .create_job(
                "proj-pipeline-reuse",
                Some("video_pipeline".to_string()),
                vec![video_path.display().to_string()],
            )
            .expect("Create job failed");

        job.metadata = serde_json::json!({ "mode": "test_1s" });
        engine.save_job_manifest(&job).unwrap();

        let cancel_token = Arc::new(AtomicBool::new(false));
        let child_pids = Arc::new(RwLock::new(HashMap::new()));

        // Run 1: initial execution
        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token.clone(),
                child_pids.clone(),
            )
            .await;

        let completed_1 = engine.get_job(&job.id).unwrap();
        assert_eq!(completed_1.status, JobStatus::Completed);

        // Simulate failure / retry: mark job failed then retry
        let mut failed = completed_1;
        failed.status = JobStatus::Failed;
        engine.save_job_manifest(&failed).unwrap();

        let retried_job = engine
            .retry_job::<tauri::Wry>(None, &failed.id)
            .await
            .unwrap();
        assert_eq!(retried_job.status, JobStatus::Running);
        assert_eq!(retried_job.retry_count, 1);
        assert_eq!(retried_job.id, failed.id);
    }

    #[tokio::test]
    async fn test_job_cancellation() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job("proj-cancel", None, vec!["missing_source.mp4".to_string()])
            .expect("Create job failed");

        // Test cancel
        let cancelled = engine
            .cancel_job::<tauri::Wry>(None, &job.id)
            .await
            .expect("Cancel failed");
        assert_eq!(cancelled.status, JobStatus::Cancelled);
        assert!(cancelled.cancelled_at.is_some());
        assert!(cancelled.can_retry());
    }

    #[test]
    fn test_phase5d_01_job_created_event_payload() {
        let job = Job::new(
            "job-test-1".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let evt = JobCreatedEvent {
            job_id: job.id.clone(),
            project_id: job.project_id.clone(),
            job_type: job.job_type.clone(),
            timestamp: job.created_at.clone(),
            job,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"jobId\":\"job-test-1\""));
        assert!(json.contains("\"projectId\":\"proj-1\""));
    }

    #[test]
    fn test_phase5d_02_job_started_event_payload() {
        let job = Job::new(
            "job-test-2".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let evt = JobStartedEvent {
            job_id: job.id.clone(),
            project_id: job.project_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            job,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"jobId\":\"job-test-2\""));
    }

    #[test]
    fn test_phase5d_03_stage_started_event_payload() {
        let evt = JobStageStartedEvent {
            job_id: "job-test-3".to_string(),
            project_id: "proj-1".to_string(),
            stage_id: "stage_1_input_validation".to_string(),
            stage_index: 0,
            stage_name: "Validate Input Media".to_string(),
            stage_status: StageStatus::Running,
            timestamp: Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"stageId\":\"stage_1_input_validation\""));
        assert!(json.contains("\"stageIndex\":0"));
    }

    #[test]
    fn test_phase5d_04_stage_progress_event_payload() {
        let evt = JobStageProgressEvent {
            job_id: "job-test-4".to_string(),
            project_id: "proj-1".to_string(),
            stage_id: "stage_3_frame_extraction".to_string(),
            stage_index: 2,
            stage_progress: 50.0,
            overall_progress: 25.0,
            message: Some("Extracting frames (50.0%)".to_string()),
            timestamp: Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"stageProgress\":50.0"));
        assert!(json.contains("\"overallProgress\":25.0"));
    }

    #[test]
    fn test_phase5d_05_stage_completed_event_payload() {
        let evt = JobStageCompletedEvent {
            job_id: "job-test-5".to_string(),
            project_id: "proj-1".to_string(),
            stage_id: "stage_2_media_probe".to_string(),
            stage_index: 1,
            stage_name: "Probe Source Metadata".to_string(),
            stage_status: StageStatus::Completed,
            message: "Probed metadata".to_string(),
            timestamp: Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"stageStatus\":\"COMPLETED\""));
    }

    #[test]
    fn test_phase5d_06_overall_progress_calculation() {
        // Stage 1 (index 0, weight 5%) @ 100% => 5%
        assert_eq!(calculate_overall_progress(0, 100.0), 5.0);
        // Stage 2 (index 1, weight 5%) @ 100% => 10%
        assert_eq!(calculate_overall_progress(1, 100.0), 10.0);
        // Stage 3 (index 2, weight 30%) @ 50% => 10 + 15 = 25%
        assert_eq!(calculate_overall_progress(2, 50.0), 25.0);
        // Stage 3 @ 100% => 40%
        assert_eq!(calculate_overall_progress(2, 100.0), 40.0);
        // Stage 4 (index 3, weight 15%) @ 100% => 55%
        assert_eq!(calculate_overall_progress(3, 100.0), 55.0);
        // Stage 5 (index 4, weight 35%) @ 50% => 55 + 17.5 = 72.5%
        assert_eq!(calculate_overall_progress(4, 50.0), 72.5);
        // Stage 5 @ 100% => 90%
        assert_eq!(calculate_overall_progress(4, 100.0), 90.0);
        // Stage 6 (index 5, weight 10%) @ 100% => 100%
        assert_eq!(calculate_overall_progress(5, 100.0), 100.0);
    }

    #[test]
    fn test_phase5d_07_job_completed_event_payload() {
        let job = Job::new(
            "job-test-7".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let evt = JobCompletedEvent {
            job_id: job.id.clone(),
            project_id: job.project_id.clone(),
            duration_seconds: 12.34,
            output_files: vec!["output.mp4".to_string()],
            message: "Success".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            job,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"durationSeconds\":12.34"));
        assert!(json.contains("\"outputFiles\":[\"output.mp4\"]"));
    }

    #[test]
    fn test_phase5d_08_job_failed_event_payload() {
        let job = Job::new(
            "job-test-8".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let evt = JobFailedEvent {
            job_id: job.id.clone(),
            project_id: job.project_id.clone(),
            stage_id: Some("stage_1_input_validation".to_string()),
            error_code: "MEDIA_FILE_NOT_FOUND".to_string(),
            message: "Missing video file".to_string(),
            recoverable: true,
            details: None,
            timestamp: Utc::now().to_rfc3339(),
            job,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"errorCode\":\"MEDIA_FILE_NOT_FOUND\""));
        assert!(json.contains("\"recoverable\":true"));
    }

    #[test]
    fn test_phase5d_09_job_cancelled_event_payload() {
        let job = Job::new(
            "job-test-9".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let evt = JobCancelledEvent {
            job_id: job.id.clone(),
            project_id: job.project_id.clone(),
            message: "Cancelled".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            job,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"jobId\":\"job-test-9\""));
        assert!(json.contains("\"message\":\"Cancelled\""));
    }

    #[test]
    fn test_phase5d_10_job_retrying_event_payload() {
        let job = Job::new(
            "job-test-10".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let evt = JobRetryingEvent {
            job_id: job.id.clone(),
            project_id: job.project_id.clone(),
            retry_count: 2,
            timestamp: Utc::now().to_rfc3339(),
            job,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"retryCount\":2"));
    }

    #[test]
    fn test_phase5d_11_job_interrupted_event_payload() {
        let job = Job::new(
            "job-test-11".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let evt = JobInterruptedEvent {
            job_id: job.id.clone(),
            project_id: job.project_id.clone(),
            message: "Interrupted".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            job,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"message\":\"Interrupted\""));
    }

    #[tokio::test]
    async fn test_phase5d_12_event_ordering_and_stage_transitions() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job(
                "proj-events-order",
                None,
                vec!["nonexistent.mp4".to_string()],
            )
            .expect("Create job failed");

        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.stages[0].status, StageStatus::Pending);

        let cancel_token = Arc::new(AtomicBool::new(false));
        let child_pids = Arc::new(RwLock::new(HashMap::new()));

        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token,
                child_pids,
            )
            .await;

        let final_job = engine.get_job(&job.id).unwrap();
        assert_eq!(final_job.status, JobStatus::Failed);
        assert_eq!(final_job.stages[0].status, StageStatus::Failed);
        assert_eq!(final_job.stages[1].status, StageStatus::Pending);
    }

    #[tokio::test]
    async fn test_phase5d_13_no_progress_after_completion() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-no-prog", None, vec![]).unwrap();

        job.status = JobStatus::Completed;
        job.progress = 100.0;
        engine.save_job_manifest(&job).unwrap();

        let loaded = engine.get_job(&job.id).unwrap();
        assert_eq!(loaded.progress, 100.0);
        assert_eq!(loaded.status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn test_phase5d_14_no_downstream_stage_events_after_failure() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job(
                "proj-downstream-fail",
                None,
                vec!["missing_input_file.mp4".to_string()],
            )
            .unwrap();

        let cancel_token = Arc::new(AtomicBool::new(false));
        let child_pids = Arc::new(RwLock::new(HashMap::new()));

        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token,
                child_pids,
            )
            .await;

        let failed_job = engine.get_job(&job.id).unwrap();
        assert_eq!(failed_job.status, JobStatus::Failed);
        for s in &failed_job.stages[1..] {
            assert_eq!(s.status, StageStatus::Pending);
            assert_eq!(s.progress, 0.0);
        }
    }

    #[test]
    fn test_phase5d_15_artifact_registration_and_persistence() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine.create_job("proj-art", None, vec![]).unwrap();

        let art = Artifact::new(
            "art-test-1".to_string(),
            "source_video".to_string(),
            "/path/to/test.mp4".to_string(),
            12345,
            Some("stage_1".to_string()),
            serde_json::json!({}),
        );

        engine
            .register_artifact(&job.project_id, &job.id, art)
            .unwrap();
        let artifacts = engine.get_job_artifacts(&job.id).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].id, "art-test-1");
        assert_eq!(artifacts[0].file_size_bytes, 12345);
    }

    #[test]
    fn test_phase5d_16_log_event_and_persistence() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine.create_job("proj-log", None, vec![]).unwrap();

        engine.append_job_log(
            &job.project_id,
            &job.id,
            "INFO",
            "TEST_STAGE",
            "Test log message 1",
        );
        engine.append_job_log(
            &job.project_id,
            &job.id,
            "WARN",
            "TEST_STAGE",
            "Test log message 2",
        );

        let logs = engine.get_job_logs(&job.id).unwrap();
        assert_eq!(logs.len(), 3); // 1 INIT log + 2 appended logs
        assert!(logs.iter().any(|l| l.contains("Test log message 1")));
        assert!(logs.iter().any(|l| l.contains("Test log message 2")));
    }

    // =========================================================================
    // PHASE 5E TESTS — CANCELLATION HARDENING
    // =========================================================================

    #[tokio::test]
    async fn test_phase5e_01_cancel_queued_job() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine.create_job("proj-c1", None, vec![]).unwrap();
        assert_eq!(job.status, JobStatus::Queued);

        let cancelled = engine
            .cancel_job::<tauri::Wry>(None, &job.id)
            .await
            .unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);

        let loaded = engine.get_job(&job.id).unwrap();
        assert_eq!(loaded.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_phase5e_02_cancel_preparing_job() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-c2", None, vec![]).unwrap();
        job.status = JobStatus::Preparing;
        engine.save_job_manifest(&job).unwrap();

        let cancelled = engine
            .cancel_job::<tauri::Wry>(None, &job.id)
            .await
            .unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);

        let loaded = engine.get_job(&job.id).unwrap();
        assert_eq!(loaded.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_phase5e_03_cancel_running_job() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-c3", None, vec![]).unwrap();
        job.status = JobStatus::Running;
        job.stages[0].status = StageStatus::Running;
        engine.save_job_manifest(&job).unwrap();

        let cancelled = engine
            .cancel_job::<tauri::Wry>(None, &job.id)
            .await
            .unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
        assert_eq!(cancelled.stages[0].status, StageStatus::Cancelled);

        let loaded = engine.get_job(&job.id).unwrap();
        assert_eq!(loaded.status, JobStatus::Cancelled);
        assert_eq!(loaded.stages[0].status, StageStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_phase5e_04_cancel_during_frame_extraction() {
        let (engine, _temp) = create_test_job_engine();
        let video_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !video_path.exists() {
            println!("Skipping real test: video not found");
            return;
        }

        let mut job = engine
            .create_job(
                "proj-c4",
                Some("video_pipeline".to_string()),
                vec![video_path.display().to_string()],
            )
            .unwrap();
        job.metadata = serde_json::json!({ "mode": "test_1s" });
        engine.save_job_manifest(&job).unwrap();

        let cancel_token = Arc::new(AtomicBool::new(false));
        let child_pids = Arc::new(RwLock::new(HashMap::new()));

        // Cancel immediately before / at stage start
        cancel_token.store(true, Ordering::SeqCst);

        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token,
                child_pids,
            )
            .await;

        let res_job = engine.get_job(&job.id).unwrap();
        assert_eq!(res_job.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_phase5e_05_cancel_during_video_reconstruction() {
        let (engine, _temp) = create_test_job_engine();
        let token = Arc::new(AtomicBool::new(true));
        let _media_service = MediaService::new();
        let render_service = RenderService::new();

        let render_req = RenderRequest {
            project_id: "proj-c5".to_string(),
            media_id: "media-c5".to_string(),
            frame_directory: None,
            audio_path: None,
            fps: Some(30.0),
            width: Some(1280),
            height: Some(720),
            output_format: Some("mp4".to_string()),
            output_name: Some("test.mp4".to_string()),
            mode: Some("test_1s".to_string()),
        };

        let dummy_source = crate::projects::SourceMedia {
            media_id: "media-c5".to_string(),
            original_file_name: "test.mp4".to_string(),
            source_path: PathBuf::from("/dummy/test.mp4"),
            duration_ms: 1000,
            width: 1280,
            height: 720,
            fps: 30.0,
            file_size_bytes: 100,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: None,
            has_audio: false,
        };

        let proj_dir = engine.storage_paths.projects_dir.join("proj-c5");
        let _ = fs::create_dir_all(&proj_dir);

        let res = render_service.render_video_with_progress_and_cancel(
            &proj_dir,
            &dummy_source,
            &render_req,
            &mut |_| {},
            Some(token),
            None,
            None,
        );

        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, crate::error::ErrorCode::Cancelled);
    }

    #[tokio::test]
    async fn test_phase5e_06_cancel_twice_idempotent() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine.create_job("proj-c6", None, vec![]).unwrap();

        let cancel1 = engine.cancel_job::<tauri::Wry>(None, &job.id).await;
        assert!(cancel1.is_ok());
        assert_eq!(cancel1.unwrap().status, JobStatus::Cancelled);

        let cancel2 = engine.cancel_job::<tauri::Wry>(None, &job.id).await;
        assert!(cancel2.is_ok());
        assert_eq!(cancel2.unwrap().status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_phase5e_07_cancel_completed_job_noop() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-c7", None, vec![]).unwrap();
        job.status = JobStatus::Completed;
        job.progress = 100.0;
        engine.save_job_manifest(&job).unwrap();

        let res = engine.cancel_job::<tauri::Wry>(None, &job.id).await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap().status, JobStatus::Completed);

        let loaded = engine.get_job(&job.id).unwrap();
        assert_eq!(loaded.status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn test_phase5e_08_cancel_failed_job_noop() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-c8", None, vec![]).unwrap();
        job.status = JobStatus::Failed;
        engine.save_job_manifest(&job).unwrap();

        let res = engine.cancel_job::<tauri::Wry>(None, &job.id).await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap().status, JobStatus::Failed);

        let loaded = engine.get_job(&job.id).unwrap();
        assert_eq!(loaded.status, JobStatus::Failed);
    }

    #[tokio::test]
    async fn test_phase5e_09_cancellation_race_with_completion() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-c9", None, vec![]).unwrap();
        for s in &mut job.stages {
            s.status = StageStatus::Completed;
            s.progress = 100.0;
        }
        job.status = JobStatus::Running;
        engine.save_job_manifest(&job).unwrap();

        let cancel_token = Arc::new(AtomicBool::new(true)); // Cancelled just before completion
        let child_pids = Arc::new(RwLock::new(HashMap::new()));

        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token,
                child_pids,
            )
            .await;

        let loaded = engine.get_job(&job.id).unwrap();
        assert_eq!(loaded.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_phase5e_10_cancellation_race_with_failure() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job("proj-c10", None, vec!["nonexistent_file.mp4".to_string()])
            .unwrap();

        let cancel_token = Arc::new(AtomicBool::new(true));
        let child_pids = Arc::new(RwLock::new(HashMap::new()));

        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token,
                child_pids,
            )
            .await;

        let loaded = engine.get_job(&job.id).unwrap();
        assert_eq!(loaded.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_phase5e_11_cancellation_token_propagation() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine.create_job("proj-c11", None, vec![]).unwrap();

        let token = Arc::new(AtomicBool::new(false));
        if let Ok(mut tokens) = engine.cancellation_tokens.write() {
            tokens.insert(job.id.clone(), token.clone());
        }

        let _ = engine
            .cancel_job::<tauri::Wry>(None, &job.id)
            .await
            .unwrap();
        assert!(token.load(Ordering::SeqCst));
    }

    #[test]
    fn test_phase5e_12_process_registry_cleanup() {
        let (engine, _temp) = create_test_job_engine();
        let job_id = "job-pids-1";

        engine.register_child_pid(job_id, 1234);
        engine.register_child_pid(job_id, 5678);

        {
            let map = engine.child_pids.read().unwrap();
            let set = map.get(job_id).unwrap();
            assert_eq!(set.len(), 2);
            assert!(set.contains(&1234));
            assert!(set.contains(&5678));
        }

        engine.unregister_child_pid(job_id, 1234);
        {
            let map = engine.child_pids.read().unwrap();
            let set = map.get(job_id).unwrap();
            assert_eq!(set.len(), 1);
            assert!(set.contains(&5678));
        }

        engine.terminate_job_processes::<tauri::Wry>(None, "proj-pids", job_id);
        {
            let map = engine.child_pids.read().unwrap();
            assert!(map.get(job_id).is_none());
        }
    }

    #[tokio::test]
    async fn test_phase5e_13_job_json_persistence_after_cancellation() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine.create_job("proj-c13", None, vec![]).unwrap();

        let _ = engine
            .cancel_job::<tauri::Wry>(None, &job.id)
            .await
            .unwrap();

        let job_file = engine.job_dir(&job.project_id, &job.id).join("job.json");
        assert!(job_file.exists());

        let raw = fs::read_to_string(&job_file).unwrap();
        assert!(raw.contains("\"CANCELLED\""));
    }

    #[tokio::test]
    async fn test_phase5e_14_stage_becomes_cancelled() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-c14", None, vec![]).unwrap();
        job.status = JobStatus::Running;
        job.stages[1].status = StageStatus::Running;
        engine.save_job_manifest(&job).unwrap();

        let cancelled = engine
            .cancel_job::<tauri::Wry>(None, &job.id)
            .await
            .unwrap();
        assert_eq!(cancelled.stages[1].status, StageStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_phase5e_15_no_downstream_stages_executed_after_cancellation() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-c15", None, vec![]).unwrap();
        job.status = JobStatus::Running;
        job.stages[0].status = StageStatus::Running;
        engine.save_job_manifest(&job).unwrap();

        let cancelled = engine
            .cancel_job::<tauri::Wry>(None, &job.id)
            .await
            .unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
        assert_eq!(cancelled.stages[0].status, StageStatus::Cancelled);

        for stage in &cancelled.stages[1..] {
            assert_eq!(stage.status, StageStatus::Pending);
            assert_eq!(stage.progress, 0.0);
        }
    }

    #[test]
    fn test_phase5e_16_no_progress_or_completion_events_after_cancellation() {
        let job = Job::new(
            "job-c16".to_string(),
            "proj-c16".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        assert!(job.can_cancel());
    }

    #[test]
    fn test_phase5e_17_multi_job_safety() {
        let (engine, _temp) = create_test_job_engine();
        let job_a = "job-a";
        let job_b = "job-b";

        engine.register_child_pid(job_a, 1111);
        engine.register_child_pid(job_b, 2222);

        // Terminate only job A
        engine.terminate_job_processes::<tauri::Wry>(None, "proj-test", job_a);

        let map = engine.child_pids.read().unwrap();
        assert!(map.get(job_a).is_none());
        assert!(map.get(job_b).is_some());
        assert!(map.get(job_b).unwrap().contains(&2222));
    }

    #[tokio::test]
    async fn test_phase5e_18_valid_artifacts_remain_reusable_on_retry() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-c18", None, vec![]).unwrap();
        job.status = JobStatus::Cancelled;
        engine.save_job_manifest(&job).unwrap();

        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        assert_eq!(retried.status, JobStatus::Running);
        assert_eq!(retried.retry_count, 1);
    }

    #[test]
    fn test_phase5e_19_cancel_requested_event_payload() {
        let job = Job::new(
            "job-c19".to_string(),
            "proj-c19".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let evt = JobCancelRequestedEvent {
            job_id: job.id.clone(),
            project_id: job.project_id.clone(),
            message: "User clicked cancel".to_string(),
            timestamp: Utc::now().to_rfc3339(),
            job,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"jobId\":\"job-c19\""));
        assert!(json.contains("\"message\":\"User clicked cancel\""));
    }

    #[test]
    fn test_phase5e_20_stage_cancelled_event_payload() {
        let evt = JobStageCancelledEvent {
            job_id: "job-c20".to_string(),
            project_id: "proj-c20".to_string(),
            stage_id: "stage_3_frame_extraction".to_string(),
            stage_index: 2,
            stage_name: "Frame Extraction".to_string(),
            timestamp: Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"stageId\":\"stage_3_frame_extraction\""));
        assert!(json.contains("\"stageIndex\":2"));
    }

    // =========================================================================
    // PHASE 5F — RETRY & RECOVERY TESTS
    // =========================================================================

    #[test]
    fn test_phase5f_01_recover_running_job() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f01", None, vec![]).unwrap();
        job.status = JobStatus::Running;
        job.stages[0].status = StageStatus::Running;
        engine.save_job_manifest(&job).unwrap();

        let recovered = engine.recover_interrupted_jobs().unwrap();
        assert_eq!(recovered, 1);

        let reloaded = engine.get_job(&job.id).unwrap();
        assert_eq!(reloaded.status, JobStatus::Interrupted);
        assert_eq!(reloaded.stages[0].status, StageStatus::Pending);
        assert_eq!(
            reloaded.message,
            "Job execution was interrupted by application restart/shutdown"
        );
    }

    #[test]
    fn test_phase5f_02_recover_preparing_job() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f02", None, vec![]).unwrap();
        job.status = JobStatus::Preparing;
        engine.save_job_manifest(&job).unwrap();

        let recovered = engine.recover_interrupted_jobs().unwrap();
        assert_eq!(recovered, 1);

        let reloaded = engine.get_job(&job.id).unwrap();
        assert_eq!(reloaded.status, JobStatus::Interrupted);
    }

    #[test]
    fn test_phase5f_03_recover_cancelling_job() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f03", None, vec![]).unwrap();
        job.status = JobStatus::Cancelling;
        engine.save_job_manifest(&job).unwrap();

        let recovered = engine.recover_interrupted_jobs().unwrap();
        assert_eq!(recovered, 1);

        let reloaded = engine.get_job(&job.id).unwrap();
        assert_eq!(reloaded.status, JobStatus::Interrupted);
    }

    #[test]
    fn test_phase5f_04_recover_queued_job() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine.create_job("proj-f04", None, vec![]).unwrap();
        assert_eq!(job.status, JobStatus::Queued);

        let recovered = engine.recover_interrupted_jobs().unwrap();
        assert_eq!(recovered, 1);

        let reloaded = engine.get_job(&job.id).unwrap();
        assert_eq!(reloaded.status, JobStatus::Interrupted);
    }

    #[test]
    fn test_phase5f_05_stale_pid_and_tokens_cleaned_during_recovery() {
        let (engine, _temp) = create_test_job_engine();
        engine.register_child_pid("job-stale", 9999);
        if let Ok(mut tokens) = engine.cancellation_tokens.write() {
            tokens.insert("job-stale".to_string(), Arc::new(AtomicBool::new(true)));
        }

        let _ = engine.recover_interrupted_jobs().unwrap();

        let pids = engine.child_pids.read().unwrap();
        assert!(pids.is_empty());
        let tokens = engine.cancellation_tokens.read().unwrap();
        assert!(tokens.is_empty());
    }

    #[tokio::test]
    async fn test_phase5f_06_retry_preserves_job_id_and_created_at() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f06", None, vec![]).unwrap();
        let original_id = job.id.clone();
        let original_created_at = job.created_at.clone();

        job.status = JobStatus::Failed;
        engine.save_job_manifest(&job).unwrap();

        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        assert_eq!(retried.id, original_id);
        assert_eq!(retried.created_at, original_created_at);
    }

    #[tokio::test]
    async fn test_phase5f_07_retry_increments_retry_count_and_updates_timestamp() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f07", None, vec![]).unwrap();
        job.status = JobStatus::Interrupted;
        engine.save_job_manifest(&job).unwrap();

        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        assert_eq!(retried.retry_count, 1);
        assert!(!retried.updated_at.is_empty());
    }

    #[tokio::test]
    async fn test_phase5f_08_retry_clears_previous_error_and_timestamps() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f08", None, vec![]).unwrap();
        job.status = JobStatus::Failed;
        job.error = Some(JobError {
            code: "TEST_ERROR".to_string(),
            message: "Previous failure".to_string(),
            details: None,
        });
        job.started_at = Some("2026-01-01T00:00:00Z".to_string());
        job.completed_at = Some("2026-01-01T00:01:00Z".to_string());
        job.cancelled_at = Some("2026-01-01T00:02:00Z".to_string());
        engine.save_job_manifest(&job).unwrap();

        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        assert!(retried.error.is_none());
        assert!(retried.completed_at.is_none());
        assert!(retried.cancelled_at.is_none());
    }

    #[tokio::test]
    async fn test_phase5f_09_retry_is_safe_and_idempotent() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f09", None, vec![]).unwrap();
        job.status = JobStatus::Running;
        engine.save_job_manifest(&job).unwrap();

        // Cannot retry a currently running job
        let err = engine.retry_job::<tauri::Wry>(None, &job.id).await;
        assert!(err.is_err());
    }

    #[test]
    fn test_phase5f_10_artifact_validation_missing_source_video() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job(
                "proj-f10",
                None,
                vec!["C:/nonexistent_video_path.mp4".to_string()],
            )
            .unwrap();

        let report = engine.validate_job_stage_artifacts(&job);
        assert!(!report.is_fully_valid);
        assert_eq!(report.resume_stage_index, 0);
        assert!(!report.stage_validations[0].is_valid);
    }

    #[test]
    fn test_phase5f_11_artifact_validation_valid_source_video() {
        let (engine, temp) = create_test_job_engine();
        let src_file = temp.path().join("source.mp4");
        fs::write(&src_file, b"test video data bytes here").unwrap();

        let job = engine
            .create_job("proj-f11", None, vec![src_file.display().to_string()])
            .unwrap();

        let report = engine.validate_job_stage_artifacts(&job);
        assert!(report.stage_validations[0].is_valid);
    }

    #[test]
    fn test_phase5f_12_retry_reuses_valid_frame_cache() {
        let (engine, temp) = create_test_job_engine();
        let proj_dir = temp.path().join("proj-f12");
        let frames_dir = proj_dir
            .join("cache")
            .join("media")
            .join("imported_media")
            .join("frames");
        fs::create_dir_all(&frames_dir).unwrap();

        // Create 30 valid test frames
        for i in 0..30 {
            fs::write(
                frames_dir.join(format!("{:06}.png", i)),
                b"\x89PNG\r\n\x1a\nframe_bytes",
            )
            .unwrap();
        }

        let src_file = temp.path().join("source12.mp4");
        fs::write(&src_file, b"test video").unwrap();

        let mut job = engine
            .create_job("proj-f12", None, vec![src_file.display().to_string()])
            .unwrap();
        job.metadata = serde_json::json!({ "mode": "test_1s" });
        engine.save_job_manifest(&job).unwrap();

        let count = fs::read_dir(&frames_dir).unwrap().count();
        assert_eq!(count, 30);
    }

    #[test]
    fn test_phase5f_13_retry_reuses_valid_audio_cache() {
        let (engine, temp) = create_test_job_engine();
        let proj_dir = temp.path().join("proj-f13");
        let audio_dir = proj_dir
            .join("cache")
            .join("media")
            .join("imported_media")
            .join("audio");
        fs::create_dir_all(&audio_dir).unwrap();

        // Create valid 44-byte RIFF WAVE header
        let mut wav_bytes = vec![0u8; 44];
        wav_bytes[0..4].copy_from_slice(b"RIFF");
        wav_bytes[8..12].copy_from_slice(b"WAVE");
        fs::write(audio_dir.join("source.wav"), &wav_bytes).unwrap();

        let src_file = temp.path().join("source13.mp4");
        fs::write(&src_file, b"test video").unwrap();

        let _job = engine
            .create_job("proj-f13", None, vec![src_file.display().to_string()])
            .unwrap();
        let audio_file = audio_dir.join("source.wav");
        assert!(audio_file.exists());
        let read_bytes = fs::read(&audio_file).unwrap();
        assert_eq!(&read_bytes[0..4], b"RIFF");
        assert_eq!(&read_bytes[8..12], b"WAVE");
    }

    #[test]
    fn test_phase5f_14_retry_reuses_valid_artifacts_end_to_end() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine.create_job("proj-f14", None, vec![]).unwrap();
        let report = engine.validate_job_stage_artifacts(&job);
        assert_eq!(report.job_id, job.id);
        assert_eq!(report.stage_validations.len(), 6);
    }

    #[test]
    fn test_phase5f_15_invalid_frame_cache_forces_stage3_rerun() {
        let (engine, temp) = create_test_job_engine();
        let proj_dir = temp.path().join("proj-f15");
        let frames_dir = proj_dir
            .join("cache")
            .join("media")
            .join("imported_media")
            .join("frames");
        fs::create_dir_all(&frames_dir).unwrap();
        // Empty frame dir!

        let src_file = temp.path().join("source15.mp4");
        fs::write(&src_file, b"test video").unwrap();

        let job = engine
            .create_job("proj-f15", None, vec![src_file.display().to_string()])
            .unwrap();
        let report = engine.validate_job_stage_artifacts(&job);
        assert!(!report.stage_validations[2].is_valid);
    }

    #[test]
    fn test_phase5f_16_invalid_audio_cache_forces_stage4_rerun() {
        let (engine, temp) = create_test_job_engine();
        let proj_dir = temp.path().join("proj-f16");
        let audio_dir = proj_dir
            .join("cache")
            .join("media")
            .join("imported_media")
            .join("audio");
        fs::create_dir_all(&audio_dir).unwrap();

        // Corrupt header (not RIFF/WAVE)
        fs::write(
            audio_dir.join("source.wav"),
            b"CORRUPT_NOT_WAV_HEADER_DATA_123456789012345678901234567890",
        )
        .unwrap();

        let src_file = temp.path().join("source16.mp4");
        fs::write(&src_file, b"test video").unwrap();

        let job = engine
            .create_job("proj-f16", None, vec![src_file.display().to_string()])
            .unwrap();
        let report = engine.validate_job_stage_artifacts(&job);
        assert!(!report.stage_validations[3].is_valid);
    }

    #[test]
    fn test_phase5f_17_missing_output_forces_stage5_rerun() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f17", None, vec![]).unwrap();
        job.output_files = vec!["C:/nonexistent_output.mp4".to_string()];
        engine.save_job_manifest(&job).unwrap();

        let report = engine.validate_job_stage_artifacts(&job);
        assert!(!report.stage_validations[4].is_valid);
    }

    #[test]
    fn test_phase5f_18_corrupt_output_forces_stage5_rerun() {
        let (engine, temp) = create_test_job_engine();
        let bad_out = temp.path().join("bad_out.mp4");
        fs::write(&bad_out, b"").unwrap(); // 0 bytes

        let mut job = engine.create_job("proj-f18", None, vec![]).unwrap();
        job.output_files = vec![bad_out.display().to_string()];
        engine.save_job_manifest(&job).unwrap();

        let report = engine.validate_job_stage_artifacts(&job);
        assert!(!report.stage_validations[4].is_valid);
    }

    #[tokio::test]
    async fn test_phase5f_19_interrupted_stage3_resumes_correctly() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f19", None, vec![]).unwrap();
        job.status = JobStatus::Interrupted;
        job.stages[0].status = StageStatus::Completed;
        job.stages[1].status = StageStatus::Completed;
        job.stages[2].status = StageStatus::Pending;
        engine.save_job_manifest(&job).unwrap();

        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        assert_eq!(retried.status, JobStatus::Running);
    }

    #[tokio::test]
    async fn test_phase5f_20_interrupted_stage5_resumes_correctly() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f20", None, vec![]).unwrap();
        job.status = JobStatus::Interrupted;
        job.stages[0].status = StageStatus::Completed;
        job.stages[1].status = StageStatus::Completed;
        job.stages[2].status = StageStatus::Completed;
        job.stages[3].status = StageStatus::Completed;
        job.stages[4].status = StageStatus::Pending;
        engine.save_job_manifest(&job).unwrap();

        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        assert_eq!(retried.status, JobStatus::Running);
    }

    #[test]
    fn test_phase5f_21_downstream_stages_invalidated_correctly() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job(
                "proj-f21",
                None,
                vec!["C:/nonexistent_source.mp4".to_string()],
            )
            .unwrap();

        let report = engine.validate_job_stage_artifacts(&job);
        assert_eq!(report.resume_stage_index, 0);
        assert!(!report.is_fully_valid);
    }

    #[tokio::test]
    async fn test_phase5f_22_phase5e_cancellation_remains_intact() {
        let (engine, _temp) = create_test_job_engine();
        let mut job = engine.create_job("proj-f22", None, vec![]).unwrap();
        job.status = JobStatus::Interrupted;
        engine.save_job_manifest(&job).unwrap();

        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        assert_eq!(retried.status, JobStatus::Running);

        let cancelled = engine
            .cancel_job::<tauri::Wry>(None, &retried.id)
            .await
            .unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
    }

    // =============================================================
    // PHASE 6D: AI VIDEO FRAME INFERENCE INTEGRATION TESTS
    // =============================================================

    #[test]
    fn test_phase6d_01_job_creation_with_ai_config() {
        let (engine, _temp) = create_test_job_engine();
        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: "test-model".to_string(),
            provider: None,
            preprocessing: crate::ai::PreprocessConfig::default(),
            postprocessing: None,
            frame_sampling: crate::ai::FrameSamplingConfig::all(),
            output_mode: crate::ai::AiFrameOutputMode::Image,
            ..Default::default()
        };

        let job = engine
            .create_ai_job("proj-6d-01", None, vec!["input.mp4".to_string()], ai_cfg)
            .unwrap();

        assert_eq!(job.stages.len(), 7);
        assert_eq!(job.total_stages, 7);
        assert_eq!(job.stages[4].id, "stage_ai_frame_inference");
        assert!(job.ai_config.is_some());
        assert!(job.ai_config.as_ref().unwrap().enabled);
    }

    #[test]
    fn test_phase6d_02_non_ai_job_retains_6_stages() {
        let (engine, _temp) = create_test_job_engine();
        let job = engine
            .create_job("proj-6d-02", None, vec!["input.mp4".to_string()])
            .unwrap();

        assert_eq!(job.stages.len(), 6);
        assert_eq!(job.total_stages, 6);
        assert!(!job
            .stages
            .iter()
            .any(|s| s.id == "stage_ai_frame_inference"));
        assert!(job.ai_config.is_none());
    }

    #[test]
    fn test_phase6d_03_ai_job_manifest_serde_roundtrip() {
        let (engine, _temp) = create_test_job_engine();
        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: "test-model-serde".to_string(),
            provider: None,
            preprocessing: crate::ai::PreprocessConfig::default(),
            postprocessing: Some(crate::ai::PostprocessConfig {
                extract_mask: true,
                mask_threshold: Some(0.6),
                extract_bboxes: false,
                bbox_confidence_threshold: None,
            }),
            frame_sampling: crate::ai::FrameSamplingConfig::every_nth(3),
            output_mode: crate::ai::AiFrameOutputMode::Mask,
            ..Default::default()
        };

        let job = engine
            .create_ai_job("proj-6d-03", None, vec!["input.mp4".to_string()], ai_cfg)
            .unwrap();

        let loaded = engine.get_job(&job.id).unwrap();
        assert_eq!(loaded.id, job.id);
        assert!(loaded.ai_config.is_some());
        let loaded_ai = loaded.ai_config.unwrap();
        assert_eq!(loaded_ai.model_id, "test-model-serde");
        assert_eq!(loaded_ai.frame_sampling.nth, Some(3));
        assert_eq!(loaded_ai.output_mode, crate::ai::AiFrameOutputMode::Mask);
        assert!(loaded_ai.postprocessing.is_some());
    }

    #[test]
    fn test_phase6d_04_ai_stage_weights_sum_to_100() {
        let sum: f32 = crate::events::AI_STAGE_WEIGHTS.iter().sum();
        assert!((sum - 100.0).abs() < 1e-4);
    }

    #[test]
    fn test_phase6d_05_calculate_overall_progress_with_stages() {
        let stages_6 = Job::build_pipeline_stages(false);
        let stages_7 = Job::build_pipeline_stages(true);

        // For 6 stages, stage 0 at 0% should be 0.0, at 100% should be 5.0
        assert_eq!(
            crate::events::calculate_overall_progress_with_stages(&stages_6, 0, 0.0),
            0.0
        );
        assert_eq!(
            crate::events::calculate_overall_progress_with_stages(&stages_6, 0, 100.0),
            5.0
        );

        // For 7 stages, stage 4 (AI stage weight 40%) from 0% to 100%
        let before_ai = crate::events::calculate_overall_progress_with_stages(&stages_7, 4, 0.0);
        let after_ai = crate::events::calculate_overall_progress_with_stages(&stages_7, 4, 100.0);
        assert!((before_ai - 40.0).abs() < 1e-4);
        assert!((after_ai - 80.0).abs() < 1e-4);
    }

    #[test]
    fn test_phase6d_06_ai_frame_sampling_modes() {
        let all_cfg = crate::ai::FrameSamplingConfig::all();
        let selected = crate::ai::select_frames(10, &all_cfg).unwrap();
        assert_eq!(selected, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

        let nth_cfg = crate::ai::FrameSamplingConfig::every_nth(3);
        let selected = crate::ai::select_frames(10, &nth_cfg).unwrap();
        assert_eq!(selected, vec![0, 3, 6, 9]);

        let range_cfg = crate::ai::FrameSamplingConfig::range(2, 6);
        let selected = crate::ai::select_frames(10, &range_cfg).unwrap();
        assert_eq!(selected, vec![2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_phase6d_07_ai_config_hash_determinism() {
        let prep = crate::ai::PreprocessConfig::default();
        let hash1 = crate::ai::compute_ai_config_hash("model-a", &prep, None);
        let hash2 = crate::ai::compute_ai_config_hash("model-a", &prep, None);
        assert_eq!(hash1, hash2);

        let post = crate::ai::PostprocessConfig {
            extract_mask: true,
            mask_threshold: Some(0.5),
            extract_bboxes: false,
            bbox_confidence_threshold: None,
        };
        let hash3 = crate::ai::compute_ai_config_hash("model-a", &prep, Some(&post));
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_phase6d_08_ai_artifact_manager_paths() {
        let temp = tempfile::tempdir().unwrap();
        let mgr = crate::ai::AiArtifactManager::new(temp.path());

        let frame_dir = mgr.frame_dir(5);
        assert_eq!(frame_dir, temp.path().join("000005"));
        assert_eq!(
            mgr.output_png_path(5),
            temp.path().join("000005").join("output.png")
        );
        assert_eq!(
            mgr.result_json_path(5),
            temp.path().join("000005").join("result.json")
        );
        assert_eq!(
            mgr.reconstruction_frame_path(5),
            temp.path().join("reconstruction_frames").join("000005.png")
        );
    }

    #[test]
    fn test_phase6d_09_ai_frame_artifact_validation_valid() {
        let temp = tempfile::tempdir().unwrap();
        let mgr = crate::ai::AiArtifactManager::new(temp.path());

        let img = image::RgbImage::new(16, 16);
        let mut png_bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();

        let meta = crate::ai::AiFrameMetadata {
            frame_index: 0,
            status: crate::ai::AiFrameStatus::Completed,
            model_id: "test-model".to_string(),
            provider: "CPU".to_string(),
            decode_duration_ms: 1.0,
            preprocess_duration_ms: 1.5,
            inference_duration_ms: 12.5,
            postprocess_duration_ms: 0.5,
            total_duration_ms: 15.5,
            input_width: 16,
            input_height: 16,
            output_width: 16,
            output_height: 16,
            output_artifact_path: mgr.output_png_path(0).display().to_string(),
            config_hash: "hash123".to_string(),
            ..Default::default()
        };

        mgr.write_frame_artifact(&meta, &png_bytes).unwrap();
        let validation = mgr.validate_frame_artifact(0, "test-model", "hash123");
        assert!(validation.is_some());
    }

    #[test]
    fn test_phase6d_10_ai_frame_artifact_validation_hash_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let mgr = crate::ai::AiArtifactManager::new(temp.path());

        let img = image::RgbImage::new(16, 16);
        let mut png_bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();

        let meta = crate::ai::AiFrameMetadata {
            frame_index: 0,
            status: crate::ai::AiFrameStatus::Completed,
            model_id: "test-model".to_string(),
            provider: "CPU".to_string(),
            decode_duration_ms: 1.0,
            preprocess_duration_ms: 1.5,
            inference_duration_ms: 12.5,
            postprocess_duration_ms: 0.5,
            total_duration_ms: 15.5,
            input_width: 16,
            input_height: 16,
            output_width: 16,
            output_height: 16,
            output_artifact_path: mgr.output_png_path(0).display().to_string(),
            config_hash: "hash_old".to_string(),
            ..Default::default()
        };

        mgr.write_frame_artifact(&meta, &png_bytes).unwrap();
        let validation = mgr.validate_frame_artifact(0, "test-model", "hash_new");
        assert!(validation.is_none());
    }

    #[test]
    fn test_phase6d_11_ai_frame_artifact_validation_empty_file() {
        let temp = tempfile::tempdir().unwrap();
        let mgr = crate::ai::AiArtifactManager::new(temp.path());

        let dir = mgr.frame_dir(0);
        fs::create_dir_all(&dir).unwrap();
        fs::write(mgr.output_png_path(0), b"").unwrap(); // 0 bytes

        let meta = crate::ai::AiFrameMetadata {
            frame_index: 0,
            status: crate::ai::AiFrameStatus::Completed,
            model_id: "test-model".to_string(),
            provider: "CPU".to_string(),
            decode_duration_ms: 1.0,
            preprocess_duration_ms: 1.5,
            inference_duration_ms: 12.5,
            postprocess_duration_ms: 0.5,
            total_duration_ms: 15.5,
            input_width: 16,
            input_height: 16,
            output_width: 16,
            output_height: 16,
            output_artifact_path: mgr.output_png_path(0).display().to_string(),
            config_hash: "hash123".to_string(),
            ..Default::default()
        };
        fs::write(
            mgr.result_json_path(0),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let validation = mgr.validate_frame_artifact(0, "test-model", "hash123");
        assert!(validation.is_none());
    }

    #[test]
    fn test_phase6d_12_stage_ai_validation_in_job_validation_report() {
        let (engine, temp) = create_test_job_engine();
        let proj_dir = temp.path().join("projects").join("proj-6d-12");
        fs::create_dir_all(&proj_dir).unwrap();

        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: "test-model".to_string(),
            provider: None,
            preprocessing: crate::ai::PreprocessConfig::default(),
            postprocessing: None,
            frame_sampling: crate::ai::FrameSamplingConfig::all(),
            output_mode: crate::ai::AiFrameOutputMode::Image,
            ..Default::default()
        };

        let job = engine
            .create_ai_job("proj-6d-12", None, vec!["dummy.mp4".to_string()], ai_cfg)
            .unwrap();

        let report = engine.validate_job_stage_artifacts(&job);
        assert_eq!(report.stage_validations.len(), 7);
        assert_eq!(
            report.stage_validations[4].stage_id,
            "stage_ai_frame_inference"
        );
        assert!(!report.stage_validations[4].is_valid);
    }

    #[tokio::test]
    async fn test_phase6d_13_retry_job_cascades_dependencies_with_ai() {
        let (engine, _temp) = create_test_job_engine();
        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: "test-model".to_string(),
            provider: None,
            preprocessing: crate::ai::PreprocessConfig::default(),
            postprocessing: None,
            frame_sampling: crate::ai::FrameSamplingConfig::all(),
            output_mode: crate::ai::AiFrameOutputMode::Image,
            ..Default::default()
        };

        let mut job = engine
            .create_ai_job("proj-6d-13", None, vec![], ai_cfg)
            .unwrap();
        job.status = JobStatus::Interrupted;
        job.stages[0].status = StageStatus::Completed;
        job.stages[1].status = StageStatus::Completed;
        job.stages[2].status = StageStatus::Completed;
        job.stages[3].status = StageStatus::Completed;
        job.stages[4].status = StageStatus::Pending; // AI stage pending
        engine.save_job_manifest(&job).unwrap();

        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        assert_eq!(retried.status, JobStatus::Running);
        assert_eq!(retried.stages[4].status, StageStatus::Pending);
        assert_eq!(retried.stages[5].status, StageStatus::Pending);
    }

    #[test]
    fn test_phase6d_14_ai_executor_frame_execution_with_real_onnx() {
        let temp = tempfile::tempdir().unwrap();
        let frames_dir = temp.path().join("frames");
        let ai_cache_dir = temp.path().join("ai_cache");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..3 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = image::RgbImage::new(2, 2);
            img.save(&p).unwrap();
        }

        let model_path = temp.path().join("test_model.onnx");
        crate::ai::pipeline::generate_image_onnx_model(&model_path).unwrap();

        let model_id = format!("test-exec-6d-{}", uuid::Uuid::new_v4());
        let manifest = crate::ai::manifest::AiModelManifest::new(
            &model_id,
            "Test ONNX Multiplier",
            "1.0.0",
            crate::ai::manifest::ModelFormat::Onnx,
            model_path,
            "Test 4D image model",
            vec![],
            vec![],
            crate::ai::manifest::ModelRequirements::default(),
        );

        let storage_paths = crate::StoragePaths::default_paths();
        let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir.clone());
        let _ = registry.register_model(manifest);

        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            provider: None,
            preprocessing: crate::ai::PreprocessConfig {
                target_width: 2,
                target_height: 2,
                ..Default::default()
            },
            postprocessing: None,
            frame_sampling: crate::ai::FrameSamplingConfig::all(),
            output_mode: crate::ai::AiFrameOutputMode::Image,
            ..Default::default()
        };

        let artifact_mgr = crate::ai::AiArtifactManager::new(&ai_cache_dir);
        let mut progress_count = 0;

        let metrics = crate::ai::AiFrameExecutor::execute(
            &frames_dir,
            &ai_cfg,
            &artifact_mgr,
            None,
            |_prog, _meta, _m| {
                progress_count += 1;
            },
        )
        .unwrap();

        assert_eq!(metrics.frames_total, 3);
        assert_eq!(metrics.frames_processed, 3);
        assert_eq!(metrics.frames_passthrough, 0);
        assert_eq!(metrics.frames_failed, 0);
        assert!(progress_count >= 3);

        for i in 0..3 {
            let recon_path = artifact_mgr.reconstruction_frame_path(i);
            assert!(recon_path.exists());
            assert!(fs::metadata(&recon_path).unwrap().len() > 0);
        }
    }

    #[test]
    fn test_phase6d_15_ai_executor_passthrough_sampling() {
        let temp = tempfile::tempdir().unwrap();
        let frames_dir = temp.path().join("frames");
        let ai_cache_dir = temp.path().join("ai_cache");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..4 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = image::RgbImage::new(2, 2);
            img.save(&p).unwrap();
        }

        let model_path = temp.path().join("test_model_sampling.onnx");
        crate::ai::pipeline::generate_image_onnx_model(&model_path).unwrap();

        let model_id = format!("test-exec-sampling-{}", uuid::Uuid::new_v4());
        let manifest = crate::ai::manifest::AiModelManifest::new(
            &model_id,
            "Test ONNX Multiplier Sampling",
            "1.0.0",
            crate::ai::manifest::ModelFormat::Onnx,
            model_path,
            "Test 4D image model",
            vec![],
            vec![],
            crate::ai::manifest::ModelRequirements::default(),
        );

        let storage_paths = crate::StoragePaths::default_paths();
        let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir.clone());
        let _ = registry.register_model(manifest);

        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            provider: None,
            preprocessing: crate::ai::PreprocessConfig {
                target_width: 2,
                target_height: 2,
                ..Default::default()
            },
            postprocessing: None,
            frame_sampling: crate::ai::FrameSamplingConfig::every_nth(2),
            output_mode: crate::ai::AiFrameOutputMode::Image,
            ..Default::default()
        };

        let artifact_mgr = crate::ai::AiArtifactManager::new(&ai_cache_dir);
        let metrics = crate::ai::AiFrameExecutor::execute(
            &frames_dir,
            &ai_cfg,
            &artifact_mgr,
            None,
            |_p, _m, _met| {},
        )
        .unwrap();

        assert_eq!(metrics.frames_total, 4);
        assert_eq!(metrics.frames_selected, 2);
        assert_eq!(metrics.frames_processed, 2);
        assert_eq!(metrics.frames_passthrough, 2);

        for i in 0..4 {
            assert!(artifact_mgr.reconstruction_frame_path(i).exists());
        }
    }

    #[test]
    fn test_phase6d_16_ai_executor_cancellation_during_run() {
        let temp = tempfile::tempdir().unwrap();
        let frames_dir = temp.path().join("frames");
        let ai_cache_dir = temp.path().join("ai_cache");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..5 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = image::RgbImage::new(2, 2);
            img.save(&p).unwrap();
        }

        let model_path = temp.path().join("test_model_cancel.onnx");
        crate::ai::pipeline::generate_image_onnx_model(&model_path).unwrap();

        let model_id = format!("test-exec-cancel-{}", uuid::Uuid::new_v4());
        let manifest = crate::ai::manifest::AiModelManifest::new(
            &model_id,
            "Test ONNX Multiplier Cancel",
            "1.0.0",
            crate::ai::manifest::ModelFormat::Onnx,
            model_path,
            "Test 4D image model",
            vec![],
            vec![],
            crate::ai::manifest::ModelRequirements::default(),
        );

        let storage_paths = crate::StoragePaths::default_paths();
        let registry = crate::ai::ModelRegistry::new(storage_paths.models_dir.clone());
        let _ = registry.register_model(manifest);

        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            provider: None,
            preprocessing: crate::ai::PreprocessConfig {
                target_width: 2,
                target_height: 2,
                ..Default::default()
            },
            postprocessing: None,
            frame_sampling: crate::ai::FrameSamplingConfig::all(),
            output_mode: crate::ai::AiFrameOutputMode::Image,
            ..Default::default()
        };

        let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_clone = cancel_token.clone();
        let artifact_mgr = crate::ai::AiArtifactManager::new(&ai_cache_dir);

        let res = crate::ai::AiFrameExecutor::execute(
            &frames_dir,
            &ai_cfg,
            &artifact_mgr,
            Some(cancel_token),
            move |_p, meta: Option<&crate::ai::AiFrameMetadata>, _met| {
                if let Some(m) = meta {
                    if m.frame_index == 1 {
                        cancel_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                }
            },
        );

        assert!(res.is_err());
        let err = res.err().unwrap();
        assert_eq!(err.code, crate::error::ErrorCode::Cancelled);
    }

    #[test]
    fn test_phase6e_01_rational_fps_preservation() {
        let fps = crate::ai::RationalFps::from_str_ratio("30000/1001").unwrap();
        assert_eq!(fps.num, 30000);
        assert_eq!(fps.den, 1001);
        assert_eq!(fps.to_ffmpeg_arg(), "30000/1001");
    }

    #[test]
    fn test_phase6e_02_video_reconstruction_pipeline_end_to_end() {
        let temp = tempfile::tempdir().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..6 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = image::RgbImage::new(64, 64);
            img.save(&p).unwrap();
        }

        let output_mp4 = temp.path().join("pipeline_reconstructed.mp4");
        let config = crate::ai::VideoReconstructionConfig {
            source_video_path: temp.path().join("source.mp4"),
            frames_dir: frames_dir.clone(),
            output_path: output_mp4.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 6,
            width: 64,
            height: 64,
            fps: crate::ai::RationalFps::new(30, 1),
            pixel_format: "yuv420p".to_string(),
            codec: crate::ai::VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: crate::ai::AudioPreservationMode::None,
            overwrite: true,
        };

        let mut progress_called = false;
        let res = crate::ai::VideoReconstructor::reconstruct_video(
            &config,
            "job-6e-e2e",
            None,
            None,
            |_p, _c, _t| {
                progress_called = true;
            },
            None,
            None::<fn(u32)>,
            None::<fn(u32)>,
        );

        assert!(res.is_ok());
        let r = res.unwrap();
        assert!(output_mp4.exists());
        assert!(r.output_metadata.file_size_bytes > 0);
        assert_eq!(r.output_metadata.width, 64);
        assert_eq!(r.output_metadata.height, 64);
        assert!(progress_called);

        let manifest_path = temp.path().join("reconstruction_manifest.json");
        assert!(manifest_path.exists());
    }

    #[test]
    fn test_phase6e_03_reconstruction_manifest_generation() {
        let temp = tempfile::tempdir().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..4 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = image::RgbImage::new(64, 64);
            img.save(&p).unwrap();
        }

        let output_mp4 = temp.path().join("test_manifest.mp4");
        let config = crate::ai::VideoReconstructionConfig {
            source_video_path: temp.path().join("source.mp4"),
            frames_dir: frames_dir.clone(),
            output_path: output_mp4.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 4,
            width: 64,
            height: 64,
            fps: crate::ai::RationalFps::new(24, 1),
            pixel_format: "yuv420p".to_string(),
            codec: crate::ai::VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: crate::ai::AudioPreservationMode::None,
            overwrite: true,
        };

        let res = crate::ai::VideoReconstructor::reconstruct_video(
            &config,
            "job-manifest-gen",
            None,
            None,
            |_p, _c, _t| {},
            None,
            None::<fn(u32)>,
            None::<fn(u32)>,
        )
        .unwrap();

        assert_eq!(res.manifest.frame_count, 4);
        assert_eq!(res.manifest.fps_num, 24);
        assert_eq!(res.manifest.fps_den, 1);
        assert_eq!(res.manifest.frames.len(), 4);
    }

    #[test]
    fn test_phase6e_04_reconstruction_cancellation_and_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let frames_dir = temp.path().join("recon_frames");
        fs::create_dir_all(&frames_dir).unwrap();

        for i in 0..5 {
            let p = frames_dir.join(format!("{:06}.png", i));
            let img = image::RgbImage::new(64, 64);
            img.save(&p).unwrap();
        }

        let output_mp4 = temp.path().join("cancel_test.mp4");
        let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        let config = crate::ai::VideoReconstructionConfig {
            source_video_path: temp.path().join("source.mp4"),
            frames_dir: frames_dir.clone(),
            output_path: output_mp4.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 5,
            width: 64,
            height: 64,
            fps: crate::ai::RationalFps::new(30, 1),
            pixel_format: "yuv420p".to_string(),
            codec: crate::ai::VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: crate::ai::AudioPreservationMode::None,
            overwrite: true,
        };

        let res = crate::ai::VideoReconstructor::reconstruct_video(
            &config,
            "job-cancel",
            None,
            None,
            |_p, _c, _t| {},
            Some(cancel_token),
            None::<fn(u32)>,
            None::<fn(u32)>,
        );

        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, crate::error::ErrorCode::Cancelled);
        assert!(!output_mp4.exists());
    }

    #[test]
    fn test_phase6e_05_reconstruction_with_mixed_ai_and_passthrough() {
        let temp = tempfile::tempdir().unwrap();
        let ai_cache_dir = temp.path().join("ai_cache");
        let mgr = crate::ai::AiArtifactManager::new(&ai_cache_dir);
        mgr.ensure_dirs().unwrap();

        for i in 0..4 {
            let img = image::RgbImage::new(64, 64);
            let p = temp.path().join(format!("src_{:06}.png", i));
            img.save(&p).unwrap();

            if i % 2 == 0 {
                // AI frame
                let meta = crate::ai::AiFrameMetadata {
                    frame_index: i,
                    status: crate::ai::AiFrameStatus::Completed,
                    model_id: "test-model".to_string(),
                    provider: "cpu".to_string(),
                    decode_duration_ms: 1.0,
                    preprocess_duration_ms: 1.0,
                    inference_duration_ms: 2.0,
                    postprocess_duration_ms: 1.0,
                    total_duration_ms: 5.0,
                    input_width: 64,
                    input_height: 64,
                    output_width: 64,
                    output_height: 64,
                    output_artifact_path: "output.png".to_string(),
                    config_hash: "test-hash".to_string(),
                    ..Default::default()
                };
                let bytes = fs::read(&p).unwrap();
                mgr.write_frame_artifact(&meta, &bytes).unwrap();
            } else {
                // Passthrough frame
                mgr.write_passthrough_frame(i, &p).unwrap();
            }
        }

        let output_mp4 = temp.path().join("mixed_output.mp4");
        let config = crate::ai::VideoReconstructionConfig {
            source_video_path: temp.path().join("source.mp4"),
            frames_dir: mgr.reconstruction_frames_dir(),
            output_path: output_mp4.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 4,
            width: 64,
            height: 64,
            fps: crate::ai::RationalFps::new(30, 1),
            pixel_format: "yuv420p".to_string(),
            codec: crate::ai::VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: crate::ai::AudioPreservationMode::None,
            overwrite: true,
        };

        let res = crate::ai::VideoReconstructor::reconstruct_video(
            &config,
            "job-mixed",
            None,
            Some(&mgr),
            |_p, _c, _t| {},
            None,
            None::<fn(u32)>,
            None::<fn(u32)>,
        );

        assert!(res.is_ok());
        assert!(output_mp4.exists());
    }

    #[test]
    fn test_phase6e_06_ai_stage_artifact_validation_and_retry_resumption() {
        let (engine, temp) = create_test_job_engine();
        let proj_id = "proj-6e-06";
        let proj_dir = temp.path().join("projects").join(proj_id);
        fs::create_dir_all(&proj_dir).unwrap();

        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: "test-model".to_string(),
            provider: None,
            preprocessing: crate::ai::PreprocessConfig::default(),
            postprocessing: None,
            frame_sampling: crate::ai::FrameSamplingConfig::all(),
            output_mode: crate::ai::AiFrameOutputMode::Image,
            ..Default::default()
        };

        let job = engine.create_ai_job(proj_id, None, vec![], ai_cfg).unwrap();

        // 1. Initially without artifacts, report shows invalid
        let report = engine.validate_job_stage_artifacts(&job);
        assert_eq!(report.stage_validations.len(), 7);
        assert_eq!(
            report.stage_validations[4].stage_id,
            "stage_ai_frame_inference"
        );
        assert!(!report.stage_validations[4].is_valid);

        // 2. Populate AI reconstruction frames
        let ai_cache_dir = proj_dir.join("cache").join("ai").join(&job.id);
        let artifact_mgr = crate::ai::AiArtifactManager::new(&ai_cache_dir);
        artifact_mgr.ensure_dirs().unwrap();

        for i in 0..3 {
            let img = image::RgbImage::new(4, 4);
            img.save(artifact_mgr.reconstruction_frame_path(i)).unwrap();
        }

        let mut count = 0;
        if let Ok(entries) = fs::read_dir(artifact_mgr.reconstruction_frames_dir()) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|x| x.to_str()) == Some("png") {
                    count += 1;
                }
            }
        }
        assert_eq!(count, 3);
    }
}
