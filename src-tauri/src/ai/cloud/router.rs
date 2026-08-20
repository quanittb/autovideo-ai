use super::cost::{CostBreakdown, CostConfidence, CostEstimate};
use super::error::CloudProviderError;
use super::job::CloudJobRequest;
use super::provider::ResolutionTier;
use super::registry::{ExecutionClass, PricingUnit, ProviderRecord, ProviderRegistry};
use crate::ai::generative::hardware::{CapabilityReport, CapabilityTier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskClass {
    #[serde(alias = "CharacterReplacement", alias = "CHARACTER_REPLACEMENT")]
    CharacterReplacement,
    #[serde(alias = "BackgroundRemoval", alias = "BACKGROUND_REMOVAL")]
    BackgroundRemoval,
    #[serde(
        alias = "BackgroundComposite",
        alias = "BackgroundReplacement",
        alias = "BACKGROUND_COMPOSITE",
        alias = "BACKGROUND_REPLACEMENT"
    )]
    BackgroundComposite,
    #[serde(
        alias = "StyleFilter",
        alias = "StyleTransformation",
        alias = "STYLE_FILTER",
        alias = "STYLE_TRANSFORMATION"
    )]
    StyleFilter,
    #[serde(
        alias = "AudioTransformation",
        alias = "AUDIO_TRANSFORMATION",
        alias = "AudioMux",
        alias = "AUDIO_MUX"
    )]
    AudioTransformation,
    #[serde(
        alias = "ActionRegeneration",
        alias = "ActionTransformation",
        alias = "ACTION_REGENERATION",
        alias = "ACTION_TRANSFORMATION"
    )]
    ActionRegeneration,
    #[serde(
        alias = "FullGenerativeTransformation",
        alias = "FullTransformation",
        alias = "FULL_GENERATIVE_TRANSFORMATION",
        alias = "FULL_TRANSFORMATION"
    )]
    FullGenerativeTransformation,
}

impl TaskClass {
    pub fn execution_class(&self) -> ExecutionClass {
        match self {
            TaskClass::StyleFilter
            | TaskClass::BackgroundComposite
            | TaskClass::AudioTransformation => ExecutionClass::LocalDeterministic,
            TaskClass::CharacterReplacement | TaskClass::ActionRegeneration => {
                ExecutionClass::SpecializedVideoTransformation
            }
            TaskClass::BackgroundRemoval => ExecutionClass::UtilityCloud,
            TaskClass::FullGenerativeTransformation => ExecutionClass::GenerativeFallback,
        }
    }

    pub fn from_str_strict(s: &str) -> Result<Self, CloudProviderError> {
        let normalized = s.trim().to_uppercase().replace('-', "_");
        match normalized.as_str() {
            "CHARACTER_REPLACEMENT" | "CHARACTERREPLACEMENT" | "CHARACTER" => {
                Ok(TaskClass::CharacterReplacement)
            }
            "BACKGROUND_REMOVAL" | "BACKGROUNDREMOVAL" | "REMOVE_BG" | "REMOVEBG"
            | "REMOVE_BACKGROUND" | "REMOVEBACKGROUND" => Ok(TaskClass::BackgroundRemoval),
            "BACKGROUND_COMPOSITE"
            | "BACKGROUNDCOMPOSITE"
            | "BACKGROUND_REPLACEMENT"
            | "BACKGROUNDREPLACEMENT"
            | "BACKGROUND" => Ok(TaskClass::BackgroundComposite),
            "STYLE_FILTER"
            | "STYLEFILTER"
            | "STYLE_TRANSFORMATION"
            | "STYLETRANSFORMATION"
            | "STYLE" => Ok(TaskClass::StyleFilter),
            "AUDIO_TRANSFORMATION" | "AUDIOTRANSFORMATION" | "AUDIO_MUX" | "AUDIOMUX" | "AUDIO" => {
                Ok(TaskClass::AudioTransformation)
            }
            "ACTION_REGENERATION"
            | "ACTIONREGENERATION"
            | "ACTION_TRANSFORMATION"
            | "ACTIONTRANSFORMATION"
            | "ACTION" => Ok(TaskClass::ActionRegeneration),
            "FULL_GENERATIVE_TRANSFORMATION"
            | "FULLGENERATIVETRANSFORMATION"
            | "FULL_TRANSFORMATION"
            | "FULLTRANSFORMATION"
            | "FULL"
            | "GENERATIVE" => Ok(TaskClass::FullGenerativeTransformation),
            _ => Err(CloudProviderError::RequestInvalid(format!(
                "UNKNOWN_TASK_CLASS: '{}' is not a recognized task class",
                s
            ))),
        }
    }

