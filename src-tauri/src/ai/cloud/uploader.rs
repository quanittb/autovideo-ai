use super::error::CloudProviderError;
use super::live_execution_guard::{EnvLiveExecutionPolicy, LiveExecutionPolicy};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UploadedAsset {
    #[serde(default)]
    pub provider_file_id: Option<String>,
    pub input_uri: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub checksum: Option<String>,
}

pub trait ProviderAssetUploader: Send + Sync {
    fn upload_file<'a>(
        &'a self,
        file_path: &'a Path,
        content_type: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<UploadedAsset, CloudProviderError>> + Send + 'a>>;
}

pub struct ReplicateAssetUploader {
    client: reqwest::Client,
    api_token: Option<String>,
    live_policy: Arc<dyn LiveExecutionPolicy>,
}

impl ReplicateAssetUploader {
    pub fn new() -> Self {
        let token = std::env::var("REPLICATE_API_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
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
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_token,
            live_policy,
        }
    }
}

impl Default for ReplicateAssetUploader {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAssetUploader for ReplicateAssetUploader {
    fn upload_file<'a>(
        &'a self,
        file_path: &'a Path,
        content_type: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<UploadedAsset, CloudProviderError>> + Send + 'a>> {
        Box::pin(async move {
            // 1. Authoritative Paid Live Execution Guard Check
            self.live_policy.ensure_paid_live_allowed()?;

            // 2. Validate token presence
            let token = match &self.api_token {
                Some(t) => t.clone(),
                None => {
                    return Err(CloudProviderError::AuthFailed(
                        "REPLICATE_API_TOKEN is not configured".to_string(),
                    ));
                }
            };

            // 3. Validate local file
            if !file_path.is_file() {
                return Err(CloudProviderError::RequestInvalid(format!(
                    "Cannot upload non-existent or invalid file: {}",
                    file_path.display()
                )));
            }

            let file_bytes = tokio::fs::read(file_path).await.map_err(|e| {
                CloudProviderError::RequestInvalid(format!(
                    "Failed to read file for upload ({}): {}",
                    file_path.display(),
                    e
                ))
            })?;

            let file_name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("asset.bin")
                .to_string();

            let part = reqwest::multipart::Part::bytes(file_bytes)
                .file_name(file_name)
                .mime_str(content_type)
                .map_err(|e| {
                    CloudProviderError::RequestInvalid(format!("Invalid mime type: {}", e))
                })?;

            let form = reqwest::multipart::Form::new().part("content", part);

            // 4. Submit to official Replicate Files API: POST https://api.replicate.com/v1/files
            let response = self
                .client
                .post("https://api.replicate.com/v1/files")
                .header("Authorization", format!("Bearer {}", token))
                .multipart(form)
                .send()
                .await
                .map_err(|e| {
                    CloudProviderError::NetworkError(format!("Network error uploading file: {}", e))
                })?;

            let status = response.status();
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(CloudProviderError::AuthFailed(
                    "Replicate authentication failed during asset upload".to_string(),
                ));
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(CloudProviderError::RateLimited(
                    "Replicate 429 rate limit exceeded during asset upload".to_string(),
                ));
            }
            if !status.is_success() {
                let err_text = response.text().await.unwrap_or_default();
                return Err(CloudProviderError::ProviderUnavailable(format!(
                    "Replicate file upload failed with HTTP {}: {}",
                    status, err_text
                )));
            }

            let resp_json: serde_json::Value = response.json().await.map_err(|e| {
                CloudProviderError::RequestInvalid(format!(
                    "Invalid json in files API response: {}",
                    e
                ))
            })?;

            // Official schema: { "id": "...", "urls": { "get": "https://replicate.delivery/..." } }
            let file_id = resp_json["id"].as_str().map(|s| s.to_string());
            let input_uri = resp_json["urls"]["get"]
                .as_str()
                .or_else(|| resp_json["urls"]["serving"].as_str())
                .or_else(|| resp_json["url"].as_str())
                .ok_or_else(|| {
                    CloudProviderError::ProviderUnavailable(
                        "Missing output url in Replicate files API response".to_string(),
                    )
                })?
                .to_string();

            let expires_at = resp_json["expires_at"].as_str().map(|s| s.to_string());
            let checksum = resp_json["checksum"].as_str().map(|s| s.to_string());

            Ok(UploadedAsset {
                provider_file_id: file_id,
                input_uri,
                expires_at,
                checksum,
            })
        })
    }
}

pub struct MockAssetUploader {
    pub upload_call_count: Arc<AtomicUsize>,
    pub live_policy: Arc<dyn LiveExecutionPolicy>,
    pub custom_prefix: String,
}

impl MockAssetUploader {
    pub fn new() -> Self {
        Self {
            upload_call_count: Arc::new(AtomicUsize::new(0)),
            live_policy: Arc::new(EnvLiveExecutionPolicy),
            custom_prefix: "https://replicate.delivery/mock_uploads".to_string(),
        }
    }

    pub fn with_policy(live_policy: Arc<dyn LiveExecutionPolicy>) -> Self {
        Self {
            upload_call_count: Arc::new(AtomicUsize::new(0)),
            live_policy,
            custom_prefix: "https://replicate.delivery/mock_uploads".to_string(),
        }
    }
}

impl Default for MockAssetUploader {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAssetUploader for MockAssetUploader {
    fn upload_file<'a>(
        &'a self,
        file_path: &'a Path,
        _content_type: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<UploadedAsset, CloudProviderError>> + Send + 'a>> {
        Box::pin(async move {
            self.live_policy.ensure_paid_live_allowed()?;
            self.upload_call_count.fetch_add(1, Ordering::SeqCst);

            if !file_path.exists() {
                return Err(CloudProviderError::RequestInvalid(format!(
                    "Mock upload failed: file does not exist: {}",
                    file_path.display()
                )));
            }

            let file_name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("asset.bin");

            Ok(UploadedAsset {
                provider_file_id: Some(format!("mock_file_{}", file_name)),
                input_uri: format!("{}/{}", self.custom_prefix, file_name),
                expires_at: None,
                checksum: None,
            })
        })
    }
}
