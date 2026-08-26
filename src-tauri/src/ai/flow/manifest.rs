use super::capability::FlowCreditRecord;
use super::prompt_optimizer::PromptSource;
use crate::ai::cloud::job::{JobErrorRecord, JobTimestamps};
use crate::ai::cloud::manifest::SegmentBoundary;
use crate::ai::cloud::spec::SourceMediaFacts;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const CURRENT_FLOW_MANIFEST_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRequestedGenerationConfig {
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub duration_sec: Option<u32>,
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default = "default_output_count")]
    pub output_count: u32,
}

fn default_output_count() -> u32 {
    1
}

impl Default for FlowRequestedGenerationConfig {
    fn default() -> Self {
        Self {
            model_id: Some("Omni Flash".to_string()),
            resolution: Some("720p".to_string()),
            duration_sec: Some(10),
            orientation: Some("PORTRAIT".to_string()),
            output_count: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FlowObservedGenerationConfig {
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub duration_sec: Option<u32>,
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default)]
    pub output_count: Option<u32>,
}

// -----------------------------------------------------------------------------
// 1. 21-State Flow Machine
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowJobState {
    Planning,
    Splitting,
    Ready,

    WaitingForBrowser,
    LoginRequired,

    Uploading,
    ReadyToSubmit,
    Submitting,
    GenerationAmbiguous,
    Generating,

    Downloading,
    ValidatingSegment,

    Stitching,
    ValidatingFinal,

    Completed,
    Failed,
    Cancelled,
    Blocked,
    CreditsRequired,
    FlowUiChanged,
    UserActionRequired,
}

impl FlowJobState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            FlowJobState::Completed
                | FlowJobState::Failed
                | FlowJobState::Cancelled
                | FlowJobState::Blocked
                | FlowJobState::UserActionRequired
        )
    }

    pub fn can_transition_to(&self, next: FlowJobState) -> bool {
        if *self == next {
            return true;
        }
        if self.is_terminal() {
            // Once terminal, only manual unblock / retry can move out
            return matches!(
                (self, next),
                (FlowJobState::Blocked, FlowJobState::Ready)
                    | (FlowJobState::UserActionRequired, FlowJobState::Ready)
                    | (FlowJobState::LoginRequired, FlowJobState::Ready)
                    | (FlowJobState::CreditsRequired, FlowJobState::Ready)
            );
        }

        // Cancellation can happen from any non-terminal state
        if next == FlowJobState::Cancelled {
            return true;
        }

        // Failure can happen from any non-terminal state
        if next == FlowJobState::Failed || next == FlowJobState::Blocked {
            return true;
        }

        true
    }
}

// -----------------------------------------------------------------------------
// 2. Child Submission State
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowChildSubmissionState {
    NeverAttempted,
    AttemptPersisted,
    Ambiguous,
    ProvenSubmitted,
    ProvenCompleted,
}

impl Default for FlowChildSubmissionState {
    fn default() -> Self {
        Self::NeverAttempted
    }
}

// -----------------------------------------------------------------------------
// 3. Audio & Artifact Records
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowFinalAudioPolicy {
    pub preserve_original_audio: bool,
    pub codec: String,
}

