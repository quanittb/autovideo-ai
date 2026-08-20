use crate::ai::cloud::cost::CostEstimate;
use crate::ai::cloud::error::CloudProviderError;
use crate::ai::cloud::job::CloudJobRequest;
use crate::ai::cloud::provider::{
    CloudJobHandle, CloudVideoProvider, ProviderCapabilities, RemotePollResponse, RemoteStatus,
};
use serde_json::json;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

pub struct ReplicateProvider {
    client: reqwest::Client,
    api_token: Option<String>,
    model_version: String,
}

impl ReplicateProvider {
    pub fn new() -> Self {
        let token = std::env::var("REPLICATE_API_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_token: token,
            model_version: "minimax/video-01".to_string(),
        }
    }

    pub fn with_token(token: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_token: Some(token.to_string()),
            model_version: "minimax/video-01".to_string(),
        }
    }
}

impl Default for ReplicateProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudVideoProvider for ReplicateProvider {
    fn provider_id(&self) -> &str {
        "replicate"
    }

    fn model_id(&self) -> &str {
        &self.model_version
    }

    fn model_version_hint(&self) -> Option<&str> {
        Some(&self.model_version)
    }

    fn provider_name(&self) -> &str {
        "Replicate Cloud Engine"
    }

    fn is_configured(&self) -> bool {
        self.api_token.is_some()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_text_to_video: true,
            supports_image_to_video: false,
            supports_video_to_video: false,
            supports_reference_image: false,
            supports_character_reference: false,
            supports_audio: false,
            max_duration_sec: Some(10.0),
            supported_resolutions: vec![(512, 512), (720, 1280), (1080, 1920)],
            estimated_cost_per_second: None,
        }
    }

    fn estimate_cost(&self, req: &CloudJobRequest) -> CostEstimate {
        let registry = crate::ai::cloud::ProviderRegistry::new();
        let dur = if req.duration_seconds <= 0.0 {
            6.0
        } else {
            req.duration_seconds
        };

        if let Some(record) = registry.find(self.provider_id(), self.model_id()) {
            let seg_len = record.max_duration_sec.unwrap_or(6.0).min(6.0);
            let segment_count = ((dur / seg_len).ceil() as usize).max(1);
            let (inf_cost, confidence) = match (record.pricing_unit, record.pricing_amount) {
                (crate::ai::cloud::PricingUnit::PerPrediction, Some(fee)) => (
                    Some(fee * segment_count as f64),
                    crate::ai::cloud::CostConfidence::Estimated,
                ),
                (crate::ai::cloud::PricingUnit::PerSecond, Some(rate)) => (
                    Some(rate * dur),
                    crate::ai::cloud::CostConfidence::Estimated,
                ),
                (crate::ai::cloud::PricingUnit::FreeLocal, Some(0.0)) => {
                    (Some(0.0), crate::ai::cloud::CostConfidence::Exact)
                }
                _ => (None, crate::ai::cloud::CostConfidence::Unknown),
            };

            CostEstimate {
                provider: self.provider_id().to_string(),
                model: record.model_id.clone(),
                estimated_usd: inf_cost,
                min_usd: inf_cost.map(|v| v * 0.9),
                max_usd: inf_cost.map(|v| v * 1.2),
                confidence: if self.is_configured() {
                    match confidence {
                        crate::ai::cloud::CostConfidence::Exact => 1.0,
                        crate::ai::cloud::CostConfidence::Estimated => 0.85,
                        crate::ai::cloud::CostConfidence::Unknown => 0.0,
                    }
                } else {
                    0.0
                },
                currency: record.currency.clone(),
                status: if self.is_configured() {
                    confidence
                } else {
                    crate::ai::cloud::CostConfidence::Unknown
                },
                breakdown: format!(
                    "Provider: {} | Rate: {:?} ${:?} | Dur: {:.1}s ({} segs)",
                    record.provider_id,
                    record.pricing_unit,
                    record.pricing_amount,
                    dur,
                    segment_count
                ),
            }
        } else {
            CostEstimate::default()
        }
    }

    fn submit_job(
        &self,
        request: &CloudJobRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CloudJobHandle, CloudProviderError>> + Send + '_>> {
        let token = match &self.api_token {
            Some(t) => t.clone(),
            None => {
                return Box::pin(async {
                    Err(CloudProviderError::AuthFailed(
                        "REPLICATE_API_TOKEN environment variable is not configured".to_string(),
                    ))
                });
            }
        };

        let client = self.client.clone();
        let prompt = request.prompt.clone();
        let job_id = request.job_id.clone();
        let model = self.model_version.clone();

        Box::pin(async move {
            let body = json!({
                "version": "minimax/video-01",
                "input": {
                    "prompt": prompt,
                    "prompt_optimizer": true
                }
            });

            let response = client
                .post("https://api.replicate.com/v1/predictions")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| CloudProviderError::NetworkError(e.to_string()))?;

            let status = response.status();
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(CloudProviderError::AuthFailed(
                    "Replicate API token is invalid or expired".to_string(),
                ));
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(CloudProviderError::RateLimited(
                    "Replicate 429 rate limit exceeded".to_string(),
                ));
            }
            if !status.is_success() {
                let err_text = response.text().await.unwrap_or_default();
                return Err(CloudProviderError::RequestInvalid(format!(
                    "HTTP {}: {}",
                    status, err_text
                )));
            }

            let resp_json: serde_json::Value = response
                .json()
                .await
                .map_err(|e| CloudProviderError::RequestInvalid(e.to_string()))?;

            let remote_id = resp_json["id"]
                .as_str()
                .ok_or_else(|| {
                    CloudProviderError::RequestInvalid(
                        "Missing 'id' in Replicate response".to_string(),
                    )
                })?
                .to_string();

            Ok(CloudJobHandle {
                job_id,
                remote_id,
                provider_id: "replicate".to_string(),
                model: model.clone(),
                model_version: Some(model),
            })
        })
    }

    fn poll_status(
        &self,
        remote_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<RemotePollResponse, CloudProviderError>> + Send + '_>>
    {
        let token = match &self.api_token {
            Some(t) => t.clone(),
            None => {
                return Box::pin(async {
                    Err(CloudProviderError::AuthFailed(
                        "REPLICATE_API_TOKEN is not configured".to_string(),
                    ))
                });
            }
        };

        let client = self.client.clone();
        let r_id = remote_id.to_string();

        Box::pin(async move {
            let url = format!("https://api.replicate.com/v1/predictions/{}", r_id);
            let response = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
                .map_err(|e| CloudProviderError::NetworkError(e.to_string()))?;

            if !response.status().is_success() {
                let status = response.status();
                return Err(CloudProviderError::NetworkError(format!(
                    "Failed to poll status: HTTP {}",
                    status
                )));
            }

            let resp_json: serde_json::Value = response
                .json()
                .await
                .map_err(|e| CloudProviderError::RequestInvalid(e.to_string()))?;

            let status_str = resp_json["status"].as_str().unwrap_or("unknown");
            let remote_status = match status_str {
                "starting" => RemoteStatus::Starting,
                "processing" => RemoteStatus::Processing,
                "succeeded" => RemoteStatus::Succeeded,
                "failed" => RemoteStatus::Failed,
                "canceled" => RemoteStatus::Canceled,
                _ => RemoteStatus::Processing,
            };

            let output_url = if let Some(out_str) = resp_json["output"].as_str() {
                Some(out_str.to_string())
            } else if let Some(out_arr) = resp_json["output"].as_array() {
                out_arr
                    .first()
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            };

            let error = resp_json["error"].as_str().map(|s| s.to_string());

            Ok(RemotePollResponse {
                remote_id: r_id,
                status: remote_status,
                output_url,
                error,
            })
        })
    }

    fn cancel_job(
        &self,
        remote_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), CloudProviderError>> + Send + '_>> {
        let token = match &self.api_token {
            Some(t) => t.clone(),
            None => {
                return Box::pin(async {
                    Err(CloudProviderError::AuthFailed(
                        "REPLICATE_API_TOKEN is not configured".to_string(),
                    ))
                });
            }
        };

        let client = self.client.clone();
        let r_id = remote_id.to_string();

        Box::pin(async move {
            let url = format!("https://api.replicate.com/v1/predictions/{}/cancel", r_id);
            let _ = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await;
            Ok(())
        })
    }

    fn download_result(
        &self,
        output_url: &str,
        target_path: &Path,
    ) -> Pin<Box<dyn Future<Output = Result<PathBuf, CloudProviderError>> + Send + '_>> {
        let client = self.client.clone();
        let url = output_url.to_string();
        let dest = target_path.to_path_buf();

        Box::pin(async move {
            let response = client
                .get(&url)
                .send()
                .await
                .map_err(|e| CloudProviderError::DownloadFailed(e.to_string()))?;

            if !response.status().is_success() {
                return Err(CloudProviderError::DownloadFailed(format!(
                    "Download failed with HTTP {}",
                    response.status()
                )));
            }

            let bytes = response
                .bytes()
                .await
                .map_err(|e| CloudProviderError::DownloadFailed(e.to_string()))?;

            if bytes.is_empty() {
                return Err(CloudProviderError::OutputInvalid(
                    "Downloaded 0 bytes from output URL".to_string(),
                ));
            }

            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| CloudProviderError::DownloadFailed(e.to_string()))?;
            }

            tokio::fs::write(&dest, bytes)
                .await
                .map_err(|e| CloudProviderError::DownloadFailed(e.to_string()))?;

            Ok(dest)
        })
    }
}
