use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::ai::onnx::AiTensorOutput;
use crate::ai::pipeline::bbox::BoundingBox;
use crate::ai::pipeline::mask::{extract_mask_from_tensor, Mask};
use crate::error::AppError;

/// Configuration for generic neural network output postprocessing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostprocessConfig {
    pub extract_mask: bool,
    pub mask_threshold: Option<f32>,
    pub extract_bboxes: bool,
    pub bbox_confidence_threshold: Option<f32>,
}

impl Default for PostprocessConfig {
    fn default() -> Self {
        Self {
            extract_mask: false,
            mask_threshold: None,
            extract_bboxes: false,
            bbox_confidence_threshold: Some(0.25),
        }
    }
}

/// Structured result of postprocessing an ONNX output tensor array.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostprocessResult {
    pub raw_outputs: Vec<AiTensorOutput>,
    pub mask: Option<Mask>,
    pub bounding_boxes: Vec<BoundingBox>,
    pub postprocess_duration_ms: f64,
}

/// Executes postprocessing on ONNX output tensors.
pub fn postprocess_outputs(
    outputs: &[AiTensorOutput],
    config: &PostprocessConfig,
) -> Result<PostprocessResult, AppError> {
    let start = Instant::now();

    let mut mask = None;
    let bounding_boxes = Vec::new();

    if config.extract_mask {
        if let Some(first_output) = outputs.first() {
            let raw_mask = extract_mask_from_tensor(first_output)?;
            let final_mask = if let Some(thresh) = config.mask_threshold {
                raw_mask.apply_threshold(thresh)
            } else {
                raw_mask
            };
            mask = Some(final_mask);
        }
    }

    let postprocess_duration_ms = start.elapsed().as_secs_f64() * 1000.0;

    Ok(PostprocessResult {
        raw_outputs: outputs.to_vec(),
        mask,
        bounding_boxes,
        postprocess_duration_ms,
    })
}