impl Default for FlowFinalAudioPolicy {
    fn default() -> Self {
        Self {
            preserve_original_audio: true,
            codec: "aac".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowOutputArtifactRecord {
    pub final_path: PathBuf,
    pub sha256: String,
    pub duration_sec: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frame_count: u64,
    pub has_audio: bool,
    pub validated_at: String,
}

// -----------------------------------------------------------------------------
// 4. Segment Plan & Child Records
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowSegmentPlan {
    pub segments: Vec<SegmentBoundary>,
    pub total_frames: u64,
    pub total_duration_sec: f64,
    pub target_fps: f64,
    pub capability_limit_sec: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowChildSegmentRecord {
    pub segment_index: usize,
    pub segment_file_name: String,
    pub segment_sha256: String,
    pub start_frame: u64,
    pub end_frame: u64,
    pub start_pts: u64,
    pub end_pts: u64,
    pub duration_sec: f64,
    pub state: FlowJobState,
    pub submission_state: FlowChildSubmissionState,
    #[serde(default)]
    pub local_submission_attempt_id: Option<String>,
    #[serde(default)]
    pub submission_evidence: Option<String>,
    #[serde(default)]
    pub download_artifact_path: Option<PathBuf>,
    #[serde(default)]
    pub download_artifact_sha: Option<String>,
    pub timestamps: JobTimestamps,
}

// -----------------------------------------------------------------------------
// 5. Hardened FlowGenerationManifest
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowGenerationManifest {
    pub schema_version: u32,
    pub state_revision: u64,
    pub parent_id: String,
    pub client_request_id: String,
    pub configuration_hash: String,
    pub project_id: String,
    pub profile_id: String,
    pub source_media_id: Option<String>,
    pub source_content_hash: String,
    pub source_file_name: Option<String>,
    #[serde(default)]
    pub transformation_intent: crate::ai::transformation::TransformationIntent,
    #[serde(default)]
    pub identity_mode: crate::ai::transformation::IdentityMode,
    #[serde(default)]
    pub target_face: Option<crate::ai::transformation::TargetFaceSelection>,
    #[serde(default)]
    pub requested_generation_config: FlowRequestedGenerationConfig,
    #[serde(default)]
    pub observed_generation_config_at_submission: Option<FlowObservedGenerationConfig>,
    pub submitted_prompt: String,
    pub prompt_hash: String,
    pub prompt_source: PromptSource,
    pub capability_policy_version: u32,
    pub split_policy_version: u32,
    pub state: FlowJobState,
    pub source_facts: SourceMediaFacts,
    pub segment_plan: FlowSegmentPlan,
    pub child_segments: Vec<FlowChildSegmentRecord>,
    pub credit_record: FlowCreditRecord,
    pub active_segment_index: usize,
    pub final_audio_policy: FlowFinalAudioPolicy,
    pub final_output: Option<FlowOutputArtifactRecord>,
    pub cancellation_requested: bool,
    pub error: Option<JobErrorRecord>,
    pub timestamps: JobTimestamps,
}

impl FlowGenerationManifest {
    pub fn new(
        parent_id: String,
        client_request_id: String,
        project_id: String,
        profile_id: String,
        configuration_hash: String,
        source_media_id: Option<String>,
        source_content_hash: String,
        source_file_name: Option<String>,
        transformation_intent: crate::ai::transformation::TransformationIntent,
        identity_mode: crate::ai::transformation::IdentityMode,
        target_face: Option<crate::ai::transformation::TargetFaceSelection>,
        requested_generation_config: FlowRequestedGenerationConfig,
        submitted_prompt: String,
        prompt_hash: String,
        prompt_source: PromptSource,
        capability_policy_version: u32,
        split_policy_version: u32,
        source_facts: SourceMediaFacts,
        segment_plan: FlowSegmentPlan,
        credit_record: FlowCreditRecord,
        final_audio_policy: FlowFinalAudioPolicy,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            schema_version: CURRENT_FLOW_MANIFEST_SCHEMA_VERSION,
            state_revision: 1,
            parent_id,
            client_request_id,
            configuration_hash,
            project_id,
            profile_id,
            source_media_id,
            source_content_hash,
            source_file_name,
            transformation_intent,
            identity_mode,
            target_face,
            requested_generation_config,
            observed_generation_config_at_submission: None,
            submitted_prompt,
            prompt_hash,
            prompt_source,
            capability_policy_version,
            split_policy_version,
            state: FlowJobState::Planning,
            source_facts,
            segment_plan,
            child_segments: Vec::new(),
            credit_record,
            active_segment_index: 0,
            final_audio_policy,
            final_output: None,
            cancellation_requested: false,
            error: None,
            timestamps: JobTimestamps {
                created_at: now.clone(),
                updated_at: now,
                submitted_at: None,
                completed_at: None,
            },
        }
    }

    pub fn to_snapshot(&self) -> FlowJobSnapshot {
        let total_segments =
            std::cmp::max(self.segment_plan.segments.len(), self.child_segments.len());

        FlowJobSnapshot {
            parent_id: self.parent_id.clone(),
            project_id: self.project_id.clone(),
            profile_id: self.profile_id.clone(),
            submitted_prompt: self.submitted_prompt.clone(),
            prompt_hash: self.prompt_hash.clone(),
            prompt_source: self.prompt_source,
            transformation_intent: Some(self.transformation_intent),
            identity_mode: Some(self.identity_mode),
            target_face: self.target_face.clone(),
            requested_generation_config: self.requested_generation_config.clone(),
            observed_generation_config: self.observed_generation_config_at_submission.clone(),
            state: self.state,
            state_revision: self.state_revision,
            active_segment_index: self.active_segment_index,
            total_segments,
            estimated_credits: self.credit_record.estimated_credits,
            observed_credit_balance: self.credit_record.observed_credit_balance,
            credit_budget_limit: self.credit_record.credit_budget_limit,
            reserved_credits: self.credit_record.reserved_credits,
            completed_generations: self.credit_record.completed_generations,
            final_output_ready: self.final_output.is_some(),
            final_output_path: self
                .final_output
                .as_ref()
                .map(|o| o.final_path.to_string_lossy().to_string()),
            error_code: self.error.as_ref().map(|e| e.code.clone()),
            error_message: self.error.as_ref().map(|e| e.sanitized_message.clone()),
            timestamps: self.timestamps.clone(),
        }
    }
}

// -----------------------------------------------------------------------------
// 6. Safe Event & DTO Payloads
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowJobSnapshot {
    pub parent_id: String,
    pub project_id: String,
    pub profile_id: String,
    pub submitted_prompt: String,
    pub prompt_hash: String,
    pub prompt_source: PromptSource,
    #[serde(default)]
    pub transformation_intent: Option<crate::ai::transformation::TransformationIntent>,
    #[serde(default)]
    pub identity_mode: Option<crate::ai::transformation::IdentityMode>,
    #[serde(default)]
    pub target_face: Option<crate::ai::transformation::TargetFaceSelection>,
    #[serde(default)]
    pub requested_generation_config: FlowRequestedGenerationConfig,
    #[serde(default)]
    pub observed_generation_config: Option<FlowObservedGenerationConfig>,
    pub state: FlowJobState,
    pub state_revision: u64,
    pub active_segment_index: usize,
    pub total_segments: usize,
    pub estimated_credits: u32,
    pub observed_credit_balance: Option<u32>,
    pub credit_budget_limit: Option<u32>,
    #[serde(default)]
    pub reserved_credits: u32,
    pub completed_generations: u32,
    pub final_output_ready: bool,
    pub final_output_path: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub timestamps: JobTimestamps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowJobEventPayload {
    pub parent_id: String,
    pub project_id: String,
    pub profile_id: String,
    pub state: FlowJobState,
    pub state_revision: u64,
    pub active_segment_index: usize,
    pub total_segments: usize,
    pub progress_pct: f64,
    pub final_output_ready: bool,
    pub error: Option<JobErrorRecord>,
    pub prompt_source: PromptSource,
}
