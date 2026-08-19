use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::ai::frame_pipeline::artifact::{
    compute_ai_config_hash, AiArtifactManager, AiFrameMetadata, AiFrameStatus, AiJobMetrics,
};
use crate::ai::frame_pipeline::config::{select_frames, AiFrameOutputMode, AiJobConfig};
use crate::ai::frame_pipeline::quality::FrameQualityValidator;
use crate::ai::onnx::{get_global_ai_runtime, InferenceRequest};
use crate::ai::pipeline::image::{ImageFrame, PixelFormat};
use crate::ai::pipeline::postprocess::{postprocess_outputs, PostprocessConfig};
use crate::ai::pipeline::preprocess::preprocess_image;
use crate::ai::pipeline::validate::validate_preprocess_against_model;
use crate::ai::registry::ModelRegistry;
use crate::ai::resource::AiResourceLimits;
use crate::ai::runtime::AiRuntime;
use crate::error::AppError;

pub struct AiFrameExecutor;

impl AiFrameExecutor {
    /// Executes bounded, memory-safe sequential AI frame inference over extracted video frames.
    pub fn execute<F>(
        source_frames_dir: &Path,
        ai_config: &AiJobConfig,
        artifact_manager: &AiArtifactManager,
        cancel_token: Option<Arc<AtomicBool>>,
        on_progress: F,
    ) -> Result<AiJobMetrics, AppError>
    where
        F: FnMut(f32, Option<&AiFrameMetadata>, &AiJobMetrics),
    {
        Self::execute_with_limits(
            source_frames_dir,
            ai_config,
            artifact_manager,
            &AiResourceLimits::default_production(),
            cancel_token,
            on_progress,
        )
    }

