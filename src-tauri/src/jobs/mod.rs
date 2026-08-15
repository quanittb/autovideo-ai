use serde::{Deserialize, Serialize};
use crate::error::AppError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobState {
    Queued,
    Running,
    Paused,
    Cancelling,
    Cancelled,
    Failed,
    Completed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStage {
    ExtractingFrames,
    Analyzing,
    GeneratingMasks,
    Inpainting,
    TemporalSmoothing,
    StitchingAudio,
    EncodingVideo,
    Finalizing,
}

impl JobStage {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ExtractingFrames => "Extracting video frames",
            Self::Analyzing => "Analyzing scene & subjects",
            Self::GeneratingMasks => "Generating character masks",
            Self::Inpainting => "Inpainting transformed subject",
            Self::TemporalSmoothing => "Applying temporal consistency",
            Self::StitchingAudio => "Re-syncing original audio track",
            Self::EncodingVideo => "Encoding final video container",
            Self::Finalizing => "Finalizing output artifact",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub stage: JobStage,
    pub stage_index: u8,
    pub total_stages: u8,
    pub current_frame: u64,
    pub total_frames: u64,
    pub percentage: f32,
    pub estimated_seconds_remaining: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JobError {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

impl From<AppError> for JobError {
    fn from(err: AppError) -> Self {
        Self {
            code: format!("{:?}", err.code),
            message: err.message,
            details: err.details,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub project_id: String,
    pub state: JobState,
    pub stage: JobStage,
    pub progress: JobProgress,
    pub error: Option<JobError>,
    pub created_at: String,
    pub updated_at: String,
    pub is_fixture: bool,
}

impl Job {
    pub fn new(id: String, project_id: String, is_fixture: bool) -> Self {
        Self {
            id,
            project_id,
            state: JobState::Queued,
            stage: JobStage::ExtractingFrames,
            progress: JobProgress {
                stage: JobStage::ExtractingFrames,
                stage_index: 1,
                total_stages: 8,
                current_frame: 0,
                total_frames: 0,
                percentage: 0.0,
                estimated_seconds_remaining: 0,
            },
            error: None,
            created_at: "Just now".to_string(),
            updated_at: "Just now".to_string(),
            is_fixture,
        }
    }

    pub fn can_cancel(&self) -> bool {
        matches!(self.state, JobState::Queued | JobState::Running | JobState::Paused)
    }

    pub fn can_pause(&self) -> bool {
        self.state == JobState::Running
    }

    pub fn can_resume(&self) -> bool {
        self.state == JobState::Paused
    }

    pub fn can_retry(&self) -> bool {
        matches!(self.state, JobState::Failed | JobState::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_lifecycle_transitions() {
        let mut job = Job::new("job-1".to_string(), "proj-1".to_string(), false);
        assert_eq!(job.state, JobState::Queued);
        assert!(job.can_cancel());
        assert!(!job.can_pause());

        job.state = JobState::Running;
        assert!(job.can_pause());
        assert!(job.can_cancel());

        job.state = JobState::Paused;
        assert!(job.can_resume());
        assert!(job.can_cancel());

        job.state = JobState::Failed;
        assert!(job.can_retry());
        assert!(!job.can_cancel());
    }

    #[test]
    fn test_stage_display_names() {
        assert_eq!(JobStage::ExtractingFrames.display_name(), "Extracting video frames");
        assert_eq!(JobStage::Inpainting.display_name(), "Inpainting transformed subject");
        assert_eq!(JobStage::Finalizing.display_name(), "Finalizing output artifact");
    }
}
