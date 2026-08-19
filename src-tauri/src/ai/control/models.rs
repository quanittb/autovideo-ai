use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ai::manifest::{AiModelManifest, ModelFormat, ModelRequirements};
use crate::ai::package::{calculate_file_sha256, AiModelPackage};
use crate::ai::pipeline::layout::TensorLayout;
use crate::ai::profile::{
    AiModelProfile, AspectHandling, InputProfile, MaskInterpretation, OutputInterpretationType,
    OutputProfile,
};
use crate::ai::provider::ExecutionProvider;
use crate::ai::tensor::TensorDataType;
use crate::error::{AppError, ErrorCode};

/// Well-known Control Model IDs.
pub const MODEL_ID_DWPOSE: &str = "dwpose";
pub const MODEL_ID_DEPTH_ANYTHING_V2: &str = "depth_anything_v2";
pub const MODEL_ID_BIREFNET: &str = "birefnet";

/// Specification of an authoritative control model package required for video conditioning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ControlModelSpec {
    pub model_id: String,
    pub display_name: String,
    pub default_version: String,
    pub description: String,
    pub expected_file_name: String,
    pub official_url: String,
    pub license: String,
    pub required_vram_mb: u64,
    pub profile: AiModelProfile,
}

impl ControlModelSpec {
    /// Authoritative specification for DWPose whole-body pose detector.
    pub fn dwpose_spec() -> Self {
        Self {
            model_id: MODEL_ID_DWPOSE.to_string(),
            display_name: "DWPose Whole-Body Pose Detector".to_string(),
            default_version: "1.0.0".to_string(),
            description: "Extracts 18 body joints, 68 face landmarks, and 42 hand keypoints for motion tracking.".to_string(),
            expected_file_name: "dwpose.onnx".to_string(),
            official_url: "https://huggingface.co/yoland/DWPose/resolve/main/dw-ll_ucoco_384.onnx".to_string(),
            license: "Apache-2.0".to_string(),
            required_vram_mb: 512,
            profile: AiModelProfile {
                input: InputProfile {
                    target_width: 384,
                    target_height: 288,
                    channel_order: crate::ai::pipeline::layout::ChannelOrder::Rgb,
                    color_space: "sRGB".to_string(),
                    layout: TensorLayout::Nchw,
                    normalization: crate::ai::pipeline::normalize::NormalizationConfig::zero_to_one(),
                    resize_filter: crate::ai::pipeline::resize::ResizeFilter::Bilinear,
                    aspect_handling: AspectHandling::Letterbox { pad_value: [114, 114, 114] },
                    tensor_name: Some("input".to_string()),
                    data_type: TensorDataType::Float32,
                },
                output: OutputProfile {
                    output_type: OutputInterpretationType::Image,
                    tensor_name: Some("output".to_string()),
                    layout: Some(TensorLayout::Nchw),
                    threshold: Some(0.3),
                    mask_interpretation: None,
                    bbox_interpretation: None,
                    coordinate_restoration: false,
                },
            },
        }
    }

    /// Authoritative specification for Depth Anything V2 metric depth model.
    pub fn depth_anything_v2_spec() -> Self {
        Self {
            model_id: MODEL_ID_DEPTH_ANYTHING_V2.to_string(),
            display_name: "Depth Anything V2 Metric Depth".to_string(),
            default_version: "1.0.0".to_string(),
            description: "Estimates high-fidelity continuous 3D scene depth and camera perspective.".to_string(),
            expected_file_name: "depth_anything_v2_vits.onnx".to_string(),
            official_url: "https://huggingface.co/depth-anything/Depth-Anything-V2-Small/resolve/main/depth_anything_v2_vits.onnx".to_string(),
            license: "Apache-2.0".to_string(),
            required_vram_mb: 768,
            profile: AiModelProfile {
                input: InputProfile {
                    target_width: 518,
                    target_height: 518,
                    channel_order: crate::ai::pipeline::layout::ChannelOrder::Rgb,
                    color_space: "sRGB".to_string(),
                    layout: TensorLayout::Nchw,
                    normalization: crate::ai::pipeline::normalize::NormalizationConfig::imagenet(),
                    resize_filter: crate::ai::pipeline::resize::ResizeFilter::Bilinear,
                    aspect_handling: AspectHandling::Stretch,
                    tensor_name: Some("image".to_string()),
                    data_type: TensorDataType::Float32,
                },
                output: OutputProfile {
                    output_type: OutputInterpretationType::Image,
                    tensor_name: Some("depth".to_string()),
                    layout: Some(TensorLayout::Nchw),
                    threshold: None,
                    mask_interpretation: None,
                    bbox_interpretation: None,
                    coordinate_restoration: false,
                },
            },
        }
    }

