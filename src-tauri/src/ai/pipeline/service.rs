use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Instant;

use crate::ai::onnx::{get_global_ai_runtime, AiTensorOutput, InferenceRequest};
use crate::ai::pipeline::bbox::BoundingBox;
use crate::ai::pipeline::image::ImageFrame;
use crate::ai::pipeline::mask::Mask;
use crate::ai::pipeline::postprocess::{postprocess_outputs, PostprocessConfig};
use crate::ai::pipeline::preprocess::{preprocess_image, PreprocessConfig, TransformMetadata};
use crate::ai::pipeline::validate::validate_preprocess_against_model;
use crate::error::AppError;

/// Structured report of an end-to-end AI pipeline execution on a real image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineExecutionReport {
    pub model_id: String,
    pub source_path: String,
    pub source_width: u32,
    pub source_height: u32,
    pub processed_width: u32,
    pub processed_height: u32,
    pub input_tensor_shape: Vec<u64>,
    pub outputs: Vec<AiTensorOutput>,
    pub mask: Option<Mask>,
    pub bounding_boxes: Vec<BoundingBox>,
    pub transform: TransformMetadata,
    pub decode_duration_ms: f64,
    pub preprocess_duration_ms: f64,
    pub inference_duration_ms: f64,
    pub postprocess_duration_ms: f64,
    pub total_duration_ms: f64,
}

/// Service that coordinates decoding, preprocessing, ONNX inference, and postprocessing.
pub struct AiInferencePipeline;

impl AiInferencePipeline {
    /// Executes the full end-to-end AI pipeline for a given image and loaded ONNX model.
    pub fn run_pipeline<P: AsRef<Path>>(
        image_path: P,
        model_id: &str,
        preprocess_config: &PreprocessConfig,
        postprocess_config: Option<&PostprocessConfig>,
    ) -> Result<PipelineExecutionReport, AppError> {
        let total_start = Instant::now();
        let path = image_path.as_ref();

        // 1. Check runtime and model
        let runtime = get_global_ai_runtime();
        let r = runtime
            .lock()
            .map_err(|e| AppError::process_failed(format!("Failed to lock AI runtime: {}", e)))?;

        let active_model = r.loaded_model_id().ok_or_else(|| {
            AppError::model_not_available(
                model_id,
                "No AI model currently loaded in session. Please load the model first.",
            )
        })?;

        if active_model != model_id {
            return Err(AppError::invalid_input(format!(
                "Model mismatch: active session has model '{}', but requested '{}'",
                active_model, model_id
            )));
        }

        let model_metadata = r.inspect_active_model()?;
        let target_tensor_name = model_metadata
            .inputs
            .first()
            .map(|i| i.name.as_str())
            .unwrap_or("input");

        // 2. Validate Preprocess configuration against model
        let val_report = validate_preprocess_against_model(
            preprocess_config,
            &model_metadata,
            Some(target_tensor_name),
        );
        if !val_report.is_valid {
            return Err(AppError::invalid_input(format!(
                "Preprocessing configuration is incompatible with model '{}': {}",
                model_id,
                val_report.errors.join("; ")
            )));
        }

        // 3. Decode image from disk with monotonic timing
        let decode_start = Instant::now();
        let image_frame = ImageFrame::decode_from_file(path)?;
        let decode_duration_ms = decode_start.elapsed().as_secs_f64() * 1000.0;

        let source_width = image_frame.width;
        let source_height = image_frame.height;

        // 4. Preprocess image with monotonic timing
        let preprocess_start = Instant::now();
        let prep_res = preprocess_image(&image_frame, preprocess_config, target_tensor_name)?;
        let preprocess_duration_ms = preprocess_start.elapsed().as_secs_f64() * 1000.0;

        let input_tensor_shape = prep_res.tensor.shape.clone();

        // 5. Run real ONNX inference
        let inference_req = InferenceRequest {
            model_id: model_id.to_string(),
            inputs: vec![prep_res.tensor],
        };
        let infer_res = r.infer(&inference_req)?;
        let inference_duration_ms = infer_res.inference_duration_ms;

        // 6. Postprocess outputs with monotonic timing
        let default_post = PostprocessConfig::default();
        let post_cfg = postprocess_config.unwrap_or(&default_post);
        let post_res = postprocess_outputs(&infer_res.outputs, post_cfg)?;

        let total_duration_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        Ok(PipelineExecutionReport {
            model_id: model_id.to_string(),
            source_path: path.to_string_lossy().to_string(),
            source_width,
            source_height,
            processed_width: prep_res.processed_width,
            processed_height: prep_res.processed_height,
            input_tensor_shape,
            outputs: infer_res.outputs,
            mask: post_res.mask,
            bounding_boxes: post_res.bounding_boxes,
            transform: prep_res.transform,
            decode_duration_ms,
            preprocess_duration_ms,
            inference_duration_ms,
            postprocess_duration_ms: post_res.postprocess_duration_ms,
            total_duration_ms,
        })
    }
}
