use super::job::{CloudJobState, JobErrorRecord, JobTimestamps, OutputArtifactRecord};
use super::spec::{DetailedTimingFacts, SourceMediaFacts};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SegmentedJobState {
    Planning,
    Splitting,
    CostApprovalRequired,
    Ready,
    Running,
    Stitching,
    ValidatingOutput,
    Completed,
    Failed,
    Blocked,
    Cancelled,
}

impl SegmentedJobState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SegmentedJobState::Completed
                | SegmentedJobState::Failed
                | SegmentedJobState::Blocked
                | SegmentedJobState::Cancelled
        )
    }

    pub fn can_transition_to(&self, next: SegmentedJobState) -> bool {
        if self.is_terminal() {
            return false;
        }

        match (self, next) {
            // Planning can move to Splitting, Failed, Blocked, Cancelled
            (SegmentedJobState::Planning, SegmentedJobState::Splitting)
            | (SegmentedJobState::Planning, SegmentedJobState::Failed)
            | (SegmentedJobState::Planning, SegmentedJobState::Blocked)
            | (SegmentedJobState::Planning, SegmentedJobState::Cancelled) => true,

            // Splitting can move to CostApprovalRequired, Ready, Failed, Blocked, Cancelled
            (SegmentedJobState::Splitting, SegmentedJobState::CostApprovalRequired)
            | (SegmentedJobState::Splitting, SegmentedJobState::Ready)
            | (SegmentedJobState::Splitting, SegmentedJobState::Failed)
            | (SegmentedJobState::Splitting, SegmentedJobState::Blocked)
            | (SegmentedJobState::Splitting, SegmentedJobState::Cancelled) => true,

            // CostApprovalRequired can move to Ready (after approval), Failed, Blocked, Cancelled
            (SegmentedJobState::CostApprovalRequired, SegmentedJobState::Ready)
            | (SegmentedJobState::CostApprovalRequired, SegmentedJobState::Failed)
            | (SegmentedJobState::CostApprovalRequired, SegmentedJobState::Blocked)
            | (SegmentedJobState::CostApprovalRequired, SegmentedJobState::Cancelled) => true,

            // Ready can move to Running, Failed, Blocked, Cancelled
            (SegmentedJobState::Ready, SegmentedJobState::Running)
            | (SegmentedJobState::Ready, SegmentedJobState::Failed)
            | (SegmentedJobState::Ready, SegmentedJobState::Blocked)
            | (SegmentedJobState::Ready, SegmentedJobState::Cancelled) => true,

            // Running can move to Stitching, Failed, Blocked, Cancelled
            (SegmentedJobState::Running, SegmentedJobState::Stitching)
            | (SegmentedJobState::Running, SegmentedJobState::Failed)
            | (SegmentedJobState::Running, SegmentedJobState::Blocked)
            | (SegmentedJobState::Running, SegmentedJobState::Cancelled) => true,

            // Stitching can move to ValidatingOutput, Failed, Blocked, Cancelled
            (SegmentedJobState::Stitching, SegmentedJobState::ValidatingOutput)
            | (SegmentedJobState::Stitching, SegmentedJobState::Failed)
            | (SegmentedJobState::Stitching, SegmentedJobState::Blocked)
            | (SegmentedJobState::Stitching, SegmentedJobState::Cancelled) => true,

            // ValidatingOutput can move to Completed, Failed, Blocked, Cancelled
            (SegmentedJobState::ValidatingOutput, SegmentedJobState::Completed)
            | (SegmentedJobState::ValidatingOutput, SegmentedJobState::Failed)
            | (SegmentedJobState::ValidatingOutput, SegmentedJobState::Blocked)
            | (SegmentedJobState::ValidatingOutput, SegmentedJobState::Cancelled) => true,

            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalAudioPolicy {
    pub preserve_original_audio: bool,
    pub codec: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentBoundary {
    pub index: usize,
    pub start_frame: u64,
    pub end_frame: u64,
    pub start_pts: u64,
    pub end_pts: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub expected_duration_sec: f64,
    #[serde(default)]
    pub start_sec: f64,
    #[serde(default)]
    pub end_sec: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentPlan {
    pub plan_id: String,
    pub source_facts: SourceMediaFacts,
    pub timing_facts: DetailedTimingFacts,
    pub boundaries: Vec<SegmentBoundary>,
    pub policy_version: u32,
    pub provider_limit_ms: u64,
    pub total_source_duration_sec: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentChildRecord {
    pub segment_index: usize,
    pub client_job_id: String,
    pub segment_sha256: Option<String>,
    pub internal_job_id: Option<String>,
    pub input_segment_path: Option<PathBuf>,
    pub state: Option<CloudJobState>,
    pub output_artifact_path: Option<PathBuf>,
    pub duration_sec: f64,
    pub cost_usd: Option<f64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentedCloudJobManifest {
    pub schema_version: u32,
    pub state_revision: u64,
    pub parent_id: String,
    pub client_request_id: String,
    pub project_id: String,
    pub task_type: String,
    pub provider_id: String,
    pub model_id: String,
    pub configuration_hash: String,
    pub source_media_id: Option<String>,
    pub source_content_hash: String,
    pub source_file_name: Option<String>,
    pub final_audio_policy: FinalAudioPolicy,
    pub unit_rate_usd: f64,
    pub state: SegmentedJobState,
    pub source_facts: SourceMediaFacts,
    pub timing_facts: DetailedTimingFacts,
    pub segment_plan: SegmentPlan,
    pub child_jobs: Vec<SegmentChildRecord>,
    pub budget_limit: Option<f64>,
    pub provisional_estimate_usd: f64,
    pub actual_batch_base_estimate_usd: Option<f64>,
    pub final_output: Option<OutputArtifactRecord>,
    pub timestamps: JobTimestamps,
    pub cancellation_requested: bool,
    pub progress_pct: Option<f64>,
    pub error: Option<JobErrorRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentedChildSnapshot {
    pub segment_index: usize,
    pub client_job_id: String,
    pub state: Option<CloudJobState>,
    pub duration_sec: f64,
    pub cost_usd: Option<f64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentedCloudJobSnapshot {
    pub schema_version: u32,
    pub state_revision: u64,
    pub parent_id: String,
    pub client_request_id: String,
    pub project_id: String,
    pub task_type: String,
    pub provider_id: String,
    pub model_id: String,
    pub configuration_hash: String,
    pub state: SegmentedJobState,
    pub source_facts: SourceMediaFacts,
    pub timing_facts: DetailedTimingFacts,
    pub segment_plan: SegmentPlan,
    pub child_jobs: Vec<SegmentedChildSnapshot>,
    pub budget_limit: Option<f64>,
    pub provisional_estimate_usd: f64,
    pub actual_batch_base_estimate_usd: Option<f64>,
    pub final_output_ready: bool,
    pub final_audio_policy: FinalAudioPolicy,
    pub timestamps: JobTimestamps,
    pub cancellation_requested: bool,
    pub progress_pct: Option<f64>,
    pub error: Option<String>,
}

impl SegmentedCloudJobManifest {
    pub fn new(
        parent_id: String,
        client_request_id: String,
        project_id: String,
        task_type: String,
        provider_id: String,
        model_id: String,
        configuration_hash: String,
        source_media_id: Option<String>,
        source_content_hash: String,
        source_file_name: Option<String>,
        final_audio_policy: FinalAudioPolicy,
        unit_rate_usd: f64,
        source_facts: SourceMediaFacts,
        timing_facts: DetailedTimingFacts,
        segment_plan: SegmentPlan,
        budget_limit: Option<f64>,
        provisional_estimate_usd: f64,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        let child_jobs = segment_plan
            .boundaries
            .iter()
            .map(|b| SegmentChildRecord {
                segment_index: b.index,
                client_job_id: format!(
                    "segjob:{}:{}:{}:v{}",
                    parent_id, b.index, configuration_hash, segment_plan.policy_version
                ),
                segment_sha256: None,
                internal_job_id: None,
                input_segment_path: None,
                state: None,
                output_artifact_path: None,
                duration_sec: b.expected_duration_sec,
                cost_usd: None,
                updated_at: now.clone(),
            })
            .collect();

        Self {
            schema_version: 1,
            state_revision: 1,
            parent_id,
            client_request_id,
            project_id,
            task_type,
            provider_id,
            model_id,
            configuration_hash,
            source_media_id,
            source_content_hash,
            source_file_name,
            final_audio_policy,
            unit_rate_usd,
            state: SegmentedJobState::Planning,
            source_facts,
            timing_facts,
            segment_plan,
            child_jobs,
            budget_limit,
            provisional_estimate_usd,
            actual_batch_base_estimate_usd: None,
            final_output: None,
            timestamps: JobTimestamps {
                created_at: now.clone(),
                updated_at: now,
                submitted_at: None,
                completed_at: None,
            },
            cancellation_requested: false,
            progress_pct: None,
            error: None,
        }
    }

    pub fn transition_to(&mut self, new_state: SegmentedJobState) -> Result<(), String> {
        if !self.state.can_transition_to(new_state) {
            return Err(format!(
                "INVALID_STATE_TRANSITION: Cannot transition from {:?} to {:?}",
                self.state, new_state
            ));
        }

        self.state = new_state;
        self.state_revision += 1;
        self.timestamps.updated_at = chrono::Utc::now().to_rfc3339();

        if new_state == SegmentedJobState::Running && self.timestamps.submitted_at.is_none() {
            self.timestamps.submitted_at = Some(self.timestamps.updated_at.clone());
        } else if new_state == SegmentedJobState::Completed {
            self.timestamps.completed_at = Some(self.timestamps.updated_at.clone());
            self.progress_pct = Some(100.0);
        }

        Ok(())
    }

    pub fn recalculate_progress(&mut self) {
        match self.state {
            SegmentedJobState::Planning
            | SegmentedJobState::Splitting
            | SegmentedJobState::CostApprovalRequired
            | SegmentedJobState::Ready
            | SegmentedJobState::Stitching
            | SegmentedJobState::ValidatingOutput => {
                self.progress_pct = None;
            }
            SegmentedJobState::Running => {
                let total_duration: f64 = self.child_jobs.iter().map(|c| c.duration_sec).sum();
                if total_duration > 0.0 {
                    let completed_duration: f64 = self
                        .child_jobs
                        .iter()
                        .filter(|c| c.state == Some(CloudJobState::Completed))
                        .map(|c| c.duration_sec)
                        .sum();
                    self.progress_pct =
                        Some(((completed_duration / total_duration) * 100.0).clamp(0.0, 99.9));
                } else {
                    self.progress_pct = None;
                }
            }
            SegmentedJobState::Completed => {
                self.progress_pct = Some(100.0);
            }
            SegmentedJobState::Failed
            | SegmentedJobState::Blocked
            | SegmentedJobState::Cancelled => {}
        }
    }

    pub fn to_snapshot(&self) -> SegmentedCloudJobSnapshot {
        let child_snapshots = self
            .child_jobs
            .iter()
            .map(|c| SegmentedChildSnapshot {
                segment_index: c.segment_index,
                client_job_id: c.client_job_id.clone(),
                state: c.state,
                duration_sec: c.duration_sec,
                cost_usd: c.cost_usd,
                updated_at: c.updated_at.clone(),
            })
            .collect();

        SegmentedCloudJobSnapshot {
            schema_version: self.schema_version,
            state_revision: self.state_revision,
            parent_id: self.parent_id.clone(),
            client_request_id: self.client_request_id.clone(),
            project_id: self.project_id.clone(),
            task_type: self.task_type.clone(),
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            configuration_hash: self.configuration_hash.clone(),
            state: self.state,
            source_facts: self.source_facts.clone(),
            timing_facts: self.timing_facts.clone(),
            segment_plan: self.segment_plan.clone(),
            child_jobs: child_snapshots,
            budget_limit: self.budget_limit,
            provisional_estimate_usd: self.provisional_estimate_usd,
            actual_batch_base_estimate_usd: self.actual_batch_base_estimate_usd,
            final_output_ready: self.final_output.is_some(),
            final_audio_policy: self.final_audio_policy.clone(),
            timestamps: self.timestamps.clone(),
            cancellation_requested: self.cancellation_requested,
            progress_pct: self.progress_pct,
            error: self.error.as_ref().map(|e| e.sanitized_message.clone()),
        }
    }
}
