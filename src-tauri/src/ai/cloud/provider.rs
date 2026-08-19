use super::cost::CostEstimate;
use super::error::CloudProviderError;
use super::job::CloudJobRequest;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub supports_text_to_video: bool,
    pub supports_image_to_video: bool,
    pub supports_video_to_video: bool,
    pub supports_reference_image: bool,
    pub supports_character_reference: bool,
    pub supports_audio: bool,
    pub max_duration_sec: f64,
    pub supported_resolutions: Vec<(u32, u32)>,
    pub estimated_cost_per_second: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudJobHandle {
    pub job_id: String,
    pub remote_id: String,
    pub provider_id: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteStatus {
    Starting,
    Processing,
    Succeeded,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemotePollResponse {
    pub remote_id: String,
    pub status: RemoteStatus,
    pub output_url: Option<String>,
    pub error: Option<String>,
}

pub trait CloudVideoProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn provider_name(&self) -> &str;
    fn is_configured(&self) -> bool;
    fn capabilities(&self) -> ProviderCapabilities;
    fn estimate_cost(&self, request: &CloudJobRequest) -> CostEstimate;

    fn submit_job(
        &self,
        request: &CloudJobRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<CloudJobHandle, CloudProviderError>>
                + Send
                + '_,
        >,
    >;

    fn poll_status(
        &self,
        remote_id: &str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<RemotePollResponse, CloudProviderError>>
                + Send
                + '_,
        >,
    >;

    fn cancel_job(
        &self,
        remote_id: &str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), CloudProviderError>> + Send + '_>,
    >;

    fn download_result(
        &self,
        output_url: &str,
        target_path: &Path,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<PathBuf, CloudProviderError>> + Send + '_>,
    >;
}
