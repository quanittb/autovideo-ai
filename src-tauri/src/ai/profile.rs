use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ai::pipeline::layout::{ChannelOrder, TensorLayout};
use crate::ai::pipeline::normalize::NormalizationConfig;
use crate::ai::pipeline::postprocess::PostprocessConfig;
use crate::ai::pipeline::preprocess::PreprocessConfig;
use crate::ai::pipeline::resize::ResizeFilter;
use crate::ai::tensor::TensorDataType;

/// Aspect-ratio preservation mode for model input tensors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AspectHandling {
    Stretch,
    Letterbox {
        #[serde(default = "default_pad_val")]
        pad_value: [u8; 3],
    },
    CenterCrop,
}

fn default_pad_val() -> [u8; 3] {
    [114, 114, 114]
}

impl Default for AspectHandling {
    fn default() -> Self {
        Self::Stretch
    }
}

/// Output interpretation classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutputInterpretationType {
    Image,
    Mask,
    BBox,
}

/// Interpretation mode for segmentation masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MaskInterpretation {
    Binary,
    Grayscale,
    ProbabilityMap,
}

/// Bounding box coordinate decoding format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BboxInterpretation {
    YoloV8,
    PascalVoc,
    NormalizedCenter,
}

/// Production Input Profile specifying frame-to-tensor preprocessing requirements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputProfile {
    pub target_width: u32,
    pub target_height: u32,
    pub channel_order: ChannelOrder,
    pub color_space: String,
    pub layout: TensorLayout,
    pub normalization: NormalizationConfig,
    pub resize_filter: ResizeFilter,
    pub aspect_handling: AspectHandling,
    pub tensor_name: Option<String>,
    pub data_type: TensorDataType,
}

impl Default for InputProfile {
    fn default() -> Self {
        Self {
            target_width: 640,
            target_height: 640,
            channel_order: ChannelOrder::Rgb,
            color_space: "sRGB".to_string(),
            layout: TensorLayout::Nchw,
            normalization: NormalizationConfig::zero_to_one(),
            resize_filter: ResizeFilter::Bilinear,
            aspect_handling: AspectHandling::Stretch,
            tensor_name: None,
            data_type: TensorDataType::Float32,
        }
    }
}

/// Production Output Profile specifying tensor-to-artifact interpretation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputProfile {
    pub output_type: OutputInterpretationType,
    pub tensor_name: Option<String>,
    pub layout: Option<TensorLayout>,
    pub threshold: Option<f32>,
    pub mask_interpretation: Option<MaskInterpretation>,
    pub bbox_interpretation: Option<BboxInterpretation>,
    pub coordinate_restoration: bool,
}

impl Default for OutputProfile {
    fn default() -> Self {
        Self {
            output_type: OutputInterpretationType::Image,
            tensor_name: None,
            layout: Some(TensorLayout::Nchw),
            threshold: None,
            mask_interpretation: None,
            bbox_interpretation: None,
            coordinate_restoration: false,
        }
    }
}

/// Self-describing Model Profile connecting raw frames to model tensors and outputs to artifacts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelProfile {
    pub input: InputProfile,
    pub output: OutputProfile,
}

impl Default for AiModelProfile {
    fn default() -> Self {
        Self {
            input: InputProfile::default(),
            output: OutputProfile::default(),
        }
    }
}

impl AiModelProfile {
    pub fn new(input: InputProfile, output: OutputProfile) -> Self {
        Self { input, output }
    }

    /// Computes a deterministic SHA-256 hash of the complete profile configuration.
    pub fn compute_profile_hash(&self) -> String {
        let json_str = serde_json::to_string(self).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(json_str.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Converts this profile into a validated PreprocessConfig for execution.
    pub fn to_preprocess_config(&self) -> PreprocessConfig {
        let (letterbox, pad, center_crop) = match &self.input.aspect_handling {
            AspectHandling::Stretch => (false, [0, 0, 0], false),
            AspectHandling::Letterbox { pad_value } => (true, *pad_value, false),
            AspectHandling::CenterCrop => (false, [0, 0, 0], true),
        };

        PreprocessConfig {
            target_width: self.input.target_width,
            target_height: self.input.target_height,
            resize_filter: self.input.resize_filter,
            letterbox,
            letterbox_pad: pad,
            center_crop,
            crop_width: if center_crop {
                Some(self.input.target_width)
            } else {
                None
            },
            crop_height: if center_crop {
                Some(self.input.target_height)
            } else {
                None
            },
            channel_order: self.input.channel_order,
            normalization: self.input.normalization.clone(),
            layout: self.input.layout,
            batch_size: 1,
        }
    }

    /// Converts this profile into a validated PostprocessConfig for execution.
    pub fn to_postprocess_config(&self) -> PostprocessConfig {
        match self.output.output_type {
            OutputInterpretationType::Mask => PostprocessConfig {
                extract_mask: true,
                mask_threshold: self.output.threshold.or(Some(0.5)),
                extract_bboxes: false,
                bbox_confidence_threshold: None,
            },
            OutputInterpretationType::BBox => PostprocessConfig {
                extract_mask: false,
                mask_threshold: None,
                extract_bboxes: true,
                bbox_confidence_threshold: self.output.threshold.or(Some(0.25)),
            },
            OutputInterpretationType::Image => PostprocessConfig {
                extract_mask: false,
                mask_threshold: None,
                extract_bboxes: false,
                bbox_confidence_threshold: None,
            },
        }
    }
}