    pub fn from_str_or_default(s: &str) -> Self {
        Self::from_str_strict(s).unwrap_or(TaskClass::CharacterReplacement)
    }
}

pub type GenerationTask = TaskClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoutingPreference {
    #[serde(alias = "Auto", alias = "CostSaving", alias = "COST_SAVING")]
    CostSaving,
    #[serde(alias = "Quality", alias = "QUALITY")]
    Quality,
    #[serde(alias = "Local", alias = "LocalOnly", alias = "LOCAL_ONLY")]
    LocalOnly,
    #[serde(alias = "Cloud", alias = "CloudOnly", alias = "CLOUD_ONLY")]
    CloudOnly,
}

impl Default for RoutingPreference {
    fn default() -> Self {
        Self::CostSaving
    }
}

pub type UserExecutionMode = RoutingPreference;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoutingTarget {
    Local,
    Cloud,
    Hybrid,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoutingDecision {
    pub target: RoutingTarget,
    pub execution_class: ExecutionClass,
    pub provider_id: String,
    pub model_id: String,
    pub task: TaskClass,
    pub mode: RoutingPreference,
    pub reason: String,
    pub cost_breakdown: CostBreakdown,
    pub estimated_cost: CostEstimate,
    pub fallback_available: bool,
    pub auto_submit_allowed: bool,
}

pub struct GenerationRouter;

impl GenerationRouter {
    pub fn route(
        task: TaskClass,
        mode: RoutingPreference,
        request: &CloudJobRequest,
        hardware: Option<&CapabilityReport>,
    ) -> RoutingDecision {
        let registry = ProviderRegistry::new();
        Self::route_with_registry(task, mode, request, hardware, &registry)
    }

    pub fn route_with_registry(
        task: TaskClass,
        mode: RoutingPreference,
        request: &CloudJobRequest,
        hardware: Option<&CapabilityReport>,
        registry: &ProviderRegistry,
    ) -> RoutingDecision {
        let target_res = request.resolution;
        let target_fps = request.fps;
        let duration = if request.duration_seconds <= 0.0 {
            6.0
        } else {
            request.duration_seconds
        };

        // 1. Explicit LocalOnly Mode
        if mode == RoutingPreference::LocalOnly {
            return Self::build_local_decision(
                task,
                mode,
                "User explicitly selected LOCAL_ONLY mode",
                request,
                registry,
            );
        }

        // 2. Local Deterministic Tasks
        let is_local_deterministic = matches!(
            task,
            TaskClass::StyleFilter
                | TaskClass::BackgroundComposite
                | TaskClass::AudioTransformation
        );

        if is_local_deterministic && mode != RoutingPreference::CloudOnly {
            return Self::build_local_decision(
                task,
                mode,
                "Local deterministic processing preferred for style, composite, and audio tasks ($0.00)",
                request,
                registry,
            );
        }

        // 3. Full Generative Transformation Guard: Blocked in CostSaving
        if task == TaskClass::FullGenerativeTransformation && mode == RoutingPreference::CostSaving
        {
            let breakdown = CostBreakdown {
                provider_id: "none".to_string(),
                model_id: "none".to_string(),
                billable_duration_sec: duration,
                resolution: target_res,
                resolution_tier: None,
                unit_rate_usd: None,
                pricing_observed_at: None,
                segment_count: 1,
                overlap_duration_sec: 0.0,
                retry_allowance_usd: 0.0,
                inference_cost_usd: None,
                transfer_storage_cost_usd: None,
                total_usd: None,
                confidence: CostConfidence::Unknown,
                currency: "USD".to_string(),
                breakdown:
                    "Full generative transformation is disabled in COST_SAVING mode to protect budget"
                        .to_string(),
            };

            return RoutingDecision {
                target: RoutingTarget::Unavailable,
                execution_class: ExecutionClass::GenerativeFallback,
                provider_id: "none".to_string(),
                model_id: "none".to_string(),
                task,
                mode,
                reason: "Full generative video transformation is blocked in COST_SAVING mode. Select explicit task or short preview.".to_string(),
                estimated_cost: breakdown.to_estimate(),
                cost_breakdown: breakdown,
                fallback_available: false,
                auto_submit_allowed: false,
            };
        }

        // 4. Action Regeneration: Explicitly unsupported in Phase 16
        if task == TaskClass::ActionRegeneration {
            let breakdown = CostBreakdown {
                provider_id: "none".to_string(),
                model_id: "none".to_string(),
                billable_duration_sec: duration,
                resolution: target_res,
                resolution_tier: None,
                unit_rate_usd: None,
                pricing_observed_at: None,
                segment_count: 1,
                overlap_duration_sec: 0.0,
                retry_allowance_usd: 0.0,
                inference_cost_usd: None,
                transfer_storage_cost_usd: None,
                total_usd: None,
                confidence: CostConfidence::Unknown,
                currency: "USD".to_string(),
                breakdown: "Action regeneration cloud adapter not implemented (explicitly unsupported in Phase 16)".to_string(),
            };

            return RoutingDecision {
                target: RoutingTarget::Unavailable,
                execution_class: ExecutionClass::SpecializedVideoTransformation,
                provider_id: "none".to_string(),
                model_id: "none".to_string(),
                task,
                mode,
                reason: "Action regeneration cloud adapter not implemented (explicitly unsupported in Phase 16)".to_string(),
                estimated_cost: breakdown.to_estimate(),
                cost_breakdown: breakdown,
                fallback_available: false,
                auto_submit_allowed: false,
            };
        }

        // 5. Utility Cloud Tasks (Background Removal)
        if task == TaskClass::BackgroundRemoval {
            let has_adapter =
                registry.has_executable_adapter("replicate_utility", "lucataco/remove-bg");
            if !has_adapter {
                let breakdown = CostBreakdown {
                    provider_id: "replicate_utility".to_string(),
                    model_id: "lucataco/remove-bg".to_string(),
                    billable_duration_sec: duration,
                    resolution: target_res,
                    resolution_tier: None,
                    unit_rate_usd: None,
                    pricing_observed_at: None,
                    segment_count: 1,
                    overlap_duration_sec: 0.0,
                    retry_allowance_usd: 0.0,
                    inference_cost_usd: None,
                    transfer_storage_cost_usd: None,
                    total_usd: None,
                    confidence: CostConfidence::Unknown,
                    currency: "USD".to_string(),
                    breakdown:
                        "Utility background-removal provider adapter not implemented (deferred to Phase 17)"
                            .to_string(),
                };

                return RoutingDecision {
                    target: RoutingTarget::Unavailable,
                    execution_class: ExecutionClass::UtilityCloud,
                    provider_id: "replicate_utility".to_string(),
                    model_id: "lucataco/remove-bg".to_string(),
                    task,
                    mode,
                    reason: "Utility background-removal provider adapter not implemented (deferred to Phase 17)".to_string(),
                    estimated_cost: breakdown.to_estimate(),
                    cost_breakdown: breakdown,
                    fallback_available: false,
                    auto_submit_allowed: false,
                };
            }
        }

        // 6. Character Replacement: Deterministic Candidate Selection
        if task == TaskClass::CharacterReplacement {
            let candidates = registry.find_candidates_for_task(TaskClass::CharacterReplacement);
            let res_tier_res = ResolutionTier::from_dimensions(target_res);

            let mut valid_candidates: Vec<(&ProviderRecord, f64)> = Vec::new();

            for record in candidates {
                if !Self::check_resolution_supported(record, target_res) {
                    continue;
                }
                if !Self::check_fps_supported(record, target_fps) {
                    continue;
                }

                // Compute cost
                let cost = if let Ok(tier) = res_tier_res {
                    if let Some(pt) = record
                        .pricing_tiers
                        .iter()
                        .find(|t| t.resolution_tier == tier.as_str())
                    {
                        pt.pricing_amount * duration
                    } else if let Some(amount) = record.pricing_amount {
                        amount * duration
                    } else {
                        continue;
                    }
                } else if let Some(amount) = record.pricing_amount {
                    amount * duration
                } else {
                    continue;
                };

                valid_candidates.push((record, cost));
            }

            // Deterministic sort by: (cost, provider_id, model_id)
            valid_candidates.sort_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.provider_id.cmp(&b.0.provider_id))
                    .then_with(|| a.0.model_id.cmp(&b.0.model_id))
            });

            if let Some((record, _cost)) = valid_candidates.first() {
                return Self::build_cloud_decision(
                    task,
                    mode,
                    record,
                    request,
                    "Specialized character replacement provider selected",
                );
            } else {
                // Return unavailable with clear explanation
                let breakdown = CostBreakdown {
                    provider_id: "none".to_string(),
                    model_id: "none".to_string(),
                    billable_duration_sec: duration,
                    resolution: target_res,
                    resolution_tier: None,
                    unit_rate_usd: None,
                    pricing_observed_at: None,
                    segment_count: 1,
                    overlap_duration_sec: 0.0,
                    retry_allowance_usd: 0.0,
                    inference_cost_usd: None,
                    transfer_storage_cost_usd: None,
                    total_usd: None,
                    confidence: CostConfidence::Unknown,
                    currency: "USD".to_string(),
                    breakdown: format!(
                        "No character replacement provider supports requested resolution {:?} / fps {:.1}",
                        target_res, target_fps
                    ),
                };

                return RoutingDecision {
                    target: RoutingTarget::Unavailable,
                    execution_class: ExecutionClass::SpecializedVideoTransformation,
                    provider_id: "none".to_string(),
                    model_id: "none".to_string(),
                    task,
                    mode,
                    reason: format!(
                        "No character replacement provider supports requested resolution {:?} / fps {:.1}",
                        target_res, target_fps
                    ),
                    estimated_cost: breakdown.to_estimate(),
                    cost_breakdown: breakdown,
                    fallback_available: false,
                    auto_submit_allowed: false,
                };
            }
        }

        // 7. Full Generative Transformation in non-CostSaving mode
        if task == TaskClass::FullGenerativeTransformation && mode != RoutingPreference::CostSaving
        {
            let candidates =
                registry.find_candidates_for_task(TaskClass::FullGenerativeTransformation);
            if let Some(record) = candidates.first() {
                return Self::build_cloud_decision(
                    task,
                    mode,
                    record,
                    request,
                    "Generative video provider selected for full transformation",
                );
            }
        }

        // 8. Fallback to Local/Hybrid
        let hw_tier = hardware
            .map(|h| h.selected_tier)
            .unwrap_or(CapabilityTier::LowVram);
        let target = match hw_tier {
            CapabilityTier::LowVram | CapabilityTier::UltraLowVram => RoutingTarget::Hybrid,
            CapabilityTier::CpuOnly | CapabilityTier::Unsupported => RoutingTarget::Unavailable,
            _ => RoutingTarget::Local,
        };

        Self::build_local_decision(
            task,
            mode,
            &format!("Fallback to local {:?} execution", target),
            request,
            registry,
        )
    }

    fn check_resolution_supported(record: &ProviderRecord, res: (u32, u32)) -> bool {
        if record.supported_resolutions.is_empty() {
            return true;
        }
        record.supported_resolutions.contains(&res)
            || record.supported_resolutions.contains(&(res.1, res.0))
            || (!record.resolution_tiers.is_empty() && ResolutionTier::from_dimensions(res).is_ok())
    }

    fn check_fps_supported(record: &ProviderRecord, fps: f64) -> bool {
        if record.supports_original_fps {
            return true;
        }
        if record.supported_fps.is_empty() {
            return true;
        }
        record.supported_fps.iter().any(|&f| (f - fps).abs() < 0.5)
    }

    fn build_local_decision(
        task: TaskClass,
        mode: RoutingPreference,
        reason: &str,
        request: &CloudJobRequest,
        registry: &ProviderRegistry,
    ) -> RoutingDecision {
        let record = registry
            .find_by_execution_class(ExecutionClass::LocalDeterministic)
            .cloned()
            .unwrap_or_else(|| ProviderRecord {
                provider_id: "local_ffmpeg".to_string(),
                model_id: "ffmpeg_native".to_string(),
                model_version: "6.0+".to_string(),
                execution_class: ExecutionClass::LocalDeterministic,
                capabilities: super::provider::ProviderCapabilities {
                    supports_text_to_video: false,
                    supports_image_to_video: false,
                    supports_video_to_video: true,
                    supports_reference_image: false,
                    supports_character_reference: false,
                    supports_audio: true,
                    max_duration_sec: None,
                    supported_resolutions: vec![],
                    estimated_cost_per_second: Some(0.0),
                },
                max_duration_sec: None,
                supported_resolutions: vec![],
                supported_fps: vec![],
                supports_original_fps: true,
                pricing_unit: PricingUnit::FreeLocal,
                pricing_amount: Some(0.0),
                pricing_tiers: vec![],
                resolution_tiers: vec![],
                currency: "USD".to_string(),
                source_url: "https://ffmpeg.org".to_string(),
                observed_at: "2026-08-19".to_string(),
            });

        let duration = if request.duration_seconds <= 0.0 {
            6.0
        } else {
            request.duration_seconds
        };
        let breakdown = CostBreakdown {
            provider_id: record.provider_id.clone(),
            model_id: record.model_id.clone(),
            billable_duration_sec: duration,
            resolution: request.resolution,
            resolution_tier: None,
            unit_rate_usd: Some(0.0),
            pricing_observed_at: Some(record.observed_at.clone()),
            segment_count: 1,
            overlap_duration_sec: 0.0,
            retry_allowance_usd: 0.0,
            inference_cost_usd: Some(0.0),
            transfer_storage_cost_usd: Some(0.0),
            total_usd: Some(0.0),
            confidence: CostConfidence::Exact,
            currency: "USD".to_string(),
            breakdown: format!("{}: Free Local Deterministic Execution ($0.00)", reason),
        };

        RoutingDecision {
            target: RoutingTarget::Local,
            execution_class: record.execution_class,
            provider_id: record.provider_id,
            model_id: record.model_id,
            task,
            mode,
            reason: reason.to_string(),
            estimated_cost: breakdown.to_estimate(),
            cost_breakdown: breakdown,
            fallback_available: true,
            auto_submit_allowed: false,
        }
    }

    fn build_cloud_decision(
        task: TaskClass,
        mode: RoutingPreference,
        record: &ProviderRecord,
        request: &CloudJobRequest,
        reason: &str,
    ) -> RoutingDecision {
        let duration = if request.duration_seconds <= 0.0 {
            6.0
        } else {
            request.duration_seconds
        };

        let res_tier = ResolutionTier::from_dimensions(request.resolution).ok();
        let (inf_cost, unit_rate, res_tier_str) = if let Some(tier) = res_tier {
            if let Some(pt) = record
                .pricing_tiers
                .iter()
                .find(|t| t.resolution_tier == tier.as_str())
            {
                (
                    Some(pt.pricing_amount * duration),
                    Some(pt.pricing_amount),
                    Some(tier.as_str().to_string()),
                )
            } else {
                (
                    record.pricing_amount.map(|r| r * duration),
                    record.pricing_amount,
                    None,
                )
            }
        } else {
            (
                record.pricing_amount.map(|r| r * duration),
                record.pricing_amount,
                None,
            )
        };

        let confidence = if inf_cost.is_some() {
            CostConfidence::Estimated
        } else {
            CostConfidence::Unknown
        };

        let breakdown = CostBreakdown {
            provider_id: record.provider_id.clone(),
            model_id: record.model_id.clone(),
            billable_duration_sec: duration,
            resolution: request.resolution,
            resolution_tier: res_tier_str.clone(),
            unit_rate_usd: unit_rate,
            pricing_observed_at: Some(record.observed_at.clone()),
            segment_count: 1,
            overlap_duration_sec: 0.0,
            retry_allowance_usd: 0.0,
            inference_cost_usd: inf_cost,
            transfer_storage_cost_usd: Some(0.0),
            total_usd: inf_cost,
            confidence,
            currency: record.currency.clone(),
            breakdown: format!(
                "Cloud Provider: {} ({}) | Rate: {:?} ${:?}/s | Dur: {:.1}s | Est: ${:.3}",
                record.provider_id,
                record.model_id,
                res_tier_str.unwrap_or_else(|| "default".to_string()),
                unit_rate,
                duration,
                inf_cost.unwrap_or(0.0)
            ),
        };

        RoutingDecision {
            target: RoutingTarget::Cloud,
            execution_class: record.execution_class,
            provider_id: record.provider_id.clone(),
            model_id: record.model_id.clone(),
            task,
            mode,
            reason: reason.to_string(),
            estimated_cost: breakdown.to_estimate(),
            cost_breakdown: breakdown,
            fallback_available: true,
            auto_submit_allowed: true,
        }
    }
}
