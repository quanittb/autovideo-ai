use super::capability::FlowCreditRecord;
use super::prompt_optimizer::PromptSource;
use crate::ai::cloud::job::{JobErrorRecord, JobTimestamps};
use crate::ai::cloud::manifest::SegmentBoundary;
use crate::ai::cloud::spec::SourceMediaFacts;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const CURRENT_FLOW_MANIFEST_SCHEMA_VERSION: u32 = 5;

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
// 2. Child Submission State & Long-Video Types
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowJobKind {
    SingleSegment,
    LongVideoParent,
    LongVideoChild,
}

impl Default for FlowJobKind {
    fn default() -> Self {
        Self::SingleSegment
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowIdentityContinuityStrategy {
    SamePromptBaseline,
    ReferenceAnchor,
    OverlapVisualAnchor,
    Unsupported,
}

impl Default for FlowIdentityContinuityStrategy {
    fn default() -> Self {
        Self::SamePromptBaseline
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowFaceContinuityStatus {
    Pass,
    Fail,
    Unverified,
    NoFace,
}

impl Default for FlowFaceContinuityStatus {
    fn default() -> Self {
        Self::Unverified
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowSeamStatus {
    Pass,
    Fail,
    Unverified,
}

impl Default for FlowSeamStatus {
    fn default() -> Self {
        Self::Unverified
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowContinuityEvidence {
    pub boundary_index: usize,
    pub previous_segment_index: usize,
    pub next_segment_index: usize,
    #[serde(default)]
    pub previous_end_frame_paths: Vec<PathBuf>,
    #[serde(default)]
    pub next_start_frame_paths: Vec<PathBuf>,
    #[serde(default)]
    pub contact_sheet_path: Option<PathBuf>,
    pub face_continuity_status: FlowFaceContinuityStatus,
    pub seam_status: FlowSeamStatus,
    #[serde(default)]
    pub metric_name: Option<String>,
    #[serde(default)]
    pub metric_category: Option<String>,
    #[serde(default)]
    pub metric_value: Option<f64>,
    #[serde(default)]
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRationalFrameRate {
    #[serde(alias = "fpsNumerator")]
    pub numerator: u32,
    #[serde(alias = "fpsDenominator")]
    pub denominator: u32,
}

impl Default for FlowRationalFrameRate {
    fn default() -> Self {
        Self {
            numerator: 30,
            denominator: 1,
        }
    }
}

impl FlowRationalFrameRate {
    pub fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator: numerator.max(1),
            denominator: denominator.max(1),
        }
    }

    pub fn to_f64(&self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    pub fn to_ffmpeg_arg(&self) -> String {
        format!("{}/{}", self.numerator, self.denominator)
    }

    pub fn expected_duration_sec(&self, frame_count: u64) -> f64 {
        (frame_count as f64 * self.denominator as f64) / (self.numerator as f64)
    }
}

impl From<(u32, u32)> for FlowRationalFrameRate {
    fn from(t: (u32, u32)) -> Self {
        Self::new(t.0, t.1)
    }
}

impl From<f64> for FlowRationalFrameRate {
    fn from(v: f64) -> Self {
        if (v - 29.97).abs() < 0.01 {
            Self::new(30000, 1001)
        } else if (v - 23.976).abs() < 0.01 {
            Self::new(24000, 1001)
        } else {
            let num = (v * 1000.0).round() as u32;
            Self::new(num, 1000)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNormalizedSegment {
    pub segment_index: usize,
    pub path: PathBuf,
    pub frame_count: u64,
    pub sha256: String,
}

impl FlowNormalizedSegment {
    pub fn new(segment_index: usize, path: PathBuf, frame_count: u64, sha256: String) -> Self {
        Self {
            segment_index,
            path,
            frame_count,
            sha256,
        }
    }

    pub fn from_path(segment_index: usize, path: PathBuf) -> Self {
        Self {
            segment_index,
            path,
            frame_count: 0,
            sha256: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowAudioRestorationMode {
    StreamCopy,
    DeterministicTranscode,
    NoSourceAudio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowCanonicalGeometry {
    pub width: u32,
    pub height: u32,
    pub orientation: String,
    #[serde(default = "default_sar")]
    pub sar: String,
}

fn default_sar() -> String {
    "1:1".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FlowParentLedger {
    pub segment_count: usize,
    pub planning_cost_estimate: u32,
    pub authoritative_committed_credits: u32,
    pub reserved_credits: u32,
    pub completed_paid_segments: usize,
    #[serde(default)]
    pub dispatched_paid_clicks: usize,
    #[serde(default)]
    pub max_total_credits: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowUploadedSourceEvidence {
    pub segment_index: usize,
    pub expected_file_name: String,
    pub observed_file_name: String,
    pub expected_duration: f64,
    #[serde(default)]
    pub observed_duration: Option<f64>,
    pub evidence_timestamp: String,
    #[serde(default)]
    pub active_card_identity: Option<String>,
    #[serde(default)]
    pub edit_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowPlannedSegment {
    pub segment_index: usize,
    pub start_frame: u64,
    pub end_frame: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub planned_duration_sec: f64,
    pub planned_frame_count: u64,
    pub source_segment_path: PathBuf,
    pub source_segment_sha256: String,
    #[serde(default)]
    pub child_job_id: Option<String>,
    pub state: FlowJobState,
    #[serde(default)]
    pub local_submission_attempt_id: Option<String>,
    #[serde(default)]
    pub submission_state: FlowChildSubmissionState,
    #[serde(default)]
    pub submission_evidence: Option<String>,
    #[serde(default)]
    pub uploaded_source_evidence: Option<FlowUploadedSourceEvidence>,
    #[serde(default)]
    pub click_dispatched: bool,
    #[serde(default)]
    pub preclick_cost: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowLongVideoPlan {
    pub parent_job_id: String,
    pub project_id: String,
    #[serde(default)]
    pub source_media_id: Option<String>,
    pub source_duration_ms: u64,
    pub source_fps_rational: (u32, u32),
    #[serde(default)]
    pub rational_fps: Option<FlowRationalFrameRate>,
    #[serde(default)]
    pub fps_numerator: Option<u32>,
    #[serde(default)]
    pub fps_denominator: Option<u32>,
    pub source_timing_mode: String,
    pub working_proxy_created: bool,
    #[serde(default)]
    pub working_proxy_path: Option<PathBuf>,
    #[serde(default)]
    pub working_proxy_sha256: Option<String>,
    pub strategy: String,
    pub segment_count: usize,
    pub segments: Vec<FlowPlannedSegment>,
    pub requested_config: FlowRequestedGenerationConfig,
    pub prompt_hash: String,
    pub transformation_intent: crate::ai::transformation::TransformationIntent,
    pub identity_mode: crate::ai::transformation::IdentityMode,
    pub continuity_strategy: FlowIdentityContinuityStrategy,
    pub identity_continuity_guaranteed: bool,
    pub created_at: String,
}

impl FlowLongVideoPlan {
    pub fn get_rational_fps(&self) -> FlowRationalFrameRate {
        if let Some(r) = self.rational_fps {
            r
        } else if let (Some(num), Some(den)) = (self.fps_numerator, self.fps_denominator) {
            FlowRationalFrameRate::new(num, den)
        } else {
            FlowRationalFrameRate::new(self.source_fps_rational.0, self.source_fps_rational.1)
        }
    }
}

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
    #[serde(default)]
    pub job_kind: FlowJobKind,
    #[serde(default)]
    pub long_video_plan: Option<FlowLongVideoPlan>,
    #[serde(default)]
    pub parent_ledger: Option<FlowParentLedger>,
    #[serde(default)]
    pub continuity_strategy: Option<FlowIdentityContinuityStrategy>,
    #[serde(default)]
    pub continuity_evidence: Vec<FlowContinuityEvidence>,
    #[serde(default)]
    pub audio_restoration_mode: Option<FlowAudioRestorationMode>,
    #[serde(default)]
    pub canonical_geometry: Option<FlowCanonicalGeometry>,
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
            job_kind: FlowJobKind::SingleSegment,
            long_video_plan: None,
            parent_ledger: None,
            continuity_strategy: None,
            continuity_evidence: Vec::new(),
            audio_restoration_mode: None,
            canonical_geometry: None,
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
            job_kind: Some(self.job_kind),
            continuity_strategy: self.continuity_strategy,
            parent_ledger: self.parent_ledger.clone(),
            audio_restoration_mode: self.audio_restoration_mode,
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
    #[serde(default)]
    pub job_kind: Option<FlowJobKind>,
    #[serde(default)]
    pub continuity_strategy: Option<FlowIdentityContinuityStrategy>,
    #[serde(default)]
    pub parent_ledger: Option<FlowParentLedger>,
    #[serde(default)]
    pub audio_restoration_mode: Option<FlowAudioRestorationMode>,
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
