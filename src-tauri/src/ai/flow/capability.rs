use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowCapabilitySource {
    LiveFlowUi,
    CachedLiveObservation,
    StaticFallback,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowCapabilityContext {
    UploadedVideoEdit,
    GenericVideoGeneration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowModelCapability {
    pub model_id: String,
    pub display_name: String,
    pub supported_resolutions: Vec<String>,
    pub supported_durations_sec: Vec<u32>,
    pub supported_orientations: Vec<String>,
    pub supported_output_counts: Vec<u32>,
    pub supports_uploaded_video_edit: bool,
    pub source: FlowCapabilitySource,
    pub context: FlowCapabilityContext,
    pub observed_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowModelCapabilitiesSnapshot {
    pub profile_id: String,
    pub operation_context: FlowCapabilityContext,
    pub models: Vec<FlowModelCapability>,
    pub source: FlowCapabilitySource,
    pub observed_at: String,
    pub status: String,
}

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowCapabilityObservation {
    pub profile_id: String,
    pub operation_context: FlowCapabilityContext,
    pub model_id: String,
    pub display_name: String,
    pub supported_resolutions: Vec<String>,
    pub supported_durations_sec: Vec<u32>,
    pub supported_orientations: Vec<String>,
    pub supported_output_counts: Vec<u32>,
    pub supports_uploaded_video_edit: bool,
    pub observed_at: String,
    pub adapter_version: String,
}

#[derive(Debug, Clone, Default)]
pub struct FlowCapabilityObservationStore {
    observations: Arc<RwLock<HashMap<(String, FlowCapabilityContext), FlowCapabilityObservation>>>,
}

impl FlowCapabilityObservationStore {
    pub fn new() -> Self {
        Self {
            observations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn record_observation(&self, observation: FlowCapabilityObservation) {
        if let Ok(mut guard) = self.observations.write() {
            guard.insert(
                (
                    observation.profile_id.clone(),
                    observation.operation_context,
                ),
                observation,
            );
        }
    }

    pub fn get_observation(
        &self,
        profile_id: &str,
        context: FlowCapabilityContext,
    ) -> Option<FlowCapabilityObservation> {
        if let Ok(guard) = self.observations.read() {
            guard.get(&(profile_id.to_string(), context)).cloned()
        } else {
            None
        }
    }

    pub fn get_snapshot(
        &self,
        profile_id: &str,
        operation_context: FlowCapabilityContext,
    ) -> FlowModelCapabilitiesSnapshot {
        if let Some(obs) = self.get_observation(profile_id, operation_context) {
            FlowModelCapabilitiesSnapshot {
                profile_id: profile_id.to_string(),
                operation_context,
                models: vec![FlowModelCapability {
                    model_id: obs.model_id.clone(),
                    display_name: obs.display_name.clone(),
                    supported_resolutions: obs.supported_resolutions.clone(),
                    supported_durations_sec: obs.supported_durations_sec.clone(),
                    supported_orientations: obs.supported_orientations.clone(),
                    supported_output_counts: obs.supported_output_counts.clone(),
                    supports_uploaded_video_edit: obs.supports_uploaded_video_edit,
                    source: FlowCapabilitySource::CachedLiveObservation,
                    context: operation_context,
                    observed_at: obs.observed_at.clone(),
                }],
                source: FlowCapabilitySource::CachedLiveObservation,
                observed_at: obs.observed_at,
                status: "READY".to_string(),
            }
        } else {
            // Static Fallback (not cached live observation, do not fabricate fresh Utc::now())
            let static_observed_at = "2026-08-26T00:00:00Z".to_string();
            let models = match operation_context {
                FlowCapabilityContext::UploadedVideoEdit => {
                    vec![FlowModelCapability {
                        model_id: "Omni Flash".to_string(),
                        display_name: "Omni Flash".to_string(),
                        supported_resolutions: vec!["720p".to_string()], // 1080p is NOT verified for UploadedVideoEdit
                        supported_durations_sec: vec![10],
                        supported_orientations: vec!["9:16".to_string()],
                        supported_output_counts: vec![1],
                        supports_uploaded_video_edit: true,
                        source: FlowCapabilitySource::StaticFallback,
                        context: FlowCapabilityContext::UploadedVideoEdit,
                        observed_at: static_observed_at.clone(),
                    }]
                }
                FlowCapabilityContext::GenericVideoGeneration => {
                    vec![FlowModelCapability {
                        model_id: "Omni Flash".to_string(),
                        display_name: "Omni Flash".to_string(),
                        supported_resolutions: vec!["720p".to_string()],
                        supported_durations_sec: vec![5, 10],
                        supported_orientations: vec!["16:9".to_string(), "9:16".to_string()],
                        supported_output_counts: vec![1, 2, 4],
                        supports_uploaded_video_edit: false,
                        source: FlowCapabilitySource::StaticFallback,
                        context: FlowCapabilityContext::GenericVideoGeneration,
                        observed_at: static_observed_at.clone(),
                    }]
                }
            };

            FlowModelCapabilitiesSnapshot {
                profile_id: profile_id.to_string(),
                operation_context,
                models,
                source: FlowCapabilitySource::StaticFallback,
                observed_at: static_observed_at,
                status: "STATIC_FALLBACK".to_string(),
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowGenerationMode {
    OmniVideoGenerate,
    OmniEditUploadedVideo,
}

/// NOTE: Static planning estimate only. Not an authoritative paid generation cost.
pub const OMNI_EDIT_UPLOADED_VIDEO_ESTIMATED_CREDITS_PER_GENERATION: u32 = 40;
pub const OMNI_VIDEO_GENERATE_ESTIMATED_CREDITS_PER_GENERATION: u32 = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowCapabilityPolicy {
    pub capability_policy_version: u32,
    pub split_policy_version: u32,
    pub max_edit_segment_duration_sec: f64,
    pub mode: FlowGenerationMode,
    pub credits_per_generation: u32,
    pub outputs_per_generation: u32,
    pub automatic_generation_retries: u32,
}

impl Default for FlowCapabilityPolicy {
    fn default() -> Self {
        Self {
            capability_policy_version: 1,
            split_policy_version: 1,
            max_edit_segment_duration_sec: 10.0,
            mode: FlowGenerationMode::OmniEditUploadedVideo,
            credits_per_generation: OMNI_EDIT_UPLOADED_VIDEO_ESTIMATED_CREDITS_PER_GENERATION,
            outputs_per_generation: 1,
            automatic_generation_retries: 0,
        }
    }
}

impl FlowCapabilityPolicy {
    pub fn for_edit_uploaded_video() -> Self {
        Self::default()
    }

    /// Calculate static planning credit estimate. The authoritative cost must come from live Flow active edit.
    pub fn estimate_credits(&self, segment_count: usize) -> u32 {
        (segment_count as u32) * self.outputs_per_generation * self.credits_per_generation
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowCreditRecord {
    pub estimated_credits: u32,
    #[serde(default)]
    pub observed_credit_balance: Option<u32>,
    pub completed_generations: u32,
    #[serde(default)]
    pub credit_budget_limit: Option<u32>,
    #[serde(default)]
    pub reserved_credits: u32,
}

impl Default for FlowCreditRecord {
    fn default() -> Self {
        Self {
            estimated_credits: 0,
            observed_credit_balance: None,
            completed_generations: 0,
            credit_budget_limit: None,
            reserved_credits: 0,
        }
    }
}
