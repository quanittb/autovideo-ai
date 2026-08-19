use super::provider::ProviderCapabilities;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionClass {
    LocalDeterministic,
    UtilityCloud,
    SpecializedVideoTransformation,
    GenerativeFallback,
    LocalExperimental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PricingUnit {
    PerSecond,
    PerPrediction,
    PerMegapixel,
    FreeLocal,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRecord {
    pub provider_id: String,
    pub model_id: String,
    pub model_version: String,
    pub execution_class: ExecutionClass,
    pub capabilities: ProviderCapabilities,
    pub max_duration_sec: Option<f64>,
    pub supported_resolutions: Vec<(u32, u32)>,
    pub supported_fps: Vec<f64>,
    pub pricing_unit: PricingUnit,
    pub pricing_amount: Option<f64>,
    pub currency: String,
    pub source_url: String,
    pub observed_at: String, // ISO 8601 Date
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRegistry {
    records: Vec<ProviderRecord>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            records: Vec::new(),
        };
        registry.register_default_providers();
        registry
    }

    fn register_default_providers(&mut self) {
        // 1. Local Deterministic FFmpeg Provider
        self.records.push(ProviderRecord {
            provider_id: "local_ffmpeg".to_string(),
            model_id: "ffmpeg_native".to_string(),
            model_version: "6.0+".to_string(),
            execution_class: ExecutionClass::LocalDeterministic,
            capabilities: ProviderCapabilities {
                supports_text_to_video: false,
                supports_image_to_video: false,
                supports_video_to_video: true,
                supports_reference_image: false,
                supports_character_reference: false,
                supports_audio: true,
                max_duration_sec: 3600.0,
                supported_resolutions: vec![
                    (288, 512),
                    (512, 512),
                    (576, 1024),
                    (720, 1280),
                    (1080, 1920),
                    (1920, 1080),
                    (3840, 2160),
                ],
                estimated_cost_per_second: Some(0.0),
            },
            max_duration_sec: None,
            supported_resolutions: vec![
                (288, 512),
                (512, 512),
                (576, 1024),
                (720, 1280),
                (1080, 1920),
                (1920, 1080),
                (3840, 2160),
            ],
            supported_fps: vec![23.976, 24.0, 25.0, 29.97, 30.0, 50.0, 59.94, 60.0],
            pricing_unit: PricingUnit::FreeLocal,
            pricing_amount: Some(0.0),
            currency: "USD".to_string(),
            source_url: "https://ffmpeg.org".to_string(),
            observed_at: "2026-08-19".to_string(),
        });

        // 2. Specialized Cloud Video Transformation (Replicate Minimax Video-01)
        self.records.push(ProviderRecord {
            provider_id: "replicate".to_string(),
            model_id: "minimax/video-01".to_string(),
            model_version: "minimax/video-01".to_string(),
            execution_class: ExecutionClass::SpecializedVideoTransformation,
            capabilities: ProviderCapabilities {
                supports_text_to_video: true,
                supports_image_to_video: true,
                supports_video_to_video: true,
                supports_reference_image: true,
                supports_character_reference: true,
                supports_audio: true,
                max_duration_sec: 10.0,
                supported_resolutions: vec![(512, 512), (576, 1024), (720, 1280), (1080, 1920)],
                estimated_cost_per_second: Some(0.04),
            },
            max_duration_sec: Some(10.0),
            supported_resolutions: vec![(512, 512), (576, 1024), (720, 1280), (1080, 1920)],
            supported_fps: vec![24.0, 25.0, 30.0],
            pricing_unit: PricingUnit::PerSecond,
            pricing_amount: Some(0.04),
            currency: "USD".to_string(),
            source_url: "https://replicate.com/minimax/video-01".to_string(),
            observed_at: "2026-08-19".to_string(),
        });

        // 3. Utility Cloud (Low-Cost Background Removal)
        self.records.push(ProviderRecord {
            provider_id: "replicate_utility".to_string(),
            model_id: "lucataco/remove-bg".to_string(),
            model_version: "fb8af171cfa1616ddcf1242fa093f9c46eada247c94b728f97d186dd78d5049c"
                .to_string(),
            execution_class: ExecutionClass::UtilityCloud,
            capabilities: ProviderCapabilities {
                supports_text_to_video: false,
                supports_image_to_video: false,
                supports_video_to_video: false,
                supports_reference_image: true,
                supports_character_reference: false,
                supports_audio: false,
                max_duration_sec: 0.0,
                supported_resolutions: vec![(512, 512), (720, 1280), (1080, 1920)],
                estimated_cost_per_second: None,
            },
            max_duration_sec: None,
            supported_resolutions: vec![(512, 512), (720, 1280), (1080, 1920), (1920, 1080)],
            supported_fps: vec![24.0, 30.0, 60.0],
            pricing_unit: PricingUnit::PerPrediction,
            pricing_amount: Some(0.005),
            currency: "USD".to_string(),
            source_url: "https://replicate.com/lucataco/remove-bg".to_string(),
            observed_at: "2026-08-19".to_string(),
        });

        // 4. Local Generative Fallback (SD1.5 + AnimateDiff)
        self.records.push(ProviderRecord {
            provider_id: "local_diffusers".to_string(),
            model_id: "sd15-animatediff-v3".to_string(),
            model_version: "v1-5-pruned-emaonly".to_string(),
            execution_class: ExecutionClass::GenerativeFallback,
            capabilities: ProviderCapabilities {
                supports_text_to_video: true,
                supports_image_to_video: true,
                supports_video_to_video: true,
                supports_reference_image: true,
                supports_character_reference: true,
                supports_audio: false,
                max_duration_sec: 5.0,
                supported_resolutions: vec![(288, 512), (512, 512), (512, 768)],
                estimated_cost_per_second: Some(0.0),
            },
            max_duration_sec: Some(5.0),
            supported_resolutions: vec![(288, 512), (512, 512), (512, 768)],
            supported_fps: vec![8.0, 12.0, 16.0, 24.0, 30.0],
            pricing_unit: PricingUnit::FreeLocal,
            pricing_amount: Some(0.0),
            currency: "USD".to_string(),
            source_url: "https://huggingface.co/runwayml/stable-diffusion-v1-5".to_string(),
            observed_at: "2026-08-19".to_string(),
        });
    }

    pub fn list_records(&self) -> &[ProviderRecord] {
        &self.records
    }

    pub fn find_by_id(&self, provider_id: &str) -> Option<&ProviderRecord> {
        self.records.iter().find(|r| r.provider_id == provider_id)
    }

    pub fn find_by_execution_class(&self, exec_class: ExecutionClass) -> Option<&ProviderRecord> {
        self.records
            .iter()
            .find(|r| r.execution_class == exec_class)
    }

    pub fn update_price(
        &mut self,
        provider_id: &str,
        pricing_amount: Option<f64>,
        source_url: &str,
        observed_at: &str,
    ) -> bool {
        if let Some(r) = self
            .records
            .iter_mut()
            .find(|r| r.provider_id == provider_id)
        {
            r.pricing_amount = pricing_amount;
            r.source_url = source_url.to_string();
            r.observed_at = observed_at.to_string();
            true
        } else {
            false
        }
    }

    pub fn register_provider(&mut self, record: ProviderRecord) {
        if let Some(idx) = self
            .records
            .iter()
            .position(|r| r.provider_id == record.provider_id)
        {
            self.records[idx] = record;
        } else {
            self.records.push(record);
        }
    }
}
