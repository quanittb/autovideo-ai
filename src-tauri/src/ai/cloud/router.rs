use super::cost::{CostBreakdown, CostConfidence, CostEstimate};
use super::job::CloudJobRequest;
use super::provider::CloudVideoProvider;
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
    pub fn from_str_or_default(s: &str) -> Self {
        let normalized = s.trim().to_uppercase().replace('-', "_");
        match normalized.as_str() {
            "CHARACTER_REPLACEMENT" | "CHARACTER" => TaskClass::CharacterReplacement,
            "BACKGROUND_REMOVAL" | "REMOVE_BG" | "REMOVE_BACKGROUND" => {
                TaskClass::BackgroundRemoval
            }
            "BACKGROUND_COMPOSITE" | "BACKGROUND_REPLACEMENT" | "BACKGROUND" => {
                TaskClass::BackgroundComposite
            }
            "STYLE_FILTER" | "STYLE_TRANSFORMATION" | "STYLE" => TaskClass::StyleFilter,
            "AUDIO_TRANSFORMATION" | "AUDIO_MUX" | "AUDIO" => TaskClass::AudioTransformation,
            "ACTION_REGENERATION" | "ACTION_TRANSFORMATION" | "ACTION" => {
                TaskClass::ActionRegeneration
            }
            "FULL_GENERATIVE_TRANSFORMATION" | "FULL_TRANSFORMATION" | "FULL" | "GENERATIVE" => {
                TaskClass::FullGenerativeTransformation
            }
            _ => TaskClass::CharacterReplacement,
        }
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
        cloud_provider: &dyn CloudVideoProvider,
        hardware: Option<&CapabilityReport>,
    ) -> RoutingDecision {
        let registry = ProviderRegistry::new();
        Self::route_with_registry(task, mode, request, cloud_provider, hardware, &registry)
    }

    pub fn route_with_registry(
        task: TaskClass,
        mode: RoutingPreference,
        request: &CloudJobRequest,
        cloud_provider: &dyn CloudVideoProvider,
        hardware: Option<&CapabilityReport>,
        registry: &ProviderRegistry,
    ) -> RoutingDecision {
        let is_cloud_auth_configured = cloud_provider.is_configured();
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

        // 2. Local Deterministic Tasks: Crop, color, style filters, audio mux, background compositing
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

        // 3. Full Generative Transformation Guard: Disabled in CostSaving mode unless short/preview
        if task == TaskClass::FullGenerativeTransformation && mode == RoutingPreference::CostSaving
        {
            let breakdown = CostBreakdown {
                provider_id: "none".to_string(),
                model_id: "none".to_string(),
                billable_duration_sec: duration,
                resolution: target_res,
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

        // 4. Utility Cloud Tasks (Background Removal)
        if task == TaskClass::BackgroundRemoval {
            // Check if executable adapter exists in providers/
            let has_adapter = registry.has_executable_adapter("replicate_utility");
            if !has_adapter {
                let breakdown = CostBreakdown {
                    provider_id: "replicate_utility".to_string(),
                    model_id: "lucataco/remove-bg".to_string(),
                    billable_duration_sec: duration,
                    resolution: target_res,
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

        // 5. Specialized Video Transformation Tasks (Character Replacement, Action Regeneration)
        if matches!(
            task,
            TaskClass::CharacterReplacement | TaskClass::ActionRegeneration
        ) {
            // Capability truth check: Character replacement requires video-to-video / character reference serialization.
            // Current Replicate MiniMax adapter only serializes prompt & prompt_optimizer (text-to-video).
            // Therefore, character replacement cannot be executed by the current MiniMax adapter.
            let record_opt =
                registry.find_by_execution_class(ExecutionClass::SpecializedVideoTransformation);
            if let Some(record) = record_opt {
                let adapter_supports_character_inputs = record.capabilities.supports_video_to_video
                    && record.capabilities.supports_character_reference;

                if !adapter_supports_character_inputs {
                    let breakdown = CostBreakdown {
                        provider_id: record.provider_id.clone(),
                        model_id: record.model_id.clone(),
                        billable_duration_sec: duration,
                        resolution: target_res,
                        segment_count: 1,
                        overlap_duration_sec: 0.0,
                        retry_allowance_usd: 0.0,
                        inference_cost_usd: None,
                        transfer_storage_cost_usd: None,
                        total_usd: None,
                        confidence: CostConfidence::Unknown,
                        currency: "USD".to_string(),
                        breakdown:
                            "Specialized provider adapter not implemented for required character replacement inputs (deferred to Phase 16)"
                                .to_string(),
                    };

                    return RoutingDecision {
                        target: RoutingTarget::Unavailable,
                        execution_class: record.execution_class,
                        provider_id: record.provider_id.clone(),
                        model_id: record.model_id.clone(),
                        task,
                        mode,
                        reason: "Specialized provider adapter not implemented for required character replacement inputs (deferred to Phase 16)".to_string(),
                        estimated_cost: breakdown.to_estimate(),
                        cost_breakdown: breakdown,
                        fallback_available: false,
                        auto_submit_allowed: false,
                    };
                }

                // Check resolution & FPS constraints
                if !Self::check_resolution_supported(record, target_res) {
                    let breakdown = CostBreakdown {
                        provider_id: record.provider_id.clone(),
                        model_id: record.model_id.clone(),
                        billable_duration_sec: duration,
                        resolution: target_res,
                        segment_count: 1,
                        overlap_duration_sec: 0.0,
                        retry_allowance_usd: 0.0,
                        inference_cost_usd: None,
                        transfer_storage_cost_usd: None,
                        total_usd: None,
                        confidence: CostConfidence::Unknown,
                        currency: "USD".to_string(),
                        breakdown: format!(
                            "Unsupported resolution {:?} by provider {}",
                            target_res, record.provider_id
                        ),
                    };

                    return RoutingDecision {
                        target: RoutingTarget::Unavailable,
                        execution_class: record.execution_class,
                        provider_id: record.provider_id.clone(),
                        model_id: record.model_id.clone(),
                        task,
                        mode,
                        reason: format!(
                            "Requested resolution {:?} is not supported by provider {}. Supported resolutions: {:?}",
                            target_res, record.provider_id, record.supported_resolutions
                        ),
                        estimated_cost: breakdown.to_estimate(),
                        cost_breakdown: breakdown,
                        fallback_available: false,
                        auto_submit_allowed: false,
                    };
                }

                if !Self::check_fps_supported(record, target_fps) {
                    let breakdown = CostBreakdown {
                        provider_id: record.provider_id.clone(),
                        model_id: record.model_id.clone(),
                        billable_duration_sec: duration,
                        resolution: target_res,
                        segment_count: 1,
                        overlap_duration_sec: 0.0,
                        retry_allowance_usd: 0.0,
                        inference_cost_usd: None,
                        transfer_storage_cost_usd: None,
                        total_usd: None,
                        confidence: CostConfidence::Unknown,
                        currency: "USD".to_string(),
                        breakdown: format!(
                            "Unsupported frame rate {:.1} fps by provider {}",
                            target_fps, record.provider_id
                        ),
                    };

                    return RoutingDecision {
                        target: RoutingTarget::Unavailable,
                        execution_class: record.execution_class,
                        provider_id: record.provider_id.clone(),
                        model_id: record.model_id.clone(),
                        task,
                        mode,
                        reason: format!(
                            "Requested frame rate {:.1} fps is not supported by provider {}. Supported fps: {:?}",
                            target_fps, record.provider_id, record.supported_fps
                        ),
                        estimated_cost: breakdown.to_estimate(),
                        cost_breakdown: breakdown,
                        fallback_available: false,
                        auto_submit_allowed: false,
                    };
                }

                return Self::build_cloud_decision(
                    task,
                    mode,
                    record,
                    request,
                    is_cloud_auth_configured,
                    "Specialized video transformation provider selected",
                );
            }
        }

        // 6. Fallback to Local/Hybrid
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
    }

    fn check_fps_supported(record: &ProviderRecord, fps: f64) -> bool {
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
                    max_duration_sec: 3600.0,
                    supported_resolutions: vec![],
                    estimated_cost_per_second: Some(0.0),
                },
                max_duration_sec: None,
                supported_resolutions: vec![],
                supported_fps: vec![],
                pricing_unit: PricingUnit::FreeLocal,
                pricing_amount: Some(0.0),
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
            segment_count: 1,
            overlap_duration_sec: 0.0,
            retry_allowance_usd: 0.0,
            inference_cost_usd: Some(0.0),
            transfer_storage_cost_usd: Some(0.0),
            total_usd: Some(0.0),
            confidence: CostConfidence::Exact,
            currency: "USD".to_string(),
            breakdown: "$0.00 local deterministic processing".to_string(),
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
            fallback_available: false,
            auto_submit_allowed: true,
        }
    }

    fn build_cloud_decision(
        task: TaskClass,
        mode: RoutingPreference,
        record: &ProviderRecord,
        request: &CloudJobRequest,
        is_cloud_auth_configured: bool,
        reason: &str,
    ) -> RoutingDecision {
        let duration = if request.duration_seconds <= 0.0 {
            6.0
        } else {
            request.duration_seconds
        };
        let seg_len = record.max_duration_sec.unwrap_or(6.0).min(6.0);
        let segment_count = ((duration / seg_len).ceil() as usize).max(1);

        let (inf_cost, confidence) = match (record.pricing_unit, record.pricing_amount) {
            (PricingUnit::PerSecond, Some(rate)) => {
                (Some(rate * duration), CostConfidence::Estimated)
            }
            (PricingUnit::PerPrediction, Some(fee)) => {
                (Some(fee * segment_count as f64), CostConfidence::Estimated)
            }
            (PricingUnit::FreeLocal, Some(0.0)) => (Some(0.0), CostConfidence::Exact),
            _ => (None, CostConfidence::Unknown),
        };

        let breakdown = CostBreakdown {
            provider_id: record.provider_id.clone(),
            model_id: record.model_id.clone(),
            billable_duration_sec: duration,
            resolution: request.resolution,
            segment_count,
            overlap_duration_sec: 0.0,
            retry_allowance_usd: 0.0,
            inference_cost_usd: inf_cost,
            transfer_storage_cost_usd: Some(0.0),
            total_usd: inf_cost,
            confidence,
            currency: record.currency.clone(),
            breakdown: format!(
                "Provider: {} | Rate: {:?} ${:?} | Dur: {:.1}s ({} segs)",
                record.provider_id,
                record.pricing_unit,
                record.pricing_amount,
                duration,
                segment_count
            ),
        };

        if !is_cloud_auth_configured {
            return RoutingDecision {
                target: RoutingTarget::Unavailable,
                execution_class: record.execution_class,
                provider_id: record.provider_id.clone(),
                model_id: record.model_id.clone(),
                task,
                mode,
                reason: format!(
                    "CLOUD provider {} selected but credentials (e.g. REPLICATE_API_TOKEN) are unconfigured",
                    record.provider_id
                ),
                estimated_cost: breakdown.to_estimate(),
                cost_breakdown: breakdown,
                fallback_available: mode != RoutingPreference::CloudOnly,
                auto_submit_allowed: false,
            };
        }

        let auto_submit = confidence != CostConfidence::Unknown && inf_cost.is_some();

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
            fallback_available: mode != RoutingPreference::CloudOnly,
            auto_submit_allowed: auto_submit,
        }
    }
}
