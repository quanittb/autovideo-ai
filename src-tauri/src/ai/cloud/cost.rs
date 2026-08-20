use super::error::CloudProviderError;
use serde::{Deserialize, Serialize};

pub const DEFAULT_PREVIEW_BUDGET_USD: f64 = 0.25;
pub const DEFAULT_STANDARD_JOB_BUDGET_USD: f64 = 3.00;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CostConfidence {
    #[serde(alias = "Exact", alias = "EXACT")]
    Exact,
    #[serde(alias = "Estimated", alias = "ESTIMATED")]
    Estimated,
    #[serde(alias = "Unknown", alias = "UNKNOWN")]
    Unknown,
}

pub type CostStatus = CostConfidence;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostBreakdown {
    pub provider_id: String,
    pub model_id: String,
    pub billable_duration_sec: f64,
    pub resolution: (u32, u32),
    #[serde(default)]
    pub resolution_tier: Option<String>,
    #[serde(default)]
    pub unit_rate_usd: Option<f64>,
    #[serde(default)]
    pub pricing_observed_at: Option<String>,
    pub segment_count: usize,
    pub overlap_duration_sec: f64,
    pub retry_allowance_usd: f64,
    pub inference_cost_usd: Option<f64>,
    pub transfer_storage_cost_usd: Option<f64>,
    pub total_usd: Option<f64>,
    pub confidence: CostConfidence,
    pub currency: String,
    pub breakdown: String,
}

impl Default for CostBreakdown {
    fn default() -> Self {
        Self {
            provider_id: "local_ffmpeg".to_string(),
            model_id: "ffmpeg_native".to_string(),
            billable_duration_sec: 0.0,
            resolution: (720, 1280),
            resolution_tier: None,
            unit_rate_usd: Some(0.0),
            pricing_observed_at: Some("2026-08-19".to_string()),
            segment_count: 1,
            overlap_duration_sec: 0.0,
            retry_allowance_usd: 0.0,
            inference_cost_usd: Some(0.0),
            transfer_storage_cost_usd: Some(0.0),
            total_usd: Some(0.0),
            confidence: CostConfidence::Exact,
            currency: "USD".to_string(),
            breakdown: "Free local processing".to_string(),
        }
    }
}

impl CostBreakdown {
    pub fn to_estimate(&self) -> CostEstimate {
        CostEstimate {
            provider: self.provider_id.clone(),
            model: self.model_id.clone(),
            estimated_usd: self.total_usd,
            min_usd: self.total_usd.map(|v| v * 0.9),
            max_usd: self.total_usd.map(|v| v * 1.2),
            confidence: match self.confidence {
                CostConfidence::Exact => 1.0,
                CostConfidence::Estimated => 0.85,
                CostConfidence::Unknown => 0.0,
            },
            currency: self.currency.clone(),
            status: self.confidence,
            breakdown: self.breakdown.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CostEstimate {
    pub provider: String,
    pub model: String,
    #[serde(alias = "estimated_usd")]
    pub estimated_usd: Option<f64>,
    #[serde(alias = "min_usd")]
    pub min_usd: Option<f64>,
    #[serde(alias = "max_usd")]
    pub max_usd: Option<f64>,
    pub confidence: f64,
    pub currency: String,
    pub status: CostConfidence,
    pub breakdown: String,
}

impl Default for CostEstimate {
    fn default() -> Self {
        Self {
            provider: "local_ffmpeg".to_string(),
            model: "ffmpeg_native".to_string(),
            estimated_usd: Some(0.0),
            min_usd: Some(0.0),
            max_usd: Some(0.0),
            confidence: 1.0,
            currency: "USD".to_string(),
            status: CostConfidence::Exact,
            breakdown: "Local deterministic compute ($0.00)".to_string(),
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
            max_cost_per_job: DEFAULT_STANDARD_JOB_BUDGET_USD,
        }
    }
}

impl CostGuard {
    pub fn new(max_cost_per_job: f64) -> Self {
        Self { max_cost_per_job }
    }

    pub fn preview_guard() -> Self {
        Self {
            max_cost_per_job: DEFAULT_PREVIEW_BUDGET_USD,
        }
    }

    pub fn standard_job_guard() -> Self {
        Self {
            max_cost_per_job: DEFAULT_STANDARD_JOB_BUDGET_USD,
        }
    }

    pub fn validate_budget(budget: f64) -> Result<f64, CloudProviderError> {
        if budget.is_nan() || budget.is_infinite() || budget < 0.0 {
            return Err(CloudProviderError::RequestInvalid(format!(
                "Invalid budget value: {} (must be a finite, non-negative number)",
                budget
            )));
        }
        Ok(budget)
    }

    pub fn check(&self, estimate: &CostEstimate) -> Result<(), CloudProviderError> {
        if estimate.status == CostConfidence::Unknown || estimate.estimated_usd.is_none() {
            return Err(CloudProviderError::RequestInvalid(
                "Unknown cost estimate cannot be auto-submitted. Budget verification requires explicit pricing."
                    .to_string(),
            ));
        }

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

    pub fn check_breakdown(&self, breakdown: &CostBreakdown) -> Result<(), CloudProviderError> {
        if breakdown.confidence == CostConfidence::Unknown || breakdown.total_usd.is_none() {
            return Err(CloudProviderError::RequestInvalid(
                "Unknown cost breakdown cannot be auto-submitted. Budget verification requires explicit pricing."
                    .to_string(),
            ));
        }

        if let Some(cost) = breakdown.total_usd {
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
