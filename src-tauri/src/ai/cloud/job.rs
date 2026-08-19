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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudJobRequest {
    pub job_id: String,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub source_video: Option<PathBuf>,
    pub reference_image: Option<PathBuf>,
    pub duration_seconds: f64,
    pub fps: f64,
    pub resolution: (u32, u32),
    pub task_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudJobStatus {
    pub job_id: String,
    pub state: CloudJobState,
    pub progress_pct: f64,
    pub remote_id: Option<String>,
    pub remote_status: Option<String>,
    pub error_message: Option<String>,
    pub output_url: Option<String>,
    pub elapsed_seconds: f64,
    pub cost_estimate: Option<CostEstimate>,
    pub actual_cost: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudJobResult {
    pub job_id: String,
    pub provider: String,
    pub model: String,
    pub output_mp4_path: PathBuf,
    pub duration_sec: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub cost_usd: Option<f64>,
    pub latency: LatencyTelemetry,
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

    pub fn mark_failed(&self, job_id: &str, err: &CloudProviderError) {
        if let Ok(mut lock) = self.jobs.write() {
            if let Some(j) = lock.get_mut(job_id) {
                j.state = CloudJobState::Failed;
                j.error_message = Some(format!("{}", err));
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

    pub fn get_status(&self, job_id: &str) -> Option<CloudJobStatus> {
        let lock = self.jobs.read().ok()?;
        lock.get(job_id).cloned()
    }
}

impl Default for CloudJobManager {
    fn default() -> Self {
        Self::new()
    }
}
