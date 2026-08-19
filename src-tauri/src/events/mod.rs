use crate::jobs::{Artifact, Job, PipelineStage, StageStatus};
use serde::{Deserialize, Serialize};

pub const STAGE_WEIGHTS: [f32; 6] = [5.0, 5.0, 30.0, 15.0, 35.0, 10.0];
pub const AI_STAGE_WEIGHTS: [f32; 7] = [5.0, 5.0, 20.0, 10.0, 40.0, 15.0, 5.0];

/// Calculates deterministic overall progress from completed stage weights and current stage progress.
pub fn calculate_overall_progress(stage_index: usize, stage_progress_percent: f32) -> f32 {
    let mut completed_weight = 0.0;
    for (i, &w) in STAGE_WEIGHTS.iter().enumerate() {
        if i < stage_index {
            completed_weight += w;
        }
    }
    let current_weight = STAGE_WEIGHTS.get(stage_index).copied().unwrap_or(0.0);
    let clamped_stage_prog = stage_progress_percent.clamp(0.0, 100.0);
    let overall = completed_weight + (current_weight * (clamped_stage_prog / 100.0));
    (overall * 100.0).round() / 100.0 // Round to 2 decimal places for clean precision
}

/// Calculates deterministic overall progress supporting both 6-stage and 7-stage pipelines.
pub fn calculate_overall_progress_with_stages(
    stages: &[PipelineStage],
    stage_index: usize,
    stage_progress_percent: f32,
) -> f32 {
    let weights: &[f32] = if stages.len() == 7 {
        &AI_STAGE_WEIGHTS
    } else {
        &STAGE_WEIGHTS
    };

    let mut completed_weight = 0.0;
    for (i, &w) in weights.iter().enumerate() {
        if i < stage_index {
            completed_weight += w;
        }
    }
    let current_weight = weights.get(stage_index).copied().unwrap_or(0.0);
    let clamped_stage_prog = stage_progress_percent.clamp(0.0, 100.0);
    let overall = completed_weight + (current_weight * (clamped_stage_prog / 100.0));
    (overall * 100.0).round() / 100.0
}

/// Calculates overall progress by summing weighted progress of all stages.
pub fn calculate_job_progress_from_stages(stages: &[PipelineStage]) -> f32 {
    let weights: &[f32] = if stages.len() == 7 {
        &AI_STAGE_WEIGHTS
    } else {
        &STAGE_WEIGHTS
    };

    let mut total = 0.0;
    for (i, stage) in stages.iter().enumerate() {
        let weight = weights.get(i).copied().unwrap_or(0.0);
        total += weight * (stage.progress.clamp(0.0, 100.0) / 100.0);
    }
    (total * 100.0).round() / 100.0
}

/// Machine-readable parser for FFmpeg `-progress pipe:1` key-value lines.
pub fn parse_ffmpeg_progress_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    if let Some((k, v)) = trimmed.split_once('=') {
        Some((k.trim(), v.trim()))
    } else {
        None
    }
}

// -----------------------------------------------------------------------------
// Authoritative Event Names
// -----------------------------------------------------------------------------

pub struct EventNames;

impl EventNames {
    pub const JOB_CREATED: &'static str = "job:created";
    pub const JOB_QUEUED: &'static str = "job:queued";
    pub const JOB_STARTED: &'static str = "job:started";
    pub const JOB_STAGE_STARTED: &'static str = "job:stage_started";
    pub const JOB_STAGE_PROGRESS: &'static str = "job:stage_progress";
    pub const JOB_STAGE_COMPLETED: &'static str = "job:stage_completed";
    pub const JOB_PROGRESS: &'static str = "job:progress";
    pub const JOB_LOG: &'static str = "job:log";
    pub const JOB_ARTIFACT: &'static str = "job:artifact";
    pub const JOB_COMPLETED: &'static str = "job:completed";
    pub const JOB_FAILED: &'static str = "job:failed";
    pub const JOB_CANCEL_REQUESTED: &'static str = "job:cancel_requested";
    pub const JOB_CANCELLED: &'static str = "job:cancelled";
    pub const JOB_STAGE_CANCELLED: &'static str = "job:stage_cancelled";
    pub const JOB_RETRYING: &'static str = "job:retrying";
    pub const JOB_INTERRUPTED: &'static str = "job:interrupted";
    pub const AI_FRAME_PROGRESS: &'static str = "ai:frame_progress";
    pub const AI_RECONSTRUCTION_PROGRESS: &'static str = "ai:reconstruction_progress";
    pub const AI_MODEL_ACTIVATED: &'static str = "ai:model_activated";
    pub const AI_MODEL_ROLLBACK_COMPLETED: &'static str = "ai:model_rollback_completed";
    pub const AI_MODEL_IMPORTED: &'static str = "ai:model_imported";
    pub const AI_MODEL_VALIDATED: &'static str = "ai:model_validated";
    pub const AI_PREFLIGHT_STARTED: &'static str = "ai:preflight_started";
    pub const AI_PREFLIGHT_COMPLETED: &'static str = "ai:preflight_completed";
    pub const AI_PREFLIGHT_FAILED: &'static str = "ai:preflight_failed";
    pub const AI_MODEL_RESOLVED: &'static str = "ai:model_resolved";
    pub const SYSTEM_NOTIFICATION: &'static str = "system:notification";
}

