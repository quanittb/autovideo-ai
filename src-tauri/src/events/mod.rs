use serde::{Deserialize, Serialize};
use crate::jobs::{JobProgress, JobState};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobUpdatedPayload {
    pub job_id: String,
    pub project_id: String,
    pub state: JobState,
    pub progress: JobProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaProgressPayload {
    pub file_path: String,
    pub extracted_frames: u64,
    pub total_frames: u64,
    pub percentage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProgressPayload {
    pub project_id: String,
    pub analyzed_keyframes: u64,
    pub detected_characters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransformationProgressPayload {
    pub project_id: String,
    pub current_step: String,
    pub frame_index: u64,
    pub total_frames: u64,
    pub percentage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemNotificationPayload {
    pub level: String, // "info", "warning", "error", "success"
    pub title: String,
    pub message: String,
}

pub struct EventNames;

impl EventNames {
    pub const JOB_UPDATED: &'static str = "job:updated";
    pub const MEDIA_PROGRESS: &'static str = "media:progress";
    pub const ANALYSIS_PROGRESS: &'static str = "analysis:progress";
    pub const TRANSFORMATION_PROGRESS: &'static str = "transformation:progress";
    pub const SYSTEM_NOTIFICATION: &'static str = "system:notification";
}
