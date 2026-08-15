use std::path::{Path, PathBuf};
use crate::error::AppError;
use crate::media::AnalysisResult;
use crate::projects::{TransformationPlan, TransformationRequest};

pub trait AnalysisEngine: Send + Sync {
    fn analyze(&self, video_path: &Path) -> Result<AnalysisResult, AppError>;
}

pub trait TransformationEngine: Send + Sync {
    fn plan(&self, request: &TransformationRequest, analysis: &AnalysisResult) -> Result<TransformationPlan, AppError>;
}

pub trait CharacterTransformationEngine: Send + Sync {
    fn transform_character(
        &self,
        frames_dir: &Path,
        mask_frames_dir: &Path,
        prompt: &str,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, AppError>;
}

pub trait BackgroundTransformationEngine: Send + Sync {
    fn transform_background(
        &self,
        frames_dir: &Path,
        depth_dir: &Path,
        prompt: &str,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, AppError>;
}

pub trait TemporalConsistencyEngine: Send + Sync {
    fn smooth_temporal_flow(
        &self,
        raw_frames_dir: &Path,
        inpainted_frames_dir: &Path,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, AppError>;
}

pub trait AudioEngine: Send + Sync {
    fn extract_audio(&self, video_path: &Path, output_audio_path: &Path) -> Result<(), AppError>;
    fn mux_audio(&self, video_frames_dir: &Path, audio_path: &Path, output_video: &Path) -> Result<(), AppError>;
}
