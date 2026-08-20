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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolutionPolicy {
    ExplicitTiered {
        supported_tiers: Vec<ResolutionTier>,
    },
    PreserveSource {
        max_width: Option<u32>,
        max_height: Option<u32>,
    },
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

    /// Explicit mapping of known AutoVideo AI application presets to provider resolution tiers.
    /// Does not guess or use arbitrary unbounded thresholds.
    pub fn from_dimensions(res: (u32, u32)) -> Result<Self, CloudProviderError> {
        let (w, h) = res;
        if w == 0 || h == 0 {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_RESOLUTION: Dimensions {:?} contain zero",
                res
            )));
        }

        match (w, h) {
            // 720p Tier presets (~1 Megapixel)
            (720, 1280)
            | (1280, 720)
            | (576, 1024)
            | (1024, 576)
            | (512, 512)
            | (640, 640)
            | (720, 720)
            | (720, 960)
            | (960, 720)
            | (288, 512)
            | (512, 288)
            | (320, 240)
            | (240, 320) => Ok(Self::P720),

            // 1080p Tier presets (~2 Megapixels)
            (1080, 1920) | (1920, 1080) | (1080, 1080) | (1080, 1440) | (1440, 1080) => {
                Ok(Self::P1080)
            }

            _ => Err(CloudProviderError::RequestInvalid(format!(
                "UNSUPPORTED_RESOLUTION_PRESET: Resolution {}x{} is not a recognized AutoVideo AI resolution preset for cloud transformation (must match explicit 720p or 1080p presets)",
                w, h
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TargetFps {
    #[serde(alias = "original", alias = "ORIGINAL")]
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
            // Source framerates such as 25, 29.97, 30, 50, 60 map to Original to preserve the source framerate
            Self::Original
        }
    }

    pub fn is_explicit_target(&self) -> bool {
        !matches!(self, Self::Original)
    }

    pub fn explicit_target_fps(&self) -> Option<u32> {
        match self {
            Self::Original => None,
            Self::Fps24 => Some(24),
            Self::Fps48 => Some(48),
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
    #[serde(default)]
    pub max_duration_sec: Option<f64>,
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
        _prepared: &PreparedProviderSubmission,
    ) -> Pin<Box<dyn Future<Output = Result<CloudJobHandle, CloudProviderError>> + Send + '_>> {
        let err = CloudProviderError::OperationUnsupported(format!(
            "PREPARED_SUBMISSION_UNSUPPORTED: Provider {}/{} does not implement prepared prediction creation",
            self.provider_id(),
            self.model_id()
        ));
        Box::pin(async move { Err(err) })
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
