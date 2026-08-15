use crate::domain::{AiAvailabilityStatus, Job, JobProgress, JobState, MediaInfo, ModelInfo, Project, TransformationConfig};
use tauri::command;

#[command]
pub fn get_ai_status() -> AiAvailabilityStatus {
    // Following strict NEVER FAKE AI rule: Report that local AI model weights are not installed yet
    AiAvailabilityStatus::ModelNotAvailable {
        model_name: "AutoVideo Diffusion v1.0".to_string(),
        guidance: "Local model weights are not installed. Download weights or enable Cloud API in settings.".to_string(),
    }
}

#[command]
pub fn list_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "character-replace-v1".to_string(),
            name: "Character Swap AI (Fox/Rabbit)".to_string(),
            category: "Character".to_string(),
            is_downloaded: false,
            is_loaded_in_vram: false,
            size_bytes: 4_200_000_000,
        },
        ModelInfo {
            id: "scene-diffuse-v1".to_string(),
            name: "Scene Transformation AI".to_string(),
            category: "Scene".to_string(),
            is_downloaded: false,
            is_loaded_in_vram: false,
            size_bytes: 6_800_000_000,
        },
    ]
}

#[command]
pub fn list_projects() -> Vec<Project> {
    vec![
        Project {
            id: "proj-1".to_string(),
            name: "Winter to Autumn".to_string(),
            created_at: "2 hours ago".to_string(),
            updated_at: "2 hours ago".to_string(),
            source_video_path: Some("fixtures/videos/input_video.mp4".to_string()),
            media_info: Some(MediaInfo {
                file_name: "input_video.mp4".to_string(),
                file_path: "fixtures/videos/input_video.mp4".to_string(),
                duration_seconds: 62.0,
                duration_formatted: "01:02".to_string(),
                resolution: "1920x1080".to_string(),
                width: 1920,
                height: 1080,
                file_size_bytes: 47_395_840,
                file_size_formatted: "45.2 MB".to_string(),
                fps: 30.0,
                codec: "h264".to_string(),
            }),
            transformation: TransformationConfig {
                category: "scene".to_string(),
                original_character: None,
                replacement_character: None,
                prompt: "Transform snowy winter environment into vibrant autumn forest with falling golden leaves".to_string(),
                resolution: "1080p (1920x1080)".to_string(),
                quality: "High Quality".to_string(),
                format: "MP4".to_string(),
                fps: 30,
                remove_watermark: true,
            },
            is_mock_demo: true,
        },
        Project {
            id: "proj-2".to_string(),
            name: "Fox to Rabbit".to_string(),
            created_at: "1 day ago".to_string(),
            updated_at: "1 day ago".to_string(),
            source_video_path: Some("fixtures/videos/fox_sample.mp4".to_string()),
            media_info: Some(MediaInfo {
                file_name: "fox_sample.mp4".to_string(),
                file_path: "fixtures/videos/fox_sample.mp4".to_string(),
                duration_seconds: 62.0,
                duration_formatted: "01:02".to_string(),
                resolution: "1920x1080".to_string(),
                width: 1920,
                height: 1080,
                file_size_bytes: 47_395_840,
                file_size_formatted: "45.2 MB".to_string(),
                fps: 30.0,
                codec: "h264".to_string(),
            }),
            transformation: TransformationConfig {
                category: "character".to_string(),
                original_character: Some("Fox".to_string()),
                replacement_character: Some("Rabbit".to_string()),
                prompt: "A cute white rabbit wearing a scarf in an autumn forest".to_string(),
                resolution: "1080p (1920x1080)".to_string(),
                quality: "High Quality".to_string(),
                format: "MP4".to_string(),
                fps: 30,
                remove_watermark: true,
            },
            is_mock_demo: true,
        },
        Project {
            id: "proj-3".to_string(),
            name: "Beach Vacation".to_string(),
            created_at: "2 days ago".to_string(),
            updated_at: "2 days ago".to_string(),
            source_video_path: None,
            media_info: None,
            transformation: TransformationConfig::default(),
            is_mock_demo: true,
        },
        Project {
            id: "proj-4".to_string(),
            name: "Home to Market".to_string(),
            created_at: "3 days ago".to_string(),
            updated_at: "3 days ago".to_string(),
            source_video_path: None,
            media_info: None,
            transformation: TransformationConfig::default(),
            is_mock_demo: true,
        },
    ]
}

#[command]
pub fn create_project(name: String) -> Project {
    Project {
        id: format!("proj-{}", uuid::Uuid::new_v4().simple()),
        name,
        created_at: "Just now".to_string(),
        updated_at: "Just now".to_string(),
        source_video_path: None,
        media_info: None,
        transformation: TransformationConfig::default(),
        is_mock_demo: false,
    }
}

#[command]
pub fn get_sample_job() -> Job {
    Job {
        id: "job-demo-1".to_string(),
        project_id: "proj-2".to_string(),
        state: JobState::Completed,
        progress: JobProgress {
            current_step: "Rendering final video".to_string(),
            current_frame: 1860,
            total_frames: 1860,
            percentage: 100.0,
            estimated_seconds_remaining: 0,
        },
        error_message: None,
        created_at: "1 day ago".to_string(),
        updated_at: "1 day ago".to_string(),
        is_mock: true,
    }
}
