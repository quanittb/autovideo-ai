use super::cost::{BudgetController, CostEstimator};
use super::keyframe::{KeyframePlan, KeyframePlanner};
use super::provider::{AiProvider, CostEstimate, GenerationError, ProviderHealth, ProviderType};
use crate::ai::generative::hardware::{CapabilityReport, CapabilityTier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformationIntent {
    CharacterReplacement,
    BackgroundReplacement,
    ActionTransformation,
    StyleTransformation,
    ObjectReplacement,
    AudioReplacement,
    Upscale,
    FullVideoRegeneration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityMode {
    Economy,
    Balanced,
    Quality,
    SmartAuto,
}

impl Default for QualityMode {
    fn default() -> Self {
        Self::SmartAuto
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComponentExecutionTarget {
    Local,
    CloudImage,
    CloudVideo,
    ReuseOriginal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentDecomposition {
    pub character: ComponentExecutionTarget,
    pub background: ComponentExecutionTarget,
    pub motion: ComponentExecutionTarget,
    pub audio: ComponentExecutionTarget,
    pub temporal_reconstruction: ComponentExecutionTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationPlan {
    pub intent: TransformationIntent,
    pub quality_mode: QualityMode,
    pub decomposition: ComponentDecomposition,
    pub keyframe_plan: KeyframePlan,
    pub selected_provider_id: String,
    pub selected_provider_type: ProviderType,
    pub total_source_frames: usize,
    pub duration_seconds: f64,
    pub estimated_cloud_requests: usize,
    pub estimated_local_operations: usize,
    pub cost_estimate: CostEstimate,
    pub budget_status: Result<(), GenerationError>,
    pub hardware_tier: CapabilityTier,
    pub recommendations: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct TransformationPlanner;

impl TransformationPlanner {
    pub fn plan(
        intent: TransformationIntent,
        quality_mode: QualityMode,
        total_frames: usize,
        fps: f64,
        scene_cuts: &[usize],
        motion_peaks: &[usize],
        hardware: &CapabilityReport,
        available_providers: &[&dyn AiProvider],
        max_budget: Option<f64>,
    ) -> Result<TransformationPlan, GenerationError> {
        let duration_sec = total_frames as f64 / fps.max(1.0);
        let mut warnings = Vec::new();
        let mut recommendations = Vec::new();

        // 1. Determine effective quality mode
        let effective_quality = match quality_mode {
            QualityMode::SmartAuto => match hardware.selected_tier {
                CapabilityTier::High | CapabilityTier::VeryHigh => QualityMode::Quality,
                CapabilityTier::Balanced => QualityMode::Balanced,
                CapabilityTier::LowVram | CapabilityTier::UltraLowVram => QualityMode::Economy,
                CapabilityTier::CpuOnly | CapabilityTier::Unsupported => QualityMode::Economy,
            },
            other => other,
        };

        // 2. Component Decomposition
        let decomposition =
            Self::decompose_components(intent, effective_quality, hardware.selected_tier);

        // 3. Plan keyframes for cloud image / hybrid workflow
        let keyframe_plan = KeyframePlanner::plan_keyframes(
            total_frames,
            fps,
            scene_cuts,
            motion_peaks,
            effective_quality,
            intent,
        );

        // 4. Provider Selection
        let (selected_provider, selected_type) =
            Self::select_provider(&decomposition, available_providers, &mut warnings)?;

        // 5. Cost Estimation
        let cost_estimate = if selected_type == ProviderType::CloudImage {
            CostEstimator::estimate_for_keyframes(
                selected_provider.config(),
                keyframe_plan.keyframe_count,
                duration_sec,
            )
        } else if selected_type == ProviderType::CloudVideo {
            CostEstimator::estimate_for_video(selected_provider.config(), duration_sec)
        } else {
            CostEstimate {
                estimated_cost: Some(0.0),
                currency: "USD".to_string(),
                estimated_requests: 0,
                estimated_generated_seconds: duration_sec,
                estimated_keyframes: keyframe_plan.keyframe_count,
                estimated_local_processing_time_sec: duration_sec * 1.2,
                confidence: 1.0,
                status: super::cost::CostStatus::Exact,
            }
        };

        // 6. Budget Check
        let budget_status = BudgetController::check_budget(&cost_estimate, max_budget);

        // 7. Operations Count
        let (cloud_reqs, local_ops) = match selected_type {
            ProviderType::Local => (0, keyframe_plan.keyframe_count + total_frames),
            ProviderType::CloudImage => (keyframe_plan.keyframe_count, total_frames * 2), // local control extraction + interpolation
            ProviderType::CloudVideo => (1, total_frames / 2),
            ProviderType::Hybrid => (keyframe_plan.keyframe_count, total_frames),
        };

        if hardware.selected_tier == CapabilityTier::LowVram
            || hardware.selected_tier == CapabilityTier::UltraLowVram
        {
            recommendations.push(
                "Hardware has limited VRAM: local control extraction + cloud keyframe generation recommended for optimal performance."
                    .to_string(),
            );
        }

        Ok(TransformationPlan {
            intent,
            quality_mode,
            decomposition,
            keyframe_plan,
            selected_provider_id: selected_provider.provider_id().to_string(),
            selected_provider_type: selected_type,
            total_source_frames: total_frames,
            duration_seconds: duration_sec,
            estimated_cloud_requests: cloud_reqs,
            estimated_local_operations: local_ops,
            cost_estimate,
            budget_status,
            hardware_tier: hardware.selected_tier,
            recommendations,
            warnings,
        })
    }

    fn decompose_components(
        intent: TransformationIntent,
        quality: QualityMode,
        tier: CapabilityTier,
    ) -> ComponentDecomposition {
        let is_low_end = matches!(
            tier,
            CapabilityTier::CpuOnly | CapabilityTier::UltraLowVram | CapabilityTier::LowVram
        );

        match intent {
            TransformationIntent::CharacterReplacement => ComponentDecomposition {
                character: if is_low_end {
                    ComponentExecutionTarget::CloudImage
                } else {
                    ComponentExecutionTarget::Local
                },
                background: ComponentExecutionTarget::ReuseOriginal,
                motion: ComponentExecutionTarget::ReuseOriginal,
                audio: ComponentExecutionTarget::ReuseOriginal,
                temporal_reconstruction: ComponentExecutionTarget::Local,
            },
            TransformationIntent::BackgroundReplacement => ComponentDecomposition {
                character: ComponentExecutionTarget::ReuseOriginal,
                background: if is_low_end {
                    ComponentExecutionTarget::CloudImage
                } else {
                    ComponentExecutionTarget::Local
                },
                motion: ComponentExecutionTarget::ReuseOriginal,
                audio: ComponentExecutionTarget::ReuseOriginal,
                temporal_reconstruction: ComponentExecutionTarget::Local,
            },
            TransformationIntent::ActionTransformation => ComponentDecomposition {
                character: ComponentExecutionTarget::Local,
                background: ComponentExecutionTarget::ReuseOriginal,
                motion: if quality == QualityMode::Quality {
                    ComponentExecutionTarget::CloudVideo
                } else {
                    ComponentExecutionTarget::Local
                },
                audio: ComponentExecutionTarget::ReuseOriginal,
                temporal_reconstruction: ComponentExecutionTarget::Local,
            },
            TransformationIntent::StyleTransformation => ComponentDecomposition {
                character: if is_low_end {
                    ComponentExecutionTarget::CloudImage
                } else {
                    ComponentExecutionTarget::Local
                },
                background: if is_low_end {
                    ComponentExecutionTarget::CloudImage
                } else {
                    ComponentExecutionTarget::Local
                },
                motion: ComponentExecutionTarget::ReuseOriginal,
                audio: ComponentExecutionTarget::ReuseOriginal,
                temporal_reconstruction: ComponentExecutionTarget::Local,
            },
            TransformationIntent::ObjectReplacement => ComponentDecomposition {
                character: ComponentExecutionTarget::ReuseOriginal,
                background: ComponentExecutionTarget::ReuseOriginal,
                motion: ComponentExecutionTarget::ReuseOriginal,
                audio: ComponentExecutionTarget::ReuseOriginal,
                temporal_reconstruction: ComponentExecutionTarget::Local,
            },
            TransformationIntent::AudioReplacement => ComponentDecomposition {
                character: ComponentExecutionTarget::ReuseOriginal,
                background: ComponentExecutionTarget::ReuseOriginal,
                motion: ComponentExecutionTarget::ReuseOriginal,
                audio: ComponentExecutionTarget::Local,
                temporal_reconstruction: ComponentExecutionTarget::ReuseOriginal,
            },
            TransformationIntent::Upscale => ComponentDecomposition {
                character: ComponentExecutionTarget::ReuseOriginal,
                background: ComponentExecutionTarget::ReuseOriginal,
                motion: ComponentExecutionTarget::ReuseOriginal,
                audio: ComponentExecutionTarget::ReuseOriginal,
                temporal_reconstruction: ComponentExecutionTarget::Local,
            },
            TransformationIntent::FullVideoRegeneration => ComponentDecomposition {
                character: if is_low_end {
                    ComponentExecutionTarget::CloudImage
                } else {
                    ComponentExecutionTarget::Local
                },
                background: if is_low_end {
                    ComponentExecutionTarget::CloudImage
                } else {
                    ComponentExecutionTarget::Local
                },
                motion: if quality == QualityMode::Quality && is_low_end {
                    ComponentExecutionTarget::CloudVideo
                } else {
                    ComponentExecutionTarget::Local
                },
                audio: ComponentExecutionTarget::ReuseOriginal,
                temporal_reconstruction: ComponentExecutionTarget::Local,
            },
        }
    }

    fn select_provider<'a>(
        decomp: &ComponentDecomposition,
        providers: &[&'a dyn AiProvider],
        warnings: &mut Vec<String>,
    ) -> Result<(&'a dyn AiProvider, ProviderType), GenerationError> {
        let needs_cloud_video = decomp.motion == ComponentExecutionTarget::CloudVideo;
        let needs_cloud_image = decomp.character == ComponentExecutionTarget::CloudImage
            || decomp.background == ComponentExecutionTarget::CloudImage;

        // Try to match appropriate available provider
        if needs_cloud_video {
            if let Some(&p) = providers.iter().find(|p| {
                p.provider_type() == ProviderType::CloudVideo
                    && p.health() == ProviderHealth::Available
            }) {
                return Ok((p, ProviderType::CloudVideo));
            }
            warnings.push(
                "Cloud video provider not available; falling back to Cloud Image or Local."
                    .to_string(),
            );
        }

        if needs_cloud_image {
            if let Some(&p) = providers.iter().find(|p| {
                p.provider_type() == ProviderType::CloudImage
                    && p.health() == ProviderHealth::Available
            }) {
                return Ok((p, ProviderType::CloudImage));
            }
            warnings
                .push("Cloud image provider not available; attempting local fallback.".to_string());
        }

        // Fallback to Local provider
        if let Some(&p) = providers.iter().find(|p| {
            p.provider_type() == ProviderType::Local && p.health() == ProviderHealth::Available
        }) {
            return Ok((p, ProviderType::Local));
        }

        // Check if any mock or configured provider exists
        if let Some(&p) = providers
            .iter()
            .find(|p| p.health() == ProviderHealth::Available)
        {
            return Ok((p, p.provider_type()));
        }

        // If not configured, check why
        if let Some(&p) = providers
            .iter()
            .find(|p| p.health() == ProviderHealth::NotConfigured)
        {
            return Err(GenerationError::ProviderNotConfigured(format!(
                "Provider '{}' is not configured with required credentials",
                p.provider_id()
            )));
        }

        Err(GenerationError::NoCapableProvider(
            "No capable and healthy AI provider found for the requested transformation plan"
                .to_string(),
        ))
    }
}
