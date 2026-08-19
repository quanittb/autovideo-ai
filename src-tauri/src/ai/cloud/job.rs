use super::cost::{CostEstimate, LatencyTelemetry};
use super::error::CloudProviderError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudJobState {
    Queued,
    Submitting,
    Processing,
    Downloading,
    Validating,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloudJobRequest {
    #[serde(alias = "job_id")]
    pub job_id: String,
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

pub struct CloudJobManager {
    jobs: RwLock<HashMap<String, CloudJobStatus>>,
}

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
            state: CloudJobState::Queued,
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
