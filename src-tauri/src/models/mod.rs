use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelDescriptor {
    pub id: String,
    pub name: String,
    pub task: String, // "character-segmentation", "subject-diffusion", "temporal-smoothing"
    pub file_size_bytes: u64,
    pub is_downloaded: bool,
    pub is_loaded_in_vram: bool,
    pub local_path: Option<PathBuf>,
    pub sha256_checksum: String,
}

pub trait ModelProvider: Send + Sync {
    fn resolve_model_file(&self, model_id: &str) -> Result<PathBuf, AppError>;
}

pub struct ModelManager {
    models_dir: PathBuf,
}

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self { models_dir }
    }

    pub fn list_available_descriptors(&self) -> Vec<ModelDescriptor> {
        vec![
            ModelDescriptor {
                id: "model-char-swap-v1".to_string(),
                name: "Character Inpainting Diffusion v1.0".to_string(),
                task: "subject-diffusion".to_string(),
                file_size_bytes: 4_294_967_296, // ~4 GB
                is_downloaded: false,
                is_loaded_in_vram: false,
                local_path: None,
                sha256_checksum: "a1b2c3d4e5f6...".to_string(),
            },
            ModelDescriptor {
                id: "model-sam-video-v1".to_string(),
                name: "Segment Anything Video v1.0".to_string(),
                task: "character-segmentation".to_string(),
                file_size_bytes: 2_147_483_648, // ~2 GB
                is_downloaded: false,
                is_loaded_in_vram: false,
                local_path: None,
                sha256_checksum: "b2c3d4e5f6g7...".to_string(),
            },
        ]
    }

    pub fn check_model_ready(&self, model_id: &str) -> Result<PathBuf, AppError> {
        let path = self.models_dir.join(format!("{}.onnx", model_id));
        if path.exists() {
            Ok(path)
        } else {
            Err(AppError::model_not_available(
                model_id,
                "Model weights are not installed. Download weights in Settings -> Model Manager.",
            ))
        }
    }
}
