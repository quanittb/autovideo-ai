use super::error::CloudProviderError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostStatus {
    Exact,
    Estimated,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub provider: String,
    pub model: String,
    pub estimated_usd: Option<f64>,
    pub min_usd: Option<f64>,
    pub max_usd: Option<f64>,
    pub confidence: f64,
    pub currency: String,
    pub status: CostStatus,
    pub breakdown: String,
}

impl Default for CostEstimate {
    fn default() -> Self {
        Self {
            provider: "replicate".to_string(),
            model: "minimax/video-01".to_string(),
            estimated_usd: None,
            min_usd: None,
            max_usd: None,
            confidence: 0.0,
            currency: "USD".to_string(),
            status: CostStatus::Unknown,
            breakdown: "Unconfigured cost estimate".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostGuard {
    pub max_cost_per_job: f64,
}

impl Default for CostGuard {
    fn default() -> Self {
        Self {
            max_cost_per_job: 5.0, // Conservative default
        }
    }
}

impl CostGuard {
    pub fn new(max_cost_per_job: f64) -> Self {
        Self { max_cost_per_job }
    }

    pub fn check(&self, estimate: &CostEstimate) -> Result<(), CloudProviderError> {
        if let Some(cost) = estimate.estimated_usd {
            if cost > self.max_cost_per_job {
                return Err(CloudProviderError::CostLimitExceeded {
                    estimated: cost,
                    limit: self.max_cost_per_job,
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyTelemetry {
    pub t0_request_started_ms: u64,
    pub t1_job_submitted_ms: Option<u64>,
    pub t2_provider_processing_ms: Option<u64>,
    pub t3_provider_completed_ms: Option<u64>,
    pub t4_download_completed_ms: Option<u64>,
    pub t5_validation_completed_ms: Option<u64>,
    pub submit_latency_sec: Option<f64>,
    pub generation_latency_sec: Option<f64>,
    pub download_latency_sec: Option<f64>,
    pub total_latency_sec: f64,
}

impl LatencyTelemetry {
    pub fn start() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            t0_request_started_ms: now,
            t1_job_submitted_ms: None,
            t2_provider_processing_ms: None,
            t3_provider_completed_ms: None,
            t4_download_completed_ms: None,
            t5_validation_completed_ms: None,
            submit_latency_sec: None,
            generation_latency_sec: None,
            download_latency_sec: None,
            total_latency_sec: 0.0,
        }
    }

    pub fn mark_submitted(&mut self) {
        let now = now_ms();
        self.t1_job_submitted_ms = Some(now);
        self.submit_latency_sec = Some((now - self.t0_request_started_ms) as f64 / 1000.0);
    }

    pub fn mark_processing(&mut self) {
        self.t2_provider_processing_ms = Some(now_ms());
    }

    pub fn mark_completed(&mut self) {
        let now = now_ms();
        self.t3_provider_completed_ms = Some(now);
        if let Some(t1) = self.t1_job_submitted_ms {
            self.generation_latency_sec = Some((now - t1) as f64 / 1000.0);
        }
    }

    pub fn mark_downloaded(&mut self) {
        let now = now_ms();
        self.t4_download_completed_ms = Some(now);
        if let Some(t3) = self.t3_provider_completed_ms {
            self.download_latency_sec = Some((now - t3) as f64 / 1000.0);
        }
    }

    pub fn mark_validated(&mut self) {
        let now = now_ms();
        self.t5_validation_completed_ms = Some(now);
        self.total_latency_sec = (now - self.t0_request_started_ms) as f64 / 1000.0;
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
