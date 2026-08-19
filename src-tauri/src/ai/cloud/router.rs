use super::cost::CostEstimate;
use super::job::CloudJobRequest;
use super::provider::CloudVideoProvider;
use crate::ai::generative::hardware::{CapabilityReport, CapabilityTier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserExecutionMode {
    Auto,
    Cloud,
    Local,
}

impl Default for UserExecutionMode {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenerationTask {
    CharacterReplacement,
    BackgroundReplacement,
    ActionTransformation,
    StyleTransformation,
    AudioTransformation,
    FullTransformation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingTarget {
    Cloud,
    Local,
    Hybrid,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub target: RoutingTarget,
    pub provider_id: String,
    pub task: GenerationTask,
    pub mode: UserExecutionMode,
    pub reason: String,
    pub estimated_cost: CostEstimate,
    pub fallback_available: bool,
}

pub struct GenerationRouter;

impl GenerationRouter {
    pub fn route(
        task: GenerationTask,
        mode: UserExecutionMode,
        request: &CloudJobRequest,
        cloud_provider: &dyn CloudVideoProvider,
        hardware: Option<&CapabilityReport>,
    ) -> RoutingDecision {
        let is_cloud_configured = cloud_provider.is_configured();
        let cost_estimate = cloud_provider.estimate_cost(request);

        let hw_tier = hardware
            .map(|h| h.selected_tier)
            .unwrap_or(CapabilityTier::LowVram);

        let is_video_generation_task = match task {
            GenerationTask::CharacterReplacement
            | GenerationTask::BackgroundReplacement
            | GenerationTask::ActionTransformation
            | GenerationTask::StyleTransformation
            | GenerationTask::FullTransformation => true,
            GenerationTask::AudioTransformation => false,
        };

        // 1. Explicit LOCAL mode
        if mode == UserExecutionMode::Local {
            return RoutingDecision {
                target: RoutingTarget::Local,
                provider_id: "local_diffusers".to_string(),
                task,
                mode,
                reason: "User explicitly selected LOCAL mode".to_string(),
                estimated_cost: CostEstimate {
                    provider: "local_diffusers".to_string(),
                    model: "sd15-animatediff-v3".to_string(),
                    estimated_usd: Some(0.0),
                    min_usd: Some(0.0),
                    max_usd: Some(0.0),
                    confidence: 1.0,
                    currency: "USD".to_string(),
                    status: super::cost::CostStatus::Exact,
                    breakdown: "$0.00 local compute".to_string(),
                },
                fallback_available: false,
            };
        }

        // 2. Explicit CLOUD mode (Never silently fallback to local!)
        if mode == UserExecutionMode::Cloud {
            if !is_cloud_configured {
                return RoutingDecision {
                    target: RoutingTarget::Unavailable,
                    provider_id: cloud_provider.provider_id().to_string(),
                    task,
                    mode,
                    reason: "CLOUD mode requested but provider credentials (e.g. REPLICATE_API_TOKEN) are missing".to_string(),
                    estimated_cost: cost_estimate,
                    fallback_available: false,
                };
            }
            return RoutingDecision {
                target: RoutingTarget::Cloud,
                provider_id: cloud_provider.provider_id().to_string(),
                task,
                mode,
                reason: "User explicitly selected CLOUD mode with active credentials".to_string(),
                estimated_cost: cost_estimate,
                fallback_available: false,
            };
        }

        // 3. AUTO mode: Cloud-First for Video Generation tasks
        if is_video_generation_task {
            if is_cloud_configured {
                return RoutingDecision {
                    target: RoutingTarget::Cloud,
                    provider_id: cloud_provider.provider_id().to_string(),
                    task,
                    mode,
                    reason: "AUTO mode: Cloud-First routing active for neural video generation"
                        .to_string(),
                    estimated_cost: cost_estimate,
                    fallback_available: true,
                };
            } else {
                // Cloud not configured: fallback to Local or Hybrid
                let target = match hw_tier {
                    CapabilityTier::LowVram | CapabilityTier::UltraLowVram => RoutingTarget::Hybrid,
                    CapabilityTier::CpuOnly | CapabilityTier::Unsupported => {
                        RoutingTarget::Unavailable
                    }
                    _ => RoutingTarget::Local,
                };

                let reason = if target == RoutingTarget::Unavailable {
                    "Cloud unconfigured and local hardware is insufficient for neural video generation".to_string()
                } else {
                    format!(
                        "Cloud unconfigured; falling back to local {:?} pipeline",
                        target
                    )
                };

                return RoutingDecision {
                    target,
                    provider_id: "local_diffusers".to_string(),
                    task,
                    mode,
                    reason,
                    estimated_cost: CostEstimate {
                        provider: "local_diffusers".to_string(),
                        model: "sd15-animatediff-v3".to_string(),
                        estimated_usd: Some(0.0),
                        min_usd: Some(0.0),
                        max_usd: Some(0.0),
                        confidence: 1.0,
                        currency: "USD".to_string(),
                        status: super::cost::CostStatus::Exact,
                        breakdown: "$0.00 local fallback".to_string(),
                    },
                    fallback_available: false,
                };
            }
        }

        // 4. Audio-only tasks stay local
        RoutingDecision {
            target: RoutingTarget::Local,
            provider_id: "local_media_engine".to_string(),
            task,
            mode,
            reason: "Non-generative audio transformation executed locally".to_string(),
            estimated_cost: CostEstimate {
                provider: "local_media_engine".to_string(),
                model: "ffmpeg_native".to_string(),
                estimated_usd: Some(0.0),
                min_usd: Some(0.0),
                max_usd: Some(0.0),
                confidence: 1.0,
                currency: "USD".to_string(),
                status: super::cost::CostStatus::Exact,
                breakdown: "$0.00 local processing".to_string(),
            },
            fallback_available: false,
        }
    }
}
