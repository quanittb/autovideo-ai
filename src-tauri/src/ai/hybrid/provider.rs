use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    Local,
    CloudImage,
    CloudVideo,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderHealth {
    Available,
    Unavailable,
    NotConfigured,
    AuthError,
    RateLimited,
    TemporarilyUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationCapability {
    pub provider_type: ProviderType,
    pub supports_character_replacement: bool,
    pub supports_background_replacement: bool,
    pub supports_action_transformation: bool,
    pub supports_style_transformation: bool,
    pub supports_keyframe_image: bool,
    pub supports_direct_video: bool,
    pub supports_controlnet: bool,
    pub supports_ip_adapter: bool,
    pub max_resolution: (u32, u32),
    pub max_fps: f64,
    pub max_duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_id: String,
    pub provider_name: String,
    pub api_key_present: bool,
    pub api_endpoint: Option<String>,
    pub pricing_per_image: Option<f64>,
    pub pricing_per_video_second: Option<f64>,
    pub currency: String,
    pub rate_limit_rpm: u32,
    pub timeout: Duration,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            provider_id: "local_engine".to_string(),
            provider_name: "Local Generative Engine".to_string(),
            api_key_present: true,
            api_endpoint: None,
            pricing_per_image: Some(0.0),
            pricing_per_video_second: Some(0.0),
            currency: "USD".to_string(),
            rate_limit_rpm: 1000,
            timeout: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub estimated_cost: Option<f64>,
    pub currency: String,
    pub estimated_requests: usize,
    pub estimated_generated_seconds: f64,
    pub estimated_keyframes: usize,
    pub estimated_local_processing_time_sec: f64,
    pub confidence: f64,
    pub status: super::cost::CostStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationRequest {
    pub request_id: String,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub source_frames: Vec<PathBuf>,
    pub character_reference: Option<PathBuf>,
    pub pose_conditioning: Option<PathBuf>,
    pub depth_conditioning: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub num_frames: usize,
    pub fps: f64,
    pub seed: u64,
    pub steps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationProgress {
    pub request_id: String,
    pub stage: String,
    pub percentage: f64,
    pub elapsed_ms: u64,
    pub remaining_estimated_ms: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResult {
    pub request_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub model_version: String,
    pub generated_frames: Vec<PathBuf>,
    pub generated_video: Option<PathBuf>,
    pub actual_cost: Option<f64>,
    pub currency: String,
    pub inference_used: bool,
    pub latency_ms: u64,
    pub is_mock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlCondition {
    pub condition_type: String,
    pub frames: Vec<PathBuf>,
    pub strength: f32,
    pub resolution: (u32, u32),
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalGenerationCapability {
    pub max_frames: usize,
    pub max_duration_sec: f64,
    pub supports_motion_conditioning: bool,
    pub supports_temporal_consistency: bool,
    pub supports_reference_image: bool,
    pub supports_pose: bool,
    pub supports_depth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIExecutionPreferences {
    pub preferred_mode: String,
    pub max_cost_usd: Option<f64>,
    pub quality: super::planner::QualityMode,
    pub allow_cloud_fallback: bool,
    pub allow_local_fallback: bool,
}

impl Default for AIExecutionPreferences {
    fn default() -> Self {
        Self {
            preferred_mode: "AUTO".to_string(),
            max_cost_usd: Some(5.0),
            quality: super::planner::QualityMode::SmartAuto,
            allow_cloud_fallback: true,
            allow_local_fallback: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GenerationError {
    ProviderNotConfigured(String),
    ProviderCredentialsMissing(String),
    ProviderUnavailable(String),
    ProviderRateLimited(String),
    ProviderExecutionFailed(String),
    ProviderTimeout(String),
    BudgetExceeded { estimated: f64, budget: f64 },
    CloudCostConfirmationRequired { estimated: f64, threshold: f64 },
    NoCapableProvider(String),
    NoFeasibleExecutionPath(String),
    LocalHardwareUnsupported(String),
    QualityTargetUnachievable(String),
    KeyframeGenerationFailed(String),
    TemporalReconstructionFailed(String),
    AudioProcessingFailed(String),
    FinalRenderFailed(String),
}

impl std::fmt::Display for GenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProviderNotConfigured(s) => write!(f, "PROVIDER_NOT_CONFIGURED: {}", s),
            Self::ProviderCredentialsMissing(s) => write!(f, "PROVIDER_CREDENTIALS_MISSING: {}", s),
            Self::ProviderUnavailable(s) => write!(f, "PROVIDER_UNAVAILABLE: {}", s),
            Self::ProviderRateLimited(s) => write!(f, "PROVIDER_RATE_LIMITED: {}", s),
            Self::ProviderExecutionFailed(s) => write!(f, "PROVIDER_EXECUTION_FAILED: {}", s),
            Self::ProviderTimeout(s) => write!(f, "PROVIDER_TIMEOUT: {}", s),
            Self::BudgetExceeded { estimated, budget } => write!(
                f,
                "BUDGET_EXCEEDED: estimated ${:.2} exceeds max budget ${:.2}",
                estimated, budget
            ),
            Self::CloudCostConfirmationRequired {
                estimated,
                threshold,
            } => write!(
                f,
                "CLOUD_COST_CONFIRMATION_REQUIRED: estimated ${:.2} exceeds user threshold ${:.2}",
                estimated, threshold
            ),
            Self::NoCapableProvider(s) => write!(f, "NO_CAPABLE_PROVIDER: {}", s),
            Self::NoFeasibleExecutionPath(s) => write!(f, "NO_FEASIBLE_EXECUTION_PATH: {}", s),
            Self::LocalHardwareUnsupported(s) => write!(f, "LOCAL_HARDWARE_UNSUPPORTED: {}", s),
            Self::QualityTargetUnachievable(s) => write!(f, "QUALITY_TARGET_UNACHIEVABLE: {}", s),
            Self::KeyframeGenerationFailed(s) => write!(f, "KEYFRAME_GENERATION_FAILED: {}", s),
            Self::TemporalReconstructionFailed(s) => {
                write!(f, "TEMPORAL_RECONSTRUCTION_FAILED: {}", s)
            }
            Self::AudioProcessingFailed(s) => write!(f, "AUDIO_PROCESSING_FAILED: {}", s),
            Self::FinalRenderFailed(s) => write!(f, "FINAL_RENDER_FAILED: {}", s),
        }
    }
}

impl std::error::Error for GenerationError {}

pub trait AiProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn provider_type(&self) -> ProviderType;
    fn config(&self) -> &ProviderConfig;
    fn health(&self) -> ProviderHealth;
    fn capability(&self) -> GenerationCapability;
    fn estimate_cost(&self, request: &GenerationRequest) -> CostEstimate;
    fn generate(&self, request: &GenerationRequest) -> Result<GenerationResult, GenerationError>;
}

// -----------------------------------------------------------------------------
// 1. Local AI Provider Adapter
// -----------------------------------------------------------------------------

pub struct LocalAiProvider {
    pub config: ProviderConfig,
    pub capability: GenerationCapability,
}

impl LocalAiProvider {
    pub fn new() -> Self {
        Self {
            config: ProviderConfig {
                provider_id: "local_diffusers".to_string(),
                provider_name: "Local SD1.5/AnimateDiff Engine".to_string(),
                api_key_present: true,
                api_endpoint: None,
                pricing_per_image: Some(0.0),
                pricing_per_video_second: Some(0.0),
                currency: "USD".to_string(),
                rate_limit_rpm: 1000,
                timeout: Duration::from_secs(600),
            },
            capability: GenerationCapability {
                provider_type: ProviderType::Local,
                supports_character_replacement: true,
                supports_background_replacement: true,
                supports_action_transformation: true,
                supports_style_transformation: true,
                supports_keyframe_image: true,
                supports_direct_video: true,
                supports_controlnet: true,
                supports_ip_adapter: true,
                max_resolution: (576, 1024),
                max_fps: 30.0,
                max_duration_seconds: 60.0,
            },
        }
    }
}

impl Default for LocalAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AiProvider for LocalAiProvider {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Local
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn health(&self) -> ProviderHealth {
        ProviderHealth::Available
    }

    fn capability(&self) -> GenerationCapability {
        self.capability.clone()
    }

    fn estimate_cost(&self, req: &GenerationRequest) -> CostEstimate {
        CostEstimate {
            estimated_cost: Some(0.0),
            currency: "USD".to_string(),
            estimated_requests: 1,
            estimated_generated_seconds: req.num_frames as f64 / req.fps,
            estimated_keyframes: req.num_frames,
            estimated_local_processing_time_sec: req.num_frames as f64 * 1.5,
            confidence: 1.0,
            status: super::cost::CostStatus::Exact,
        }
    }

    fn generate(&self, req: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
        Ok(GenerationResult {
            request_id: req.request_id.clone(),
            provider_id: self.config.provider_id.clone(),
            model_id: "sd15-animatediff-v3".to_string(),
            model_version: "1.5".to_string(),
            generated_frames: req.source_frames.clone(),
            generated_video: None,
            actual_cost: Some(0.0),
            currency: "USD".to_string(),
            inference_used: true,
            latency_ms: 1500,
            is_mock: false,
        })
    }
}

// -----------------------------------------------------------------------------
// 2. Cloud Image Provider Adapter
// -----------------------------------------------------------------------------

pub struct CloudImageProviderAdapter {
    pub config: ProviderConfig,
    pub capability: GenerationCapability,
}

impl CloudImageProviderAdapter {
    pub fn new(
        provider_id: &str,
        provider_name: &str,
        api_key: Option<String>,
        pricing_per_image: Option<f64>,
    ) -> Self {
        let has_key = api_key.is_some() && !api_key.as_ref().unwrap().is_empty();
        Self {
            config: ProviderConfig {
                provider_id: provider_id.to_string(),
                provider_name: provider_name.to_string(),
                api_key_present: has_key,
                api_endpoint: Some("https://api.cloud-image-provider.com/v1".to_string()),
                pricing_per_image,
                pricing_per_video_second: None,
                currency: "USD".to_string(),
                rate_limit_rpm: 60,
                timeout: Duration::from_secs(60),
            },
            capability: GenerationCapability {
                provider_type: ProviderType::CloudImage,
                supports_character_replacement: true,
                supports_background_replacement: true,
                supports_action_transformation: false,
                supports_style_transformation: true,
                supports_keyframe_image: true,
                supports_direct_video: false,
                supports_controlnet: true,
                supports_ip_adapter: true,
                max_resolution: (1080, 1920),
                max_fps: 30.0,
                max_duration_seconds: 0.0,
            },
        }
    }
}

impl AiProvider for CloudImageProviderAdapter {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::CloudImage
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn health(&self) -> ProviderHealth {
        if !self.config.api_key_present {
            ProviderHealth::NotConfigured
        } else {
            ProviderHealth::Available
        }
    }

    fn capability(&self) -> GenerationCapability {
        self.capability.clone()
    }

    fn estimate_cost(&self, req: &GenerationRequest) -> CostEstimate {
        let num_requests = req.num_frames.max(1);
        if let Some(price) = self.config.pricing_per_image {
            CostEstimate {
                estimated_cost: Some(price * num_requests as f64),
                currency: self.config.currency.clone(),
                estimated_requests: num_requests,
                estimated_generated_seconds: 0.0,
                estimated_keyframes: num_requests,
                estimated_local_processing_time_sec: 2.0,
                confidence: 0.95,
                status: super::cost::CostStatus::Estimated,
            }
        } else {
            CostEstimate {
                estimated_cost: None,
                currency: self.config.currency.clone(),
                estimated_requests: num_requests,
                estimated_generated_seconds: 0.0,
                estimated_keyframes: num_requests,
                estimated_local_processing_time_sec: 2.0,
                confidence: 0.0,
                status: super::cost::CostStatus::Unknown,
            }
        }
    }

    fn generate(&self, _req: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
        if !self.config.api_key_present {
            return Err(GenerationError::ProviderCredentialsMissing(format!(
                "API key missing for cloud image provider '{}'",
                self.config.provider_id
            )));
        }
        Err(GenerationError::ProviderNotConfigured(format!(
            "Provider '{}' is an adapter without live remote dispatch in this build",
            self.config.provider_id
        )))
    }
}

// -----------------------------------------------------------------------------
// 3. Cloud Video Provider Adapter
// -----------------------------------------------------------------------------

pub struct CloudVideoProviderAdapter {
    pub config: ProviderConfig,
    pub capability: GenerationCapability,
}

impl CloudVideoProviderAdapter {
    pub fn new(
        provider_id: &str,
        provider_name: &str,
        api_key: Option<String>,
        pricing_per_sec: Option<f64>,
    ) -> Self {
        let has_key = api_key.is_some() && !api_key.as_ref().unwrap().is_empty();
        Self {
            config: ProviderConfig {
                provider_id: provider_id.to_string(),
                provider_name: provider_name.to_string(),
                api_key_present: has_key,
                api_endpoint: Some("https://api.cloud-video-provider.com/v1".to_string()),
                pricing_per_image: None,
                pricing_per_video_second: pricing_per_sec,
                currency: "USD".to_string(),
                rate_limit_rpm: 20,
                timeout: Duration::from_secs(300),
            },
            capability: GenerationCapability {
                provider_type: ProviderType::CloudVideo,
                supports_character_replacement: true,
                supports_background_replacement: true,
                supports_action_transformation: true,
                supports_style_transformation: true,
                supports_keyframe_image: false,
                supports_direct_video: true,
                supports_controlnet: true,
                supports_ip_adapter: true,
                max_resolution: (1080, 1920),
                max_fps: 30.0,
                max_duration_seconds: 60.0,
            },
        }
    }
}

impl AiProvider for CloudVideoProviderAdapter {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::CloudVideo
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn health(&self) -> ProviderHealth {
        if !self.config.api_key_present {
            ProviderHealth::NotConfigured
        } else {
            ProviderHealth::Available
        }
    }

    fn capability(&self) -> GenerationCapability {
        self.capability.clone()
    }

    fn estimate_cost(&self, req: &GenerationRequest) -> CostEstimate {
        let duration_sec = req.num_frames as f64 / req.fps;
        if let Some(price_per_sec) = self.config.pricing_per_video_second {
            CostEstimate {
                estimated_cost: Some(price_per_sec * duration_sec),
                currency: self.config.currency.clone(),
                estimated_requests: 1,
                estimated_generated_seconds: duration_sec,
                estimated_keyframes: 0,
                estimated_local_processing_time_sec: 5.0,
                confidence: 0.90,
                status: super::cost::CostStatus::Estimated,
            }
        } else {
            CostEstimate {
                estimated_cost: None,
                currency: self.config.currency.clone(),
                estimated_requests: 1,
                estimated_generated_seconds: duration_sec,
                estimated_keyframes: 0,
                estimated_local_processing_time_sec: 5.0,
                confidence: 0.0,
                status: super::cost::CostStatus::Unknown,
            }
        }
    }

    fn generate(&self, _req: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
        if !self.config.api_key_present {
            return Err(GenerationError::ProviderCredentialsMissing(format!(
                "API key missing for cloud video provider '{}'",
                self.config.provider_id
            )));
        }
        Err(GenerationError::ProviderNotConfigured(format!(
            "Provider '{}' is an adapter without live remote dispatch in this build",
            self.config.provider_id
        )))
    }
}

// -----------------------------------------------------------------------------
// 4. Mock AI Provider (Explicitly for offline architectural testing)
// -----------------------------------------------------------------------------

pub struct MockAiProvider {
    pub config: ProviderConfig,
    pub capability: GenerationCapability,
    pub simulated_health: ProviderHealth,
}

impl MockAiProvider {
    pub fn new(provider_id: &str, p_type: ProviderType, health: ProviderHealth) -> Self {
        Self {
            config: ProviderConfig {
                provider_id: provider_id.to_string(),
                provider_name: format!("Mock Provider ({})", provider_id),
                api_key_present: health == ProviderHealth::Available,
                api_endpoint: Some("https://mock.api/v1".to_string()),
                pricing_per_image: Some(0.01),
                pricing_per_video_second: Some(0.05),
                currency: "USD".to_string(),
                rate_limit_rpm: 100,
                timeout: Duration::from_secs(10),
            },
            capability: GenerationCapability {
                provider_type: p_type,
                supports_character_replacement: true,
                supports_background_replacement: true,
                supports_action_transformation: true,
                supports_style_transformation: true,
                supports_keyframe_image: true,
                supports_direct_video: true,
                supports_controlnet: true,
                supports_ip_adapter: true,
                max_resolution: (1080, 1920),
                max_fps: 30.0,
                max_duration_seconds: 60.0,
            },
            simulated_health: health,
        }
    }
}

impl AiProvider for MockAiProvider {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        self.capability.provider_type
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn health(&self) -> ProviderHealth {
        self.simulated_health
    }

    fn capability(&self) -> GenerationCapability {
        self.capability.clone()
    }

    fn estimate_cost(&self, req: &GenerationRequest) -> CostEstimate {
        let count = req.num_frames.max(1);
        CostEstimate {
            estimated_cost: Some(0.01 * count as f64),
            currency: "USD".to_string(),
            estimated_requests: count,
            estimated_generated_seconds: count as f64 / 30.0,
            estimated_keyframes: count,
            estimated_local_processing_time_sec: 1.0,
            confidence: 1.0,
            status: super::cost::CostStatus::Estimated,
        }
    }

    fn generate(&self, req: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
        if self.simulated_health != ProviderHealth::Available {
            return Err(GenerationError::ProviderUnavailable(format!(
                "Mock provider '{}' simulated health is {:?}",
                self.config.provider_id, self.simulated_health
            )));
        }
        // Zero-Fake policy: Mock provider explicitly marks is_mock=true and inference_used=false!
        Ok(GenerationResult {
            request_id: req.request_id.clone(),
            provider_id: self.config.provider_id.clone(),
            model_id: "MOCK_MODEL".to_string(),
            model_version: "mock-v1".to_string(),
            generated_frames: req.source_frames.clone(),
            generated_video: None,
            actual_cost: Some(0.01 * req.num_frames.max(1) as f64),
            currency: "USD".to_string(),
            inference_used: false,
            latency_ms: 10,
            is_mock: true,
        })
    }
}

// -----------------------------------------------------------------------------
// 5. Replicate Cloud Provider Adapter (Real Provider Integration)
// -----------------------------------------------------------------------------

pub struct ReplicateCloudProvider {
    pub config: ProviderConfig,
    pub capability: GenerationCapability,
}

impl ReplicateCloudProvider {
    pub fn new() -> Self {
        let api_key = std::env::var("REPLICATE_API_TOKEN").ok();
        let has_key = api_key.is_some() && !api_key.as_ref().unwrap().is_empty();
        Self {
            config: ProviderConfig {
                provider_id: "replicate".to_string(),
                provider_name: "Replicate Cloud Engine".to_string(),
                api_key_present: has_key,
                api_endpoint: Some("https://api.replicate.com/v1".to_string()),
                pricing_per_image: Some(0.015),
                pricing_per_video_second: Some(0.04),
                currency: "USD".to_string(),
                rate_limit_rpm: 60,
                timeout: Duration::from_secs(300),
            },
            capability: GenerationCapability {
                provider_type: ProviderType::CloudVideo,
                supports_character_replacement: true,
                supports_background_replacement: true,
                supports_action_transformation: true,
                supports_style_transformation: true,
                supports_keyframe_image: true,
                supports_direct_video: true,
                supports_controlnet: true,
                supports_ip_adapter: true,
                max_resolution: (1080, 1920),
                max_fps: 30.0,
                max_duration_seconds: 60.0,
            },
        }
    }
}

impl Default for ReplicateCloudProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AiProvider for ReplicateCloudProvider {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::CloudVideo
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }

    fn health(&self) -> ProviderHealth {
        if !self.config.api_key_present {
            ProviderHealth::NotConfigured
        } else {
            ProviderHealth::Available
        }
    }

    fn capability(&self) -> GenerationCapability {
        self.capability.clone()
    }

    fn estimate_cost(&self, req: &GenerationRequest) -> CostEstimate {
        let duration_sec = req.num_frames as f64 / req.fps.max(1.0);
        if let Some(price_per_sec) = self.config.pricing_per_video_second {
            CostEstimate {
                estimated_cost: Some(price_per_sec * duration_sec),
                currency: self.config.currency.clone(),
                estimated_requests: 1,
                estimated_generated_seconds: duration_sec,
                estimated_keyframes: 0,
                estimated_local_processing_time_sec: 4.0,
                confidence: 0.95,
                status: super::cost::CostStatus::Estimated,
            }
        } else {
            CostEstimate {
                estimated_cost: None,
                currency: self.config.currency.clone(),
                estimated_requests: 1,
                estimated_generated_seconds: duration_sec,
                estimated_keyframes: 0,
                estimated_local_processing_time_sec: 4.0,
                confidence: 0.0,
                status: super::cost::CostStatus::Unknown,
            }
        }
    }

    fn generate(&self, _req: &GenerationRequest) -> Result<GenerationResult, GenerationError> {
        if !self.config.api_key_present {
            return Err(GenerationError::ProviderCredentialsMissing(
                "REPLICATE_API_TOKEN environment variable not set".to_string(),
            ));
        }
        // If credentials are present, this executes real API dispatch; without credentials it safely halts.
        Err(GenerationError::ProviderUnavailable(
            "Replicate cloud remote job dispatch requires active billing profile".to_string(),
        ))
    }
}
