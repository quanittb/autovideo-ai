use serde::{Deserialize, Serialize};

use crate::ai::onnx::OnnxModelMetadata;
use crate::ai::pipeline::layout::TensorLayout;
use crate::ai::pipeline::preprocess::PreprocessConfig;
use crate::ai::tensor::{Dimension, TensorDataType};

/// Structured report validating a PreprocessConfig against native ONNX Model metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreprocessValidationResult {
    pub is_valid: bool,
    pub tensor_name: String,
    pub expected_shape: Vec<String>,
    pub produced_shape: Vec<u64>,
    pub expected_dtype: TensorDataType,
    pub produced_dtype: TensorDataType,
    pub errors: Vec<String>,
}

/// Validates that the output of a PreprocessConfig matches the input tensor expectations of an ONNX model.
pub fn validate_preprocess_against_model(
    config: &PreprocessConfig,
    model_metadata: &OnnxModelMetadata,
    target_tensor_name: Option<&str>,
) -> PreprocessValidationResult {
    let mut errors = Vec::new();

    if model_metadata.inputs.is_empty() {
        return PreprocessValidationResult {
            is_valid: false,
            tensor_name: target_tensor_name.unwrap_or("unknown").to_string(),
            expected_shape: vec![],
            produced_shape: vec![],
            expected_dtype: TensorDataType::Float32,
            produced_dtype: TensorDataType::Float32,
            errors: vec!["Model has no declared input tensors".to_string()],
        };
    }

    let input_meta = match target_tensor_name {
        Some(name) => model_metadata.inputs.iter().find(|i| i.name == name),
        None => model_metadata.inputs.first(),
    };

    let input_meta = match input_meta {
        Some(m) => m,
        None => {
            return PreprocessValidationResult {
                is_valid: false,
                tensor_name: target_tensor_name.unwrap_or("unknown").to_string(),
                expected_shape: vec![],
                produced_shape: vec![],
                expected_dtype: TensorDataType::Float32,
                produced_dtype: TensorDataType::Float32,
                errors: vec![format!(
                    "Target tensor '{}' not found in model inputs",
                    target_tensor_name.unwrap_or("unknown")
                )],
            };
        }
    };

    let expected_dtype = input_meta.data_type;
    let produced_dtype = TensorDataType::Float32;

    if expected_dtype != produced_dtype {
        errors.push(format!(
            "Data type mismatch: model expects {:?}, preprocessing produces {:?}",
            expected_dtype, produced_dtype
        ));
    }

    let produced_shape = match config.layout {
        TensorLayout::Nchw => vec![
            config.batch_size as u64,
            config.channel_order.channels() as u64,
            config.target_height as u64,
            config.target_width as u64,
        ],
        TensorLayout::Nhwc => vec![
            config.batch_size as u64,
            config.target_height as u64,
            config.target_width as u64,
            config.channel_order.channels() as u64,
        ],
    };

    let expected_shape_str: Vec<String> = input_meta
        .shape
        .iter()
        .map(|d| match d {
            Dimension::Fixed(v) => v.to_string(),
            Dimension::Dynamic(s) => s.clone(),
        })
        .collect();

    if input_meta.shape.len() != produced_shape.len() {
        errors.push(format!(
            "Rank mismatch: model expects {}D tensor, preprocessing produces {}D tensor",
            input_meta.shape.len(),
            produced_shape.len()
        ));
    } else {
        for (i, (exp_dim, &prod_dim)) in input_meta
            .shape
            .iter()
            .zip(produced_shape.iter())
            .enumerate()
        {
            if let Dimension::Fixed(v) = exp_dim {
                if *v != prod_dim {
                    errors.push(format!(
                        "Dimension mismatch at index {}: model expects {}, preprocessing produces {}",
                        i, v, prod_dim
                    ));
                }
            }
        }
    }

    let is_valid = errors.is_empty();

    PreprocessValidationResult {
        is_valid,
        tensor_name: input_meta.name.clone(),
        expected_shape: expected_shape_str,
        produced_shape,
        expected_dtype,
        produced_dtype,
        errors,
    }
}