    /// Executes bounded AI frame inference with explicit resource limits.
    pub fn execute_with_limits<F>(
        source_frames_dir: &Path,
        ai_config: &AiJobConfig,
        artifact_manager: &AiArtifactManager,
        limits: &AiResourceLimits,
        cancel_token: Option<Arc<AtomicBool>>,
        mut on_progress: F,
    ) -> Result<AiJobMetrics, AppError>
    where
        F: FnMut(f32, Option<&AiFrameMetadata>, &AiJobMetrics),
    {
        let pipeline_start = Instant::now();

        // 1. Ensure artifact directories exist
        artifact_manager.ensure_dirs()?;

        // 2. Discover and sort source frame paths
        if !source_frames_dir.exists() || !source_frames_dir.is_dir() {
            return Err(AppError::storage_error(
                "Source frames directory not found",
                source_frames_dir.display().to_string(),
            ));
        }

        let mut frame_paths: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(source_frames_dir)
            .map_err(|e| AppError::storage_error("Failed to read frames dir", e.to_string()))?
            .flatten()
        {
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("jpg") {
                        frame_paths.push(p);
                    }
                }
            }
        }

        if frame_paths.is_empty() {
            return Err(AppError::invalid_input(format!(
                "No valid PNG/JPG frames found in {}",
                source_frames_dir.display()
            )));
        }

        frame_paths.sort_by_key(|p| p.file_name().unwrap_or_default().to_os_string());
        let total_frames = frame_paths.len();

        // 3. Compute frame selection
        let selected_indices = select_frames(total_frames, &ai_config.frame_sampling)?;
        let selected_set: std::collections::HashSet<usize> =
            selected_indices.iter().copied().collect();

        // 4. Compute configuration hash
        let config_hash = compute_ai_config_hash(
            &ai_config.model_id,
            &ai_config.preprocessing,
            ai_config.postprocessing.as_ref(),
        );

        // 5. Preprocessing resource limits check
        limits.validate_frame_dimensions(
            ai_config.preprocessing.target_width,
            ai_config.preprocessing.target_height,
        )?;

        // 6. Ensure model is loaded once in ONNX session
        let runtime = get_global_ai_runtime();
        let mut r = runtime
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if r.loaded_model_id().as_deref() != Some(&ai_config.model_id) {
            let storage_paths = crate::StoragePaths::default_paths();
            let registry = ModelRegistry::new(storage_paths.models_dir);
            let model_manifest = registry.get_model(&ai_config.model_id)?;

            r.load_model(&model_manifest)?;
        }

        let model_metadata = r.inspect_active_model()?;
        let target_tensor_name = model_metadata
            .inputs
            .first()
            .map(|i| i.name.as_str())
            .unwrap_or("input");

        // Validate Preprocessing configuration against model
        let val_report = validate_preprocess_against_model(
            &ai_config.preprocessing,
            &model_metadata,
            Some(target_tensor_name),
        );
        if !val_report.is_valid {
            return Err(AppError::invalid_input(format!(
                "Preprocessing configuration incompatible with model '{}': {}",
                ai_config.model_id,
                val_report.errors.join("; ")
            )));
        }

        let provider_name = format!("{:?}", r.provider());

        let mut metrics = AiJobMetrics {
            frames_total: total_frames,
            frames_selected: selected_indices.len(),
            frames_processed: 0,
            frames_reused: 0,
            frames_passthrough: 0,
            frames_failed: 0,
            total_inference_duration_ms: 0.0,
            average_inference_duration_ms: 0.0,
            min_inference_duration_ms: f64::MAX,
            max_inference_duration_ms: 0.0,
            total_pipeline_duration_ms: 0.0,
            artifact_bytes_written: 0,
            eta_ms: None,
        };

        // 7. Memory-Bounded Frame Processing Loop (Single frame in flight)
        for (frame_idx, frame_path) in frame_paths.iter().enumerate() {
            // Check cancellation before processing frame
            if let Some(ref token) = cancel_token {
                if token.load(Ordering::SeqCst) {
                    return Err(AppError::cancelled());
                }
            }

            if !selected_set.contains(&frame_idx) {
                // Passthrough frame
                let written = artifact_manager.write_passthrough_frame(frame_idx, frame_path)?;
                metrics.frames_passthrough += 1;
                metrics.artifact_bytes_written += written;

                let progress = (frame_idx + 1) as f32 / total_frames as f32 * 100.0;
                on_progress(progress, None, &metrics);
                continue;
            }

            // Check if frame artifact can be reused from previous run with deep validation
            if let Some(cached_meta) = artifact_manager.validate_frame_artifact_deep(
                frame_idx,
                &ai_config.model_id,
                &config_hash,
                ai_config.model_hash.as_deref(),
                ai_config.profile_hash.as_deref(),
            ) {
                metrics.frames_reused += 1;
                metrics.frames_processed += 1;
                metrics.total_inference_duration_ms += cached_meta.inference_duration_ms;
                metrics.min_inference_duration_ms = metrics
                    .min_inference_duration_ms
                    .min(cached_meta.inference_duration_ms);
                metrics.max_inference_duration_ms = metrics
                    .max_inference_duration_ms
                    .max(cached_meta.inference_duration_ms);
                metrics.average_inference_duration_ms =
                    metrics.total_inference_duration_ms / metrics.frames_processed as f64;

                let progress = (frame_idx + 1) as f32 / total_frames as f32 * 100.0;
                on_progress(progress, Some(&cached_meta), &metrics);
                continue;
            }

            // Execute frame inference with bounded lifetime
            let frame_start = Instant::now();

            // Decode image from disk (1 image in RAM)
            let decode_start = Instant::now();
            let source_image = ImageFrame::decode_from_file(frame_path)?;
            let decode_duration_ms = decode_start.elapsed().as_secs_f64() * 1000.0;
            let in_width = source_image.width;
            let in_height = source_image.height;

            // Resource limit check on decoded frame
            limits.validate_frame_dimensions(in_width, in_height)?;

            // Preprocess to tensor
            let prep_start = Instant::now();
            let prep_result =
                preprocess_image(&source_image, &ai_config.preprocessing, target_tensor_name)?;
            let preprocess_duration_ms = prep_start.elapsed().as_secs_f64() * 1000.0;

            // Validate tensor element count against limits
            limits.validate_tensor_elements(&prep_result.tensor.shape)?;

            // Check cancellation right before inference
            if let Some(ref token) = cancel_token {
                if token.load(Ordering::SeqCst) {
                    return Err(AppError::cancelled());
                }
            }

            // ONNX Inference (under session lock)
            let infer_req = InferenceRequest {
                model_id: ai_config.model_id.clone(),
                inputs: vec![prep_result.tensor],
            };
            let infer_res = r.infer(&infer_req)?;
            let inference_duration_ms = infer_res.inference_duration_ms;

            // Postprocess
            let default_post = PostprocessConfig::default();
            let post_cfg = ai_config.postprocessing.as_ref().unwrap_or(&default_post);
            let post_res = postprocess_outputs(&infer_res.outputs, post_cfg)?;

            // Encode output PNG
            let is_mask = matches!(ai_config.output_mode, AiFrameOutputMode::Mask);
            let png_bytes = match ai_config.output_mode {
                AiFrameOutputMode::Mask => {
                    if let Some(ref m) = post_res.mask {
                        m.mask_to_png_bytes()?
                    } else {
                        source_image.encode_to_png_bytes()?
                    }
                }
                AiFrameOutputMode::Image => {
                    if let Some(first_output) = infer_res.outputs.first() {
                        if let Some(ref f32_data) = first_output.data_f32 {
                            let tensor_dims = &first_output.shape;
                            let (c, h, w) = if tensor_dims.len() == 4 {
                                (
                                    tensor_dims[1] as usize,
                                    tensor_dims[2] as u32,
                                    tensor_dims[3] as u32,
                                )
                            } else if tensor_dims.len() == 3 {
                                (
                                    tensor_dims[0] as usize,
                                    tensor_dims[1] as u32,
                                    tensor_dims[2] as u32,
                                )
                            } else {
                                (1, in_height, in_width)
                            };

                            let mut u8_data = Vec::with_capacity(f32_data.len());
                            for &val in f32_data {
                                let clamped = if val <= 1.0 && val >= 0.0 {
                                    (val * 255.0).round().clamp(0.0, 255.0) as u8
                                } else {
                                    val.round().clamp(0.0, 255.0) as u8
                                };
                                u8_data.push(clamped);
                            }

                            let format = match c {
                                1 => PixelFormat::Gray8,
                                4 => PixelFormat::Rgba8,
                                _ => PixelFormat::Rgb8,
                            };

                            if u8_data.len() == (w * h * c as u32) as usize {
                                let out_frame = ImageFrame::new(w, h, format, u8_data)?;
                                out_frame.encode_to_png_bytes()?
                            } else {
                                source_image.encode_to_png_bytes()?
                            }
                        } else {
                            source_image.encode_to_png_bytes()?
                        }
                    } else if let Some(ref m) = post_res.mask {
                        m.mask_to_png_bytes()?
                    } else {
                        source_image.encode_to_png_bytes()?
                    }
                }
            };

            // 8. Quality Validation Gate for output artifact
            let quality_report = FrameQualityValidator::validate_png_bytes(
                frame_idx,
                &png_bytes,
                Some(prep_result.processed_width),
                Some(prep_result.processed_height),
                is_mask,
            )?;

            if !quality_report.is_valid {
                return Err(AppError::frame_quality_failed(
                    format!("Frame {:06} quality validation failed", frame_idx),
                    quality_report.errors.join("; "),
                ));
            }

            // Check disk budget before write
            limits.validate_disk_budget(
                metrics.artifact_bytes_written,
                (png_bytes.len() * 2) as u64,
            )?;

            // Check cancellation before atomic write
            if let Some(ref token) = cancel_token {
                if token.load(Ordering::SeqCst) {
                    return Err(AppError::cancelled());
                }
            }

            let total_frame_dur = frame_start.elapsed().as_secs_f64() * 1000.0;

            let meta = AiFrameMetadata {
                job_id: None,
                frame_index: frame_idx,
                source_frame_index: frame_idx,
                status: AiFrameStatus::Completed,
                model_id: ai_config.model_id.clone(),
                model_version: ai_config.model_version.clone(),
                model_hash: ai_config.model_hash.clone(),
                profile_hash: ai_config.profile_hash.clone(),
                provider: provider_name.clone(),
                decode_duration_ms,
                preprocess_duration_ms,
                inference_duration_ms,
                postprocess_duration_ms: post_res.postprocess_duration_ms,
                total_duration_ms: total_frame_dur,
                input_width: in_width,
                input_height: in_height,
                output_width: prep_result.processed_width,
                output_height: prep_result.processed_height,
                output_artifact_path: format!("{:06}/output.png", frame_idx),
                config_hash: config_hash.clone(),
                artifact_hash: None,
                artifact_size_bytes: None,
                created_at: None,
            };

            // Write artifact atomically
            let written_bytes = artifact_manager.write_frame_artifact(&meta, &png_bytes)?;
            metrics.artifact_bytes_written += written_bytes;

            metrics.frames_processed += 1;
            metrics.total_inference_duration_ms += inference_duration_ms;
            metrics.min_inference_duration_ms =
                metrics.min_inference_duration_ms.min(inference_duration_ms);
            metrics.max_inference_duration_ms =
                metrics.max_inference_duration_ms.max(inference_duration_ms);
            metrics.average_inference_duration_ms =
                metrics.total_inference_duration_ms / metrics.frames_processed as f64;

            // Deterministic ETA calculation (only when >= 2 frames processed)
            if metrics.frames_processed >= 2 {
                let remaining_frames = (total_frames.saturating_sub(frame_idx + 1)) as f64;
                metrics.eta_ms = Some(remaining_frames * metrics.average_inference_duration_ms);
            } else {
                metrics.eta_ms = None;
            }

            let progress = ((frame_idx + 1) as f32 / total_frames as f32) * 100.0;
            on_progress(progress, Some(&meta), &metrics);

            // Frame memory (source_image, prep_result, png_bytes) is dropped at scope end
        }

        // Finalize aggregate metrics
        metrics.total_pipeline_duration_ms = pipeline_start.elapsed().as_secs_f64() * 1000.0;
        if metrics.frames_processed > 0 {
            metrics.average_inference_duration_ms =
                metrics.total_inference_duration_ms / metrics.frames_processed as f64;
        } else {
            metrics.min_inference_duration_ms = 0.0;
        }
        if metrics.min_inference_duration_ms == f64::MAX {
            metrics.min_inference_duration_ms = 0.0;
        }

        Ok(metrics)
    }
}
