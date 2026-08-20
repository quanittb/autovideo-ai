use super::cost::{CostConfidence, CostEstimate, LatencyTelemetry};
use super::error::CloudProviderError;
use super::registry::ExecutionClass;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

pub const CURRENT_CLOUD_JOB_SCHEMA_VERSION: u32 = 1;

// -----------------------------------------------------------------------------
// 1. Canonical State & Validated Transitions
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CloudJobState {
    #[serde(alias = "Created", alias = "Queued", alias = "QUEUED")]
    Created,
    #[serde(alias = "Validating", alias = "VALIDATING")]
    Validating,
    #[serde(alias = "CostApprovalRequired", alias = "COST_APPROVAL_REQUIRED")]
    CostApprovalRequired,
    #[serde(alias = "Uploading", alias = "UPLOADING")]
    Uploading,
    #[serde(
        alias = "Submitted",
        alias = "Submitting",
        alias = "SUBMITTED",
        alias = "SUBMITTING"
    )]
    Submitted,
    #[serde(alias = "Processing", alias = "PROCESSING")]
    Processing,
    #[serde(alias = "Downloading", alias = "DOWNLOADING")]
    Downloading,
    #[serde(alias = "ValidatingOutput", alias = "VALIDATING_OUTPUT")]
    ValidatingOutput,
    #[serde(alias = "Completed", alias = "COMPLETED")]
    Completed,
    #[serde(alias = "Failed", alias = "FAILED")]
    Failed,
    #[serde(
        alias = "Cancelled",
        alias = "Canceled",
        alias = "CANCELLED",
        alias = "CANCELED"
    )]
    Cancelled,
    #[serde(alias = "Blocked", alias = "BLOCKED")]
    Blocked,
}

impl CloudJobState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub fn can_transition_to(&self, next: Self) -> bool {
        if *self == next {
            return true;
        }
        match (self, next) {
            (Self::Created, Self::Validating)
            | (Self::Created, Self::Blocked)
            | (Self::Created, Self::Failed)
            | (Self::Created, Self::Cancelled) => true,

            (Self::Validating, Self::CostApprovalRequired)
            | (Self::Validating, Self::Uploading)
            | (Self::Validating, Self::Submitted)
            | (Self::Validating, Self::Blocked)
            | (Self::Validating, Self::Failed)
            | (Self::Validating, Self::Cancelled) => true,

            (Self::CostApprovalRequired, Self::Uploading)
            | (Self::CostApprovalRequired, Self::Submitted)
            | (Self::CostApprovalRequired, Self::Blocked)
            | (Self::CostApprovalRequired, Self::Cancelled)
            | (Self::CostApprovalRequired, Self::Failed) => true,

            (Self::Uploading, Self::Submitted)
            | (Self::Uploading, Self::Blocked)
            | (Self::Uploading, Self::Failed)
            | (Self::Uploading, Self::Cancelled) => true,

            (Self::Submitted, Self::Processing)
            | (Self::Submitted, Self::Blocked)
            | (Self::Submitted, Self::Failed)
            | (Self::Submitted, Self::Cancelled) => true,

            (Self::Processing, Self::Downloading)
            | (Self::Processing, Self::Blocked)
            | (Self::Processing, Self::Failed)
            | (Self::Processing, Self::Cancelled) => true,

            (Self::Downloading, Self::ValidatingOutput)
            | (Self::Downloading, Self::Blocked)
            | (Self::Downloading, Self::Failed)
            | (Self::Downloading, Self::Cancelled) => true,

            (Self::ValidatingOutput, Self::Completed)
            | (Self::ValidatingOutput, Self::Blocked)
            | (Self::ValidatingOutput, Self::Failed) => true,

            // Terminal states cannot transition to anything
            (Self::Completed, _) => false,
            (Self::Cancelled, _) => false,
            (Self::Failed, _) => false,

            // Blocked state can transition to Validating / Submitted / Processing upon manual unblock/resume
            (Self::Blocked, Self::Validating)
            | (Self::Blocked, Self::Submitted)
            | (Self::Blocked, Self::Processing)
            | (Self::Blocked, Self::Cancelled)
            | (Self::Blocked, Self::Failed) => true,

            _ => false,
        }
    }
}

