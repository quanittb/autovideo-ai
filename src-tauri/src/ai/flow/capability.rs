use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowGenerationMode {
    OmniVideoGenerate,
    OmniEditUploadedVideo,
}

pub const OMNI_EDIT_UPLOADED_VIDEO_CREDITS_PER_GENERATION: u32 = 40;
pub const OMNI_VIDEO_GENERATE_CREDITS_PER_GENERATION: u32 = 20;

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
            credits_per_generation: OMNI_EDIT_UPLOADED_VIDEO_CREDITS_PER_GENERATION,
            outputs_per_generation: 1,
            automatic_generation_retries: 0,
        }
    }
}

impl FlowCapabilityPolicy {
    pub fn for_edit_uploaded_video() -> Self {
        Self::default()
    }

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
