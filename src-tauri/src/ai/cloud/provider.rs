use super::cost::CostEstimate;
use super::error::CloudProviderError;
use super::job::CloudJobRequest;
use super::spec::PreparedProviderSubmission;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderKey {
    pub provider_id: String,
    pub model_id: String,
}

impl ProviderKey {
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResolutionTier {
    #[serde(alias = "720p", alias = "720P")]
    P720,
    #[serde(alias = "1080p", alias = "1080P")]
    P1080,
}

impl ResolutionTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::P720 => "720p",
            Self::P1080 => "1080p",
        }
    }

    pub fn from_dimensions(res: (u32, u32)) -> Result<Self, CloudProviderError> {
        let max_dim = res.0.max(res.1);
        if max_dim <= 1280 {
            Ok(Self::P720)
        } else if max_dim <= 1920 {
            Ok(Self::P1080)
        } else {
            Err(CloudProviderError::RequestInvalid(format!(
                "Requested resolution {:?} exceeds maximum supported 1080p tier",
                res
            )))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TargetFps {
    Original,
    #[serde(alias = "24")]
    Fps24,
    #[serde(alias = "48")]
    Fps48,
}

impl TargetFps {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Fps24 => "24",
            Self::Fps48 => "48",
        }
    }

    pub fn from_f64(fps: f64) -> Self {
        if (fps - 24.0).abs() < 0.5 {
            Self::Fps24
        } else if (fps - 48.0).abs() < 0.5 {
            Self::Fps48
        } else {
            Self::Original
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub model_version: Option<String>,
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
    fn model_id(&self) -> &str;
    fn model_version_hint(&self) -> Option<&str> {
        None
    }
    fn provider_name(&self) -> &str;
    fn is_configured(&self) -> bool;
    fn capabilities(&self) -> ProviderCapabilities;
    fn estimate_cost(&self, request: &CloudJobRequest) -> CostEstimate;

    fn submit_job(
        &self,
        request: &CloudJobRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CloudJobHandle, CloudProviderError>> + Send + '_>>;

    fn create_prediction(
        &self,
        prepared: &PreparedProviderSubmission,
    ) -> Pin<Box<dyn Future<Output = Result<CloudJobHandle, CloudProviderError>> + Send + '_>> {
        // Default adapter bridge for providers that do not implement create_prediction directly
        let prompt = prepared.spec.instruction_prompt.clone().unwrap_or_default();
        let job_id = format!("job-{}", uuid::Uuid::new_v4());
        let req = CloudJobRequest {
            job_id,
            project_id: None,
            prompt,
            negative_prompt: None,
            source_video: Some(prepared.spec.source_video.clone()),
            reference_image: prepared.spec.reference_images.first().cloned(),
            reference_images: Some(prepared.spec.reference_images.clone()),
            duration_seconds: 6.0,
            fps: 24.0,
            resolution: (720, 1280),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
        };
        self.submit_job(&req)
    }

    fn poll_status(
        &self,
        remote_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<RemotePollResponse, CloudProviderError>> + Send + '_>>;

    fn cancel_job(
        &self,
        remote_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), CloudProviderError>> + Send + '_>>;

    fn download_result(
        &self,
        output_url: &str,
        target_path: &Path,
    ) -> Pin<Box<dyn Future<Output = Result<PathBuf, CloudProviderError>> + Send + '_>>;
}
