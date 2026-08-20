use crate::ai::cloud::cost::{CostConfidence, CostEstimate};
use crate::ai::cloud::error::CloudProviderError;
use crate::ai::cloud::job::CloudJobRequest;
use crate::ai::cloud::live_execution_guard::{EnvLiveExecutionPolicy, LiveExecutionPolicy};
use crate::ai::cloud::provider::{
    CloudJobHandle, CloudVideoProvider, ProviderCapabilities, RemotePollResponse, RemoteStatus,
    ResolutionTier,
};
use crate::ai::cloud::spec::PreparedProviderSubmission;
use serde_json::json;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

pub struct PrunaPVideoReplaceProvider {
    client: reqwest::Client,
    api_token: Option<String>,
    live_policy: Arc<dyn LiveExecutionPolicy>,
}

impl PrunaPVideoReplaceProvider {
    pub fn new() -> Self {
        let token = std::env::var("REPLICATE_API_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none()) // We validate redirects explicitly
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_token: token,
            live_policy: Arc::new(EnvLiveExecutionPolicy),
        }
    }

    pub fn with_policy(
        api_token: Option<String>,
        live_policy: Arc<dyn LiveExecutionPolicy>,
    ) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_token,
            live_policy,
        }
    }

    pub fn validate_ssrf_url(raw_url: &str) -> Result<reqwest::Url, CloudProviderError> {
        let parsed = reqwest::Url::parse(raw_url).map_err(|e| {
            CloudProviderError::SecurityViolation(format!("Malformed output URL: {}", e))
        })?;

        if parsed.scheme() != "https" {
            return Err(CloudProviderError::SecurityViolation(format!(
                "SSRF_VIOLATION: Output URL must use HTTPS (scheme: {})",
                parsed.scheme()
            )));
        }

        let host_str = parsed.host_str().ok_or_else(|| {
            CloudProviderError::SecurityViolation("Missing host in output URL".to_string())
        })?;

        let host_lower = host_str.to_lowercase();

        // Strictly allow only replicate.delivery and its subdomains (*.replicate.delivery)
        let is_allowed_delivery_host = host_lower == "replicate.delivery"
            || (host_lower.ends_with(".replicate.delivery") && !host_lower.starts_with('.'));

        if !is_allowed_delivery_host {
            return Err(CloudProviderError::SecurityViolation(format!(
                "SSRF_VIOLATION: Output host '{}' is not in allowed Replicate delivery domains",
                host_str
            )));
        }

        // Additional safeguard: verify host is not an IP address (especially private/loopback)
        if host_lower == "localhost"
            || host_lower.starts_with("127.")
            || host_lower.starts_with("10.")
            || host_lower.starts_with("192.168.")
            || host_lower.starts_with("169.254.")
            || host_lower.starts_with("172.")
            || host_lower.contains("::1")
        {
            return Err(CloudProviderError::SecurityViolation(format!(
                "SSRF_VIOLATION: Prohibited private host '{}'",
                host_str
            )));
        }

        Ok(parsed)
    }
}

impl Default for PrunaPVideoReplaceProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudVideoProvider for PrunaPVideoReplaceProvider {
    fn provider_id(&self) -> &str {
        "replicate"
    }

    fn model_id(&self) -> &str {
        "prunaai/p-video-replace"
    }

    fn model_version_hint(&self) -> Option<&str> {
        Some("official-current")
    }

    fn provider_name(&self) -> &str {
        "Replicate Pruna Video Replace"
    }

