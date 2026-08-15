use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub current_step: String,
    pub current_frame: u64,
    pub total_frames: u64,
    pub percentage: f32,
    pub estimated_seconds_remaining: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub project_id: String,
    pub state: JobState,
    pub progress: JobProgress,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_mock: bool,
}