// -----------------------------------------------------------------------------
// 2. Submission State
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubmissionState {
    NeverAttempted,
    InFlight,
    Acknowledged,
    Ambiguous,
}

// -----------------------------------------------------------------------------
// 3. Sub-Records for PersistentCloudJob
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InputAssets {
    #[serde(default)]
    pub source_video_path: Option<PathBuf>,
    #[serde(default)]
    pub source_video_hash: Option<String>,
    #[serde(default)]
    pub reference_image_path: Option<PathBuf>,
    #[serde(default)]
    pub reference_image_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostRecord {
    #[serde(default)]
    pub estimate: Option<CostEstimate>,
    pub confidence: CostConfidence,
    pub budget_limit: f64,
    #[serde(default)]
    pub reserved_budget: Option<f64>,
    #[serde(default)]
    pub actual_cost: Option<f64>,
}

impl Default for CostRecord {
    fn default() -> Self {
        Self {
            estimate: None,
            confidence: CostConfidence::Unknown,
            budget_limit: 3.00,
            reserved_budget: None,
            actual_cost: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutputArtifactRecord {
    #[serde(default)]
    pub temporary_path: Option<PathBuf>,
    #[serde(default)]
    pub final_path: Option<PathBuf>,
    #[serde(default)]
    pub artifact_hash: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub duration_sec: Option<f64>,
    #[serde(default)]
    pub fps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RetryCounters {
    #[serde(default)]
    pub submit_attempts: u32,
    #[serde(default)]
    pub poll_attempts: u32,
    #[serde(default)]
    pub download_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobErrorRecord {
    pub code: String,
    pub sanitized_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobTimestamps {
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub submitted_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
}

impl Default for JobTimestamps {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            created_at: now.clone(),
            updated_at: now,
            submitted_at: None,
            completed_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ValidationPolicy {
    #[serde(default)]
    pub expected_duration_sec: Option<f64>,
    #[serde(default)]
    pub require_audio: bool,
}

// -----------------------------------------------------------------------------
// 4. Primary Persistent Record: PersistentCloudJob
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentCloudJob {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub state_revision: u64,

    pub job_id: String,
    pub internal_job_id: String,
    pub project_id: String,

    pub provider_id: String,
    pub model_id: String,
    pub model_version: String,

    pub task_type: String,
    pub execution_class: ExecutionClass,

    pub input_assets: InputAssets,
    pub configuration_hash: String,

    #[serde(default = "default_submission_state")]
    pub submission_state: SubmissionState,
    #[serde(default)]
    pub remote_job_id: Option<String>,
    pub state: CloudJobState,

    pub cost: CostRecord,
    pub output: OutputArtifactRecord,
    #[serde(default)]
    pub retry: RetryCounters,
    #[serde(default)]
    pub error: Option<JobErrorRecord>,
    pub timestamps: JobTimestamps,

    #[serde(default)]
    pub cancellation_requested: bool,
    #[serde(default)]
    pub progress_pct: Option<f64>,
    #[serde(default)]
    pub remote_status: Option<String>,
    #[serde(default)]
    pub output_url: Option<String>,

    #[serde(default)]
    pub validation_policy: ValidationPolicy,
}

fn default_schema_version() -> u32 {
    CURRENT_CLOUD_JOB_SCHEMA_VERSION
}

fn default_submission_state() -> SubmissionState {
    SubmissionState::NeverAttempted
}

impl PersistentCloudJob {
    pub fn new(
        job_id: String,
        internal_job_id: String,
        project_id: String,
        provider_id: String,
        model_id: String,
        model_version: String,
        task_type: String,
        execution_class: ExecutionClass,
        input_assets: InputAssets,
        configuration_hash: String,
        cost: CostRecord,
    ) -> Self {
        Self {
            schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
            state_revision: 1,
            job_id,
            internal_job_id,
            project_id,
            provider_id,
            model_id,
            model_version,
            task_type,
            execution_class,
            input_assets,
            configuration_hash,
            submission_state: SubmissionState::NeverAttempted,
            remote_job_id: None,
            state: CloudJobState::Created,
            cost,
            output: OutputArtifactRecord::default(),
            retry: RetryCounters::default(),
            error: None,
            timestamps: JobTimestamps::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: None,
            output_url: None,
            validation_policy: ValidationPolicy::default(),
        }
    }

    pub fn increment_revision(&mut self) {
        self.state_revision = self.state_revision.saturating_add(1);
        self.timestamps.updated_at = Utc::now().to_rfc3339();
    }

    pub fn to_event_payload(&self) -> CloudJobEventPayload {
        CloudJobEventPayload {
            job_id: self.job_id.clone(),
            internal_job_id: self.internal_job_id.clone(),
            project_id: self.project_id.clone(),
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            task_type: self.task_type.clone(),
            execution_class: self.execution_class,
            state: self.state,
            submission_state: self.submission_state,
            remote_job_id: self.remote_job_id.clone(),
            cost_estimate: self.cost.estimate.clone(),
            actual_cost: self.cost.actual_cost,
            budget_limit: self.cost.budget_limit,
            output_path: self
                .output
                .final_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            retry_counters: self.retry.clone(),
            error: self.error.clone(),
            created_at: self.timestamps.created_at.clone(),
            updated_at: self.timestamps.updated_at.clone(),
            submitted_at: self.timestamps.submitted_at.clone(),
            completed_at: self.timestamps.completed_at.clone(),
            cancellation_requested: self.cancellation_requested,
            progress_pct: self.progress_pct,
            remote_status: self.remote_status.clone(),
        }
    }

    pub fn to_legacy_status(&self) -> CloudJobStatus {
        CloudJobStatus {
            job_id: self.job_id.clone(),
            state: self.state,
            progress_pct: self.progress_pct.unwrap_or(0.0),
            remote_id: self.remote_job_id.clone(),
            remote_status: self.remote_status.clone(),
            error_message: self.error.as_ref().map(|e| e.sanitized_message.clone()),
            output_url: self.output_url.clone(),
            elapsed_seconds: 0.0,
            cost_estimate: self.cost.estimate.clone(),
            actual_cost: self.cost.actual_cost,
        }
    }
}

// -----------------------------------------------------------------------------
// 5. Safe Frontend Event Payload
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloudJobEventPayload {
    pub job_id: String,
    pub internal_job_id: String,
    pub project_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub task_type: String,
    pub execution_class: ExecutionClass,
    pub state: CloudJobState,
    pub submission_state: SubmissionState,
    pub remote_job_id: Option<String>,
    pub cost_estimate: Option<CostEstimate>,
    pub actual_cost: Option<f64>,
    pub budget_limit: f64,
    pub output_path: Option<String>,
    pub retry_counters: RetryCounters,
    pub error: Option<JobErrorRecord>,
    pub created_at: String,
    pub updated_at: String,
    pub submitted_at: Option<String>,
    pub completed_at: Option<String>,
    pub cancellation_requested: bool,
    pub progress_pct: Option<f64>,
    pub remote_status: Option<String>,
}

// -----------------------------------------------------------------------------
// 6. IPC Request & Status Types
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloudJobRequest {
    #[serde(alias = "job_id")]
    pub job_id: String,
    #[serde(default, alias = "project_id")]
    pub project_id: Option<String>,
    pub prompt: String,
    #[serde(alias = "negative_prompt")]
    pub negative_prompt: Option<String>,
    #[serde(alias = "source_video")]
    pub source_video: Option<PathBuf>,
    #[serde(alias = "reference_image")]
    pub reference_image: Option<PathBuf>,
    #[serde(alias = "duration_seconds")]
    pub duration_seconds: f64,
    pub fps: f64,
    pub resolution: (u32, u32),
    #[serde(alias = "task_type")]
    pub task_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloudJobStatus {
    #[serde(alias = "job_id")]
    pub job_id: String,
    pub state: CloudJobState,
    #[serde(alias = "progress_pct")]
    pub progress_pct: f64,
    #[serde(alias = "remote_id")]
    pub remote_id: Option<String>,
    #[serde(alias = "remote_status")]
    pub remote_status: Option<String>,
    #[serde(alias = "error_message")]
    pub error_message: Option<String>,
    #[serde(alias = "output_url")]
    pub output_url: Option<String>,
    #[serde(alias = "elapsed_seconds")]
    pub elapsed_seconds: f64,
    #[serde(alias = "cost_estimate")]
    pub cost_estimate: Option<CostEstimate>,
    #[serde(alias = "actual_cost")]
    pub actual_cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudJobResult {
    #[serde(alias = "job_id")]
    pub job_id: String,
    pub provider: String,
    pub model: String,
    #[serde(alias = "output_mp4_path")]
    pub output_mp4_path: PathBuf,
    #[serde(alias = "duration_sec")]
    pub duration_sec: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    #[serde(alias = "cost_usd")]
    pub cost_usd: Option<f64>,
    pub latency: LatencyTelemetry,
    #[serde(alias = "metadata_json_path")]
    pub metadata_json_path: PathBuf,
}

#[deprecated(note = "Superseded by PersistentCloudJobStore and CloudJobLifecycleService")]
pub struct CloudJobManager {
    jobs: RwLock<HashMap<String, CloudJobStatus>>,
}

#[allow(deprecated)]
impl CloudJobManager {
    pub fn new() -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
        }
    }

    pub fn register_job(
        &self,
        job_id: &str,
        _req: &CloudJobRequest,
        cost_est: Option<CostEstimate>,
    ) {
        let status = CloudJobStatus {
            job_id: job_id.to_string(),
            state: CloudJobState::Created,
            progress_pct: 0.0,
            remote_id: None,
            remote_status: Some("queued".to_string()),
            error_message: None,
            output_url: None,
            elapsed_seconds: 0.0,
            cost_estimate: cost_est,
            actual_cost: None,
        };
        if let Ok(mut lock) = self.jobs.write() {
            lock.insert(job_id.to_string(), status);
        }
    }

    pub fn update_state(&self, job_id: &str, state: CloudJobState, progress_pct: f64) {
        if let Ok(mut lock) = self.jobs.write() {
            if let Some(j) = lock.get_mut(job_id) {
                j.state = state;
                j.progress_pct = progress_pct;
            }
        }
    }

    pub fn set_remote_info(
        &self,
        job_id: &str,
        remote_id: &str,
        remote_status: &str,
        output_url: Option<&str>,
    ) {
        if let Ok(mut lock) = self.jobs.write() {
            if let Some(j) = lock.get_mut(job_id) {
                j.remote_id = Some(remote_id.to_string());
                j.remote_status = Some(remote_status.to_string());
                if let Some(url) = output_url {
                    j.output_url = Some(url.to_string());
                }
            }
        }
    }

    pub fn mark_failed(&self, job_id: &str, err: &str) {
        if let Ok(mut lock) = self.jobs.write() {
            if let Some(j) = lock.get_mut(job_id) {
                j.state = CloudJobState::Failed;
                j.error_message = Some(err.to_string());
            }
        }
    }

    pub fn mark_cancelled(&self, job_id: &str) {
        if let Ok(mut lock) = self.jobs.write() {
            if let Some(j) = lock.get_mut(job_id) {
                j.state = CloudJobState::Cancelled;
            }
        }
    }

    pub fn get_status(&self, job_id: &str) -> Result<CloudJobStatus, CloudProviderError> {
        let lock = self
            .jobs
            .read()
            .map_err(|_| CloudProviderError::ProviderUnavailable("Lock poisoned".to_string()))?;
        lock.get(job_id)
            .cloned()
            .ok_or_else(|| CloudProviderError::RequestInvalid(format!("Job {} not found", job_id)))
    }
}