    fn is_configured(&self) -> bool {
        self.api_token.is_some()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_text_to_video: false,
            supports_image_to_video: false,
            supports_video_to_video: true,
            supports_reference_image: true,
            supports_character_reference: true,
            supports_audio: true,
            max_duration_sec: 300.0,
            supported_resolutions: vec![
                (576, 1024),
                (720, 1280),
                (1080, 1920),
                (1280, 720),
                (1920, 1080),
                (512, 512),
            ],
            estimated_cost_per_second: Some(0.03),
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
            let res_tier = ResolutionTier::from_dimensions(req.resolution).ok();
            let (rate, res_tier_str) = if let Some(tier) = res_tier {
                if let Some(pt) = record
                    .pricing_tiers
                    .iter()
                    .find(|t| t.resolution_tier == tier.as_str())
                {
                    (pt.pricing_amount, tier.as_str().to_string())
                } else {
                    (record.pricing_amount.unwrap_or(0.03), "720p".to_string())
                }
            } else {
                (record.pricing_amount.unwrap_or(0.03), "720p".to_string())
            };

            let inf_cost = rate * dur;
            let confidence = if self.is_configured() {
                CostConfidence::Estimated
            } else {
                CostConfidence::Unknown
            };

            CostEstimate {
                provider: self.provider_id().to_string(),
                model: self.model_id().to_string(),
                estimated_usd: Some(inf_cost),
                min_usd: Some(inf_cost * 0.95),
                max_usd: Some(inf_cost * 1.05),
                confidence: if self.is_configured() { 0.95 } else { 0.0 },
                currency: record.currency.clone(),
                status: confidence,
                breakdown: format!(
                    "Provider: {} ({}) | Rate: ${:.3}/s ({}) | Dur: {:.1}s",
                    record.provider_id, record.model_id, rate, res_tier_str, dur
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
        // Build mock prepared submission from request for direct compatibility
        let source_video = request
            .source_video
            .clone()
            .unwrap_or_else(|| PathBuf::from("mock_source.mp4"));
        let reference_images = request.get_reference_images();
        let prompt = if request.prompt.trim().is_empty() {
            None
        } else {
            Some(request.prompt.clone())
        };

        let spec = crate::ai::cloud::spec::ProviderSubmissionSpec {
            provider_key: crate::ai::cloud::provider::ProviderKey::new(
                self.provider_id(),
                self.model_id(),
            ),
            source_video: source_video.clone(),
            reference_images: if reference_images.is_empty() {
                vec![PathBuf::from("mock_ref.jpg")]
            } else {
                reference_images
            },
            instruction_prompt: prompt,
            resolution_tier: ResolutionTier::from_dimensions(request.resolution)
                .unwrap_or(ResolutionTier::P720),
            target_fps: crate::ai::cloud::provider::TargetFps::from_f64(request.fps),
            save_audio: true,
            ignore_audio: false,
            turbo: false,
            disable_safety_checker: false,
            seed: None,
        };

        let prepared = PreparedProviderSubmission {
            spec,
            uploaded_source: crate::ai::cloud::uploader::UploadedAsset {
                provider_file_id: Some("mock_source_id".to_string()),
                input_uri: "https://replicate.delivery/mock_uploads/source.mp4".to_string(),
                expires_at: None,
                checksum: None,
            },
            uploaded_references: vec![crate::ai::cloud::uploader::UploadedAsset {
                provider_file_id: Some("mock_ref_id".to_string()),
                input_uri: "https://replicate.delivery/mock_uploads/ref.jpg".to_string(),
                expires_at: None,
                checksum: None,
            }],
        };

        self.create_prediction(&prepared)
    }

    fn create_prediction(
        &self,
        prepared: &PreparedProviderSubmission,
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

        // Live execution policy check before billable prediction create
        let live_policy = self.live_policy.clone();
        let client = self.client.clone();
        let prepared_clone = prepared.clone();
        let model_id_str = self.model_id().to_string();
        let provider_id_str = self.provider_id().to_string();

        Box::pin(async move {
            live_policy.ensure_paid_live_allowed()?;

            let input_body = json!({
                "input": {
                    "video": prepared_clone.uploaded_source.input_uri,
                    "images": prepared_clone.uploaded_references.iter().map(|a| a.input_uri.clone()).collect::<Vec<_>>(),
                    "instruction_prompt": prepared_clone.spec.instruction_prompt,
                    "resolution": prepared_clone.spec.resolution_tier.as_str(),
                    "target_fps": prepared_clone.spec.target_fps.as_str(),
                    "save_audio": prepared_clone.spec.save_audio,
                    "ignore_audio": prepared_clone.spec.ignore_audio,
                    "turbo": prepared_clone.spec.turbo,
                    "disable_safety_checker": false
                }
            });

            // Official Replicate Model Endpoint: POST /v1/models/prunaai/p-video-replace/predictions
            let endpoint =
                "https://api.replicate.com/v1/models/prunaai/p-video-replace/predictions";
            let response = client
                .post(endpoint)
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .json(&input_body)
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
                return Err(CloudProviderError::ProviderUnavailable(format!(
                    "Replicate prediction creation failed with HTTP {}: {}",
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
                        "Missing 'id' in Replicate prediction response".to_string(),
                    )
                })?
                .to_string();

            let model_version = resp_json["version"]
                .as_str()
                .map(|s| s.to_string())
                .or_else(|| Some("official-current".to_string()));

            Ok(CloudJobHandle {
                job_id: format!("cjob-{}", uuid::Uuid::new_v4()),
                remote_id,
                provider_id: provider_id_str,
                model: model_id_str,
                model_version,
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

            let status = response.status();
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(CloudProviderError::AuthFailed(
                    "Replicate authentication failed during status poll".to_string(),
                ));
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(CloudProviderError::RateLimited(
                    "Replicate rate limit exceeded during status poll".to_string(),
                ));
            }
            if !status.is_success() {
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
                other => {
                    return Err(CloudProviderError::ProtocolViolation(format!(
                        "UNKNOWN_REMOTE_STATUS: Received unexpected remote status string '{}'",
                        other
                    )));
                }
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
            let response = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
                .map_err(|e| CloudProviderError::NetworkError(e.to_string()))?;

            let status = response.status();
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(CloudProviderError::AuthFailed(
                    "Replicate authentication failed during cancel".to_string(),
                ));
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(CloudProviderError::RateLimited(
                    "Replicate rate limit exceeded during cancel".to_string(),
                ));
            }
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(CloudProviderError::RequestInvalid(
                    "Prediction not found on remote provider".to_string(),
                ));
            }
            if !status.is_success() {
                let err_text = response.text().await.unwrap_or_default();
                return Err(CloudProviderError::ProviderUnavailable(format!(
                    "Remote cancellation request failed with HTTP {}: {}",
                    status, err_text
                )));
            }

            // Check if response contains immediate terminal status
            if let Ok(resp_json) = response.json::<serde_json::Value>().await {
                if let Some(status_str) = resp_json["status"].as_str() {
                    if status_str == "canceled" || status_str == "failed" {
                        return Ok(());
                    }
                }
            }

            // Perform bounded poll if cancellation was accepted but not yet confirmed terminal
            for _ in 0..5 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                let poll_url = format!("https://api.replicate.com/v1/predictions/{}", r_id);
                if let Ok(poll_resp) = client
                    .get(&poll_url)
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await
                {
                    if let Ok(poll_json) = poll_resp.json::<serde_json::Value>().await {
                        if let Some(s) = poll_json["status"].as_str() {
                            if s == "canceled" || s == "failed" || s == "succeeded" {
                                return Ok(());
                            }
                        }
                    }
                }
            }

            Ok(())
        })
    }

    fn download_result(
        &self,
        output_url: &str,
        target_path: &Path,
    ) -> Pin<Box<dyn Future<Output = Result<PathBuf, CloudProviderError>> + Send + '_>> {
        let client = self.client.clone();
        let url_str = output_url.to_string();
        let dest = target_path.to_path_buf();

        Box::pin(async move {
            // 1. Authoritative SSRF Validation on initial URL
            let mut current_url = Self::validate_ssrf_url(&url_str)?;

            // 2. Bounded Redirect Following with SSRF Validation on Every Hop
            let mut redirect_count = 0;
            let max_redirects = 5;

            let response = loop {
                let resp = client.get(current_url.as_str()).send().await.map_err(|e| {
                    CloudProviderError::DownloadFailed(format!(
                        "Network error downloading result: {}",
                        e
                    ))
                })?;

                let status = resp.status();
                if status.is_redirection() {
                    redirect_count += 1;
                    if redirect_count > max_redirects {
                        return Err(CloudProviderError::SecurityViolation(
                            "Too many redirects during artifact download".to_string(),
                        ));
                    }

                    let loc = resp
                        .headers()
                        .get(reqwest::header::LOCATION)
                        .ok_or_else(|| {
                            CloudProviderError::DownloadFailed(
                                "Redirect response missing Location header".to_string(),
                            )
                        })?
                        .to_str()
                        .map_err(|_| {
                            CloudProviderError::SecurityViolation(
                                "Invalid Location header characters".to_string(),
                            )
                        })?;

                    let next_url = current_url.join(loc).map_err(|e| {
                        CloudProviderError::SecurityViolation(format!(
                            "Invalid redirect URL: {}",
                            e
                        ))
                    })?;

                    // Authoritative SSRF Validation on Redirect Hop
                    current_url = Self::validate_ssrf_url(next_url.as_str())?;
                    continue;
                }

                if !status.is_success() {
                    return Err(CloudProviderError::DownloadFailed(format!(
                        "Download failed with HTTP {}",
                        status
                    )));
                }

                break resp;
            };

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