// -----------------------------------------------------------------------------
// Strongly-Typed Event Payloads
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobCreatedEvent {
    pub job_id: String,
    pub project_id: String,
    pub job_type: String,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobQueuedEvent {
    pub job_id: String,
    pub project_id: String,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobStartedEvent {
    pub job_id: String,
    pub project_id: String,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobStageStartedEvent {
    pub job_id: String,
    pub project_id: String,
    pub stage_id: String,
    pub stage_index: usize,
    pub stage_name: String,
    pub stage_status: StageStatus,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobStageProgressEvent {
    pub job_id: String,
    pub project_id: String,
    pub stage_id: String,
    pub stage_index: usize,
    pub stage_progress: f32,
    pub overall_progress: f32,
    pub message: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobStageCompletedEvent {
    pub job_id: String,
    pub project_id: String,
    pub stage_id: String,
    pub stage_index: usize,
    pub stage_name: String,
    pub stage_status: StageStatus,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobProgressEvent {
    pub job_id: String,
    pub project_id: String,
    pub overall_progress: f32,
    pub stage_progress: f32,
    pub current_stage: Option<String>,
    pub current_stage_index: usize,
    pub completed_stages: usize,
    pub total_stages: usize,
    pub message: String,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobLogEvent {
    pub job_id: String,
    pub project_id: String,
    pub timestamp: String,
    pub level: String,
    pub stage_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobArtifactEvent {
    pub job_id: String,
    pub project_id: String,
    pub artifact_id: String,
    pub artifact_type: String,
    pub path: String,
    pub file_size_bytes: u64,
    pub stage_id: Option<String>,
    pub status: String,
    pub timestamp: String,
    pub artifact: Artifact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobCompletedEvent {
    pub job_id: String,
    pub project_id: String,
    pub duration_seconds: f32,
    pub output_files: Vec<String>,
    pub message: String,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobFailedEvent {
    pub job_id: String,
    pub project_id: String,
    pub stage_id: Option<String>,
    pub error_code: String,
    pub message: String,
    pub recoverable: bool,
    pub details: Option<String>,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobCancelRequestedEvent {
    pub job_id: String,
    pub project_id: String,
    pub message: String,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobStageCancelledEvent {
    pub job_id: String,
    pub project_id: String,
    pub stage_id: String,
    pub stage_index: usize,
    pub stage_name: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobCancelledEvent {
    pub job_id: String,
    pub project_id: String,
    pub message: String,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobRetryingEvent {
    pub job_id: String,
    pub project_id: String,
    pub retry_count: u32,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobInterruptedEvent {
    pub job_id: String,
    pub project_id: String,
    pub message: String,
    pub timestamp: String,
    pub job: Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiFrameProgressEvent {
    pub job_id: String,
    pub project_id: String,
    pub frame_index: usize,
    pub processed_frames: usize,
    pub total_frames: usize,
    pub stage_progress: f32,
    pub overall_progress: f32,
    pub inference_duration_ms: Option<f64>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiReconstructionProgressEvent {
    pub job_id: String,
    pub project_id: String,
    pub frames_encoded: usize,
    pub total_frames: usize,
    pub progress_percent: f32,
    pub overall_progress: f32,
    pub message: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemNotificationPayload {
    pub level: String, // "info", "warning", "error", "success"
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiModelActivatedEvent {
    pub model_id: String,
    pub version: String,
    pub previous_version: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiModelRollbackEvent {
    pub model_id: String,
    pub restored_version: String,
    pub previous_version: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiModelImportedEvent {
    pub model_id: String,
    pub version: String,
    pub sha256: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiPreflightEvent {
    pub source_path: String,
    pub model_id: String,
    pub is_valid: bool,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiModelResolvedEvent {
    pub model_id: String,
    pub version: String,
    pub model_hash: String,
    pub provider: String,
    pub timestamp: String,
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_weights_sum_to_100() {
        let sum: f32 = STAGE_WEIGHTS.iter().sum();
        assert!(
            (sum - 100.0).abs() < f32::EPSILON,
            "Stage weights must sum to exactly 100%"
        );

        let ai_sum: f32 = AI_STAGE_WEIGHTS.iter().sum();
        assert!(
            (ai_sum - 100.0).abs() < f32::EPSILON,
            "AI Stage weights must sum to exactly 100%"
        );
    }

    #[test]
    fn test_progress_calculation_stages() {
        // Stage 0 (weight 5%) at 0%
        assert_eq!(calculate_overall_progress(0, 0.0), 0.0);
        // Stage 0 at 50%
        assert_eq!(calculate_overall_progress(0, 50.0), 2.5);
        // Stage 0 at 100%
        assert_eq!(calculate_overall_progress(0, 100.0), 5.0);

        // Stage 1 (weight 5%) at 0% (Stage 0 completed = 5%)
        assert_eq!(calculate_overall_progress(1, 0.0), 5.0);
        // Stage 1 at 100% (Stage 0 + 1 completed = 10%)
        assert_eq!(calculate_overall_progress(1, 100.0), 10.0);

        // Stage 2 (weight 30%) at 50% (5 + 5 + 15 = 25%)
        assert_eq!(calculate_overall_progress(2, 50.0), 25.0);
        // Stage 2 at 100% (5 + 5 + 30 = 40%)
        assert_eq!(calculate_overall_progress(2, 100.0), 40.0);

        // Stage 3 (weight 15%) at 100% (40 + 15 = 55%)
        assert_eq!(calculate_overall_progress(3, 100.0), 55.0);

        // Stage 4 (weight 35%) at 50% (55 + 17.5 = 72.5%)
        assert_eq!(calculate_overall_progress(4, 50.0), 72.5);
        // Stage 4 at 100% (55 + 35 = 90%)
        assert_eq!(calculate_overall_progress(4, 100.0), 90.0);

        // Stage 5 (weight 10%) at 100% (90 + 10 = 100%)
        assert_eq!(calculate_overall_progress(5, 100.0), 100.0);
    }

    #[test]
    fn test_progress_clamping() {
        assert_eq!(calculate_overall_progress(0, -10.0), 0.0);
        assert_eq!(calculate_overall_progress(5, 150.0), 100.0);
    }

    #[test]
    fn test_parse_ffmpeg_progress_line() {
        assert_eq!(
            parse_ffmpeg_progress_line("frame=142"),
            Some(("frame", "142"))
        );
        assert_eq!(
            parse_ffmpeg_progress_line("fps=29.97 "),
            Some(("fps", "29.97"))
        );
        assert_eq!(
            parse_ffmpeg_progress_line("out_time_us=4567890"),
            Some(("out_time_us", "4567890"))
        );
        assert_eq!(
            parse_ffmpeg_progress_line("progress=continue"),
            Some(("progress", "continue"))
        );
        assert_eq!(
            parse_ffmpeg_progress_line("invalid line without equal"),
            None
        );
    }

    #[test]
    fn test_event_payload_serialization() {
        let job = Job::new(
            "job-1".to_string(),
            "proj-1".to_string(),
            "video_pipeline".to_string(),
            vec![],
        );
        let event = JobCreatedEvent {
            job_id: "job-1".to_string(),
            project_id: "proj-1".to_string(),
            job_type: "video_pipeline".to_string(),
            timestamp: "2026-08-15T12:00:00Z".to_string(),
            job,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"jobId\":\"job-1\""));
        assert!(json.contains("\"projectId\":\"proj-1\""));
        assert!(json.contains("\"jobType\":\"video_pipeline\""));
    }
}
