use crate::ai::cloud::cost::{CostConfidence, CostEstimate};
use crate::ai::cloud::error::CloudProviderError;
use crate::ai::cloud::job::CloudJobRequest;
use crate::ai::cloud::live_execution_guard::{EnvLiveExecutionPolicy, LiveExecutionPolicy};
use crate::ai::cloud::provider::{
    CloudJobHandle, CloudVideoProvider, ProviderCapabilities, RemotePollResponse, RemoteStatus,
};
use crate::ai::cloud::spec::PreparedProviderSubmission;
use serde_json::json;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

pub struct ReplicateBriaBgRemovalProvider {
    client: reqwest::Client,
    api_token: Option<String>,
    live_policy: Arc<dyn LiveExecutionPolicy>,
}

impl ReplicateBriaBgRemovalProvider {
    pub fn new() -> Self {
        let token = std::env::var("REPLICATE_API_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .redirect(reqwest::redirect::Policy::none())
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

        let is_allowed_delivery_host = host_lower == "replicate.delivery"
            || (host_lower.ends_with(".replicate.delivery") && !host_lower.starts_with('.'));

        if !is_allowed_delivery_host {
            return Err(CloudProviderError::SecurityViolation(format!(
                "SSRF_VIOLATION: Output host '{}' is not in allowed Replicate delivery domains",
                host_str
            )));
        }

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

impl Default for ReplicateBriaBgRemovalProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudVideoProvider for ReplicateBriaBgRemovalProvider {
    fn provider_id(&self) -> &str {
        "replicate"
    }

    fn model_id(&self) -> &str {
        "bria/video-remove-background"
    }

    fn model_version_hint(&self) -> Option<&str> {
        Some("official-current")
    }

    fn provider_name(&self) -> &str {
        "Replicate BRIA Video Background Removal"
    }

    fn is_configured(&self) -> bool {
        self.api_token.is_some()
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_text_to_video: false,
            supports_image_to_video: false,
            supports_video_to_video: true,
            supports_reference_image: false,
            supports_character_reference: false,
            supports_audio: true,
            max_duration_sec: Some(60.0),
            supported_resolutions: vec![],
            estimated_cost_per_second: None,
        }
    }

    fn estimate_cost(&self, _req: &CloudJobRequest) -> CostEstimate {
        CostEstimate {
            provider: self.provider_id().to_string(),
            model: self.model_id().to_string(),
            estimated_usd: None,
            min_usd: None,
            max_usd: None,
            confidence: 0.0,
            currency: "USD".to_string(),
            status: CostConfidence::Unknown,
            breakdown: "Replicate BRIA background removal cost requires authoritative source media duration facts via preflight submission gate".to_string(),
        }
    }

    fn submit_job(
        &self,
        _request: &CloudJobRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CloudJobHandle, CloudProviderError>> + Send + '_>> {
        let err = CloudProviderError::OperationUnsupported(
            "RAW_SUBMISSION_UNSUPPORTED: BRIA provider requires PreparedProviderSubmission via create_prediction".to_string(),
        );
        Box::pin(async move { Err(err) })
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

        let prep = match prepared {
            PreparedProviderSubmission::BackgroundRemoval(b) => b.clone(),
            _ => {
                let err = CloudProviderError::RequestInvalid(
                    "TASK_SUBMISSION_MISMATCH: BRIA provider only accepts BackgroundRemoval prepared submissions".to_string(),
                );
                return Box::pin(async move { Err(err) });
            }
        };

        let live_policy = self.live_policy.clone();
        let client = self.client.clone();
        let model_id_str = self.model_id().to_string();
        let provider_id_str = self.provider_id().to_string();

        Box::pin(async move {
            live_policy.ensure_paid_live_allowed()?;

            let input_body = json!({
                "input": {
                    "video_url": prep.uploaded_source.input_uri,
                    "background_color": "Transparent",
                    "output_container_and_codec": "webm_vp9",
                    "preserve_audio": prep.spec.preserve_audio
                }
            });

            // Official Replicate Model Endpoint: POST /v1/models/bria/video-remove-background/predictions
            let endpoint =
                "https://api.replicate.com/v1/models/bria/video-remove-background/predictions";
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

            if let Ok(resp_json) = response.json::<serde_json::Value>().await {
                if let Some(status_str) = resp_json["status"].as_str() {
                    if status_str == "canceled" || status_str == "failed" {
                        return Ok(());
                    }
                }
            }

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
            let mut current_url = Self::validate_ssrf_url(&url_str)?;
            let mut redirect_count = 0;

            let final_resp = loop {
                let resp = client
                    .get(current_url.as_str())
                    .send()
                    .await
                    .map_err(|e| CloudProviderError::NetworkError(e.to_string()))?;

                let status = resp.status();
                if status.is_redirection() {
                    redirect_count += 1;
                    if redirect_count > 5 {
                        return Err(CloudProviderError::NetworkError(
                            "Too many redirects while downloading artifact".to_string(),
                        ));
                    }

                    let location_header = resp
                        .headers()
                        .get(reqwest::header::LOCATION)
                        .ok_or_else(|| {
                            CloudProviderError::NetworkError(
                                "Redirect response missing Location header".to_string(),
                            )
                        })?
                        .to_str()
                        .map_err(|e| CloudProviderError::NetworkError(e.to_string()))?;

                    let next_url = current_url.join(location_header).map_err(|e| {
                        CloudProviderError::SecurityViolation(format!(
                            "Invalid redirect URL: {}",
                            e
                        ))
                    })?;

                    current_url = Self::validate_ssrf_url(next_url.as_str())?;
                    continue;
                }

                if !status.is_success() {
                    return Err(CloudProviderError::NetworkError(format!(
                        "Download failed with HTTP {}",
                        status
                    )));
                }

                break resp;
            };

            let bytes = final_resp
                .bytes()
                .await
                .map_err(|e| CloudProviderError::NetworkError(e.to_string()))?;

            if bytes.is_empty() {
                return Err(CloudProviderError::OutputInvalid(
                    "Downloaded artifact is empty (0 bytes)".to_string(),
                ));
            }

            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            std::fs::write(&dest, &bytes).map_err(|e| {
                CloudProviderError::ProviderUnavailable(format!(
                    "Failed to save downloaded artifact to {}: {}",
                    dest.display(),
                    e
                ))
            })?;

            Ok(dest)
        })
    }
}