    /// Authoritative specification for BiRefNet subject segmentation model.
    pub fn birefnet_spec() -> Self {
        Self {
            model_id: MODEL_ID_BIREFNET.to_string(),
            display_name: "BiRefNet Subject Segmentation".to_string(),
            default_version: "1.0.0".to_string(),
            description:
                "High-precision character and subject matting for foreground/background separation."
                    .to_string(),
            expected_file_name: "birefnet.onnx".to_string(),
            official_url: "https://huggingface.co/ZhengPeng7/BiRefNet/resolve/main/birefnet.onnx"
                .to_string(),
            license: "MIT".to_string(),
            required_vram_mb: 1024,
            profile: AiModelProfile {
                input: InputProfile {
                    target_width: 1024,
                    target_height: 1024,
                    channel_order: crate::ai::pipeline::layout::ChannelOrder::Rgb,
                    color_space: "sRGB".to_string(),
                    layout: TensorLayout::Nchw,
                    normalization: crate::ai::pipeline::normalize::NormalizationConfig::imagenet(),
                    resize_filter: crate::ai::pipeline::resize::ResizeFilter::Bilinear,
                    aspect_handling: AspectHandling::Stretch,
                    tensor_name: Some("input_image".to_string()),
                    data_type: TensorDataType::Float32,
                },
                output: OutputProfile {
                    output_type: OutputInterpretationType::Mask,
                    tensor_name: Some("output_mask".to_string()),
                    layout: Some(TensorLayout::Nchw),
                    threshold: Some(0.5),
                    mask_interpretation: Some(MaskInterpretation::ProbabilityMap),
                    bbox_interpretation: None,
                    coordinate_restoration: false,
                },
            },
        }
    }

    /// Returns list of all required control model specifications.
    pub fn all_required_specs() -> Vec<Self> {
        vec![
            Self::dwpose_spec(),
            Self::depth_anything_v2_spec(),
            Self::birefnet_spec(),
        ]
    }

    /// Creates an authoritative AiModelPackage from a locally imported model file.
    pub fn create_package_from_file(
        &self,
        model_file: PathBuf,
        version: Option<&str>,
        is_production: bool,
    ) -> Result<AiModelPackage, AppError> {
        if !model_file.exists() {
            return Err(AppError::file_not_found(model_file.display().to_string()));
        }

        let sha256 = calculate_file_sha256(&model_file)?;
        let file_size_bytes = std::fs::metadata(&model_file).map(|m| m.len()).unwrap_or(0);

        if file_size_bytes == 0 {
            return Err(AppError::new(
                ErrorCode::ModelIntegrityMismatch,
                format!("Imported model file is empty: {}", model_file.display()),
            ));
        }

        let ver = version.unwrap_or(&self.default_version);

        let manifest = AiModelManifest::new(
            format!("{}:{}", self.model_id, ver),
            &self.display_name,
            ver,
            ModelFormat::Onnx,
            model_file.clone(),
            &self.description,
            vec![],
            vec![],
            ModelRequirements {
                min_memory_mb: Some(self.required_vram_mb),
                preferred_provider: Some(ExecutionProvider::DirectML),
                requires_gpu: false,
            },
        )
        .with_production(is_production);

        AiModelPackage::new(
            &self.model_id,
            &self.display_name,
            ver,
            format!("{} v{}", self.display_name, ver),
            &self.description,
            ModelFormat::Onnx,
            model_file,
            file_size_bytes,
            sha256,
            manifest,
            self.profile.clone(),
            ModelRequirements {
                min_memory_mb: Some(self.required_vram_mb),
                preferred_provider: Some(ExecutionProvider::DirectML),
                requires_gpu: false,
            },
            vec![
                ExecutionProvider::Cpu,
                ExecutionProvider::DirectML,
                ExecutionProvider::Cuda,
            ],
        )
        .map(|pkg| pkg.with_production(is_production))
    }
}
