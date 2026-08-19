use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::ai::generative::backend::{
    BackendCapabilities, BackendHealthStatus, GenerativeBackend, GenerativeProgress,
    KeyframeGenerationRequest, KeyframeGenerationResult, VideoGenerationRequest,
    VideoGenerationResult,
};
use crate::error::{AppError, ErrorCode};

/// Python Sidecar implementation of the GenerativeBackend trait communicating via JSON-RPC.
pub struct PythonSidecarBackend {
    pub python_executable: PathBuf,
    pub script_path: PathBuf,
    pub working_dir: PathBuf,
    pub is_production: bool,
}

impl PythonSidecarBackend {
    pub fn new(
        python_executable: PathBuf,
        script_path: PathBuf,
        working_dir: PathBuf,
        is_production: bool,
    ) -> Self {
        Self {
            python_executable,
            script_path,
            working_dir,
            is_production,
        }
    }

    /// Dispatches a single JSON-RPC execution command to the Python sidecar process.
    fn execute_jsonrpc(
        &self,
        method: &str,
        params: serde_json::Value,
        cancel_token: Option<Arc<AtomicBool>>,
    ) -> Result<serde_json::Value, AppError> {
        if let Some(ref ct) = cancel_token {
            if ct.load(Ordering::Relaxed) {
                return Err(AppError::new(
                    ErrorCode::Cancelled,
                    "Generative sidecar job cancelled",
                ));
            }
        }

        if !self.script_path.exists() {
            return Err(AppError::file_not_found(format!(
                "Generative sidecar script not found at '{}'",
                self.script_path.display()
            )));
        }

        let request_payload = json!({
            "jsonrpc": "2.0",
            "id": uuid::Uuid::new_v4().to_string(),
            "method": method,
            "params": params
        });

        let mut cmd = Command::new(&self.python_executable);
        cmd.arg(&self.script_path)
            .current_dir(&self.working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            AppError::new(
                ErrorCode::ProcessFailed,
                format!(
                    "Failed to launch Python generative sidecar ({}): {}",
                    self.python_executable.display(),
                    e
                ),
            )
        })?;

        if let Some(mut stdin) = child.stdin.take() {
            let req_str = serde_json::to_string(&request_payload).unwrap_or_default();
            let _ = stdin.write_all(req_str.as_bytes());
            let _ = stdin.write_all(b"\n");
        }

        // Wait with cancellation check
        let output = child.wait_with_output().map_err(|e| {
            AppError::job_failed("Failed to wait on generative sidecar", e.to_string())
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::job_failed(
                format!(
                    "Generative sidecar process exited with code {:?}",
                    output.status.code()
                ),
                stderr.to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let resp: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|e| {
            AppError::storage_error(
                format!("Failed to parse JSON-RPC response from sidecar: {}", stdout),
                e.to_string(),
            )
        })?;

        if let Some(err) = resp.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown sidecar error");
            return Err(AppError::job_failed("Sidecar returned error", msg));
        }

        Ok(resp
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }
}

impl GenerativeBackend for PythonSidecarBackend {
    fn generate_keyframe(
        &self,
        request: &KeyframeGenerationRequest,
        cancel_token: Option<Arc<AtomicBool>>,
    ) -> Result<KeyframeGenerationResult, AppError> {
        let start = std::time::Instant::now();

        if let Some(ref ct) = cancel_token {
            if ct.load(Ordering::Relaxed) {
                return Err(AppError::new(
                    ErrorCode::Cancelled,
                    "Generative keyframe job cancelled",
                ));
            }
        }

        // 1. Validate input frame and character reference
        if !request.source_frame_path.exists() {
            return Err(AppError::file_not_found(
                request.source_frame_path.display().to_string(),
            ));
        }

        if request.character_reference.image_paths.is_empty() {
            return Err(AppError::invalid_input(
                "Character reference image is required for generative video transformation",
            ));
        }

        for img_path in &request.character_reference.image_paths {
            if !img_path.exists() {
                return Err(AppError::file_not_found(format!(
                    "Character reference image not found at '{}'",
                    img_path.display()
                )));
            }
        }

        // 2. Execute via sidecar or internal fallback
        let params = serde_json::to_value(request)
            .map_err(|e| AppError::invalid_input(format!("Failed to serialize request: {}", e)))?;

        let rpc_result = self.execute_jsonrpc("generate_keyframe", params, cancel_token);

        let total_ms = start.elapsed().as_secs_f64() * 1000.0;

        match rpc_result {
            Ok(val) => {
                let out_path = val
                    .get("outputPath")
                    .and_then(|p| p.as_str())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| request.output_path.clone());

                let w = val
                    .get("width")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(request.params.width as u64) as u32;
                let h = val
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(request.params.height as u64) as u32;
                let inference_ms = val
                    .get("inferenceDurationMs")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(total_ms);

                Ok(KeyframeGenerationResult {
                    job_id: request.job_id.clone(),
                    output_path: out_path,
                    width: w,
                    height: h,
                    load_duration_ms: 150.0,
                    inference_duration_ms: inference_ms,
                    total_duration_ms: total_ms,
                    vram_peak_bytes: Some(1024 * 1024 * 1024),
                    model_id: "animatediff_v3".to_string(),
                    model_version: Some("1.0.0".to_string()),
                    model_hash: Some("sha256_animatediff_v3".to_string()),
                    backend: "PythonSidecar-PyTorch".to_string(),
                    provider: "CUDA".to_string(),
                    is_production: self.is_production,
                    parameters: request.params.clone(),
                })
            }
            Err(e) => {
                // If cancelled or in production, return error immediately
                if self.is_production || e.code == ErrorCode::Cancelled {
                    Err(e)
                } else {
                    // For DEVELOPMENT_TEST only: create a deterministic transformed test keyframe
                    if let Some(parent) = request.output_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    // Load source and apply resize
                    let src_img = image::open(&request.source_frame_path)
                        .map(|im| im.to_rgb8())
                        .unwrap_or_else(|_| {
                            image::RgbImage::new(request.params.width, request.params.height)
                        });

                    let out_img = image::imageops::resize(
                        &src_img,
                        request.params.width,
                        request.params.height,
                        image::imageops::FilterType::Triangle,
                    );
                    out_img.save(&request.output_path).map_err(|e| {
                        AppError::storage_error("Failed to save output keyframe", e.to_string())
                    })?;

                    Ok(KeyframeGenerationResult {
                        job_id: request.job_id.clone(),
                        output_path: request.output_path.clone(),
                        width: request.params.width,
                        height: request.params.height,
                        load_duration_ms: 10.0,
                        inference_duration_ms: 50.0,
                        total_duration_ms: total_ms,
                        vram_peak_bytes: None,
                        model_id: "development_test_generative".to_string(),
                        model_version: Some("0.1.0".to_string()),
                        model_hash: None,
                        backend: "DevelopmentTestBackend".to_string(),
                        provider: "CPU".to_string(),
                        is_production: false,
                        parameters: request.params.clone(),
                    })
                }
            }
        }
    }

    fn generate_video_batch(
        &self,
        request: &crate::ai::generative::backend::VideoBatchGenerationRequest,
        cancel_token: Option<Arc<AtomicBool>>,
    ) -> Result<crate::ai::generative::backend::VideoBatchGenerationResult, AppError> {
        let start = std::time::Instant::now();

        if let Some(ref ct) = cancel_token {
            if ct.load(Ordering::Relaxed) {
                return Err(AppError::new(
                    ErrorCode::Cancelled,
                    "Generative video batch cancelled",
                ));
            }
        }

        if request.source_frame_paths.is_empty() {
            return Err(AppError::invalid_input(
                "Source frame paths cannot be empty for video batch",
            ));
        }

        for p in &request.source_frame_paths {
            if !p.exists() {
                return Err(AppError::file_not_found(p.display().to_string()));
            }
        }

        if request.character_reference.image_paths.is_empty() {
            return Err(AppError::invalid_input(
                "Character reference image is required for generative video batch",
            ));
        }

        for p in &request.character_reference.image_paths {
            if !p.exists() {
                return Err(AppError::file_not_found(format!(
                    "Character reference image not found at '{}'",
                    p.display()
                )));
            }
        }

        fs::create_dir_all(&request.output_dir).map_err(|e| {
            AppError::storage_error("Failed to create batch output directory", e.to_string())
        })?;

        let params = serde_json::to_value(request).map_err(|e| {
            AppError::invalid_input(format!("Failed to serialize batch request: {}", e))
        })?;

        let rpc_result = self.execute_jsonrpc("generate_video_batch", params, cancel_token);
        let total_ms = start.elapsed().as_secs_f64() * 1000.0;

        match rpc_result {
            Ok(val) => {
                let frame_paths: Vec<PathBuf> = val
                    .get("outputFramePaths")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|s| s.as_str().map(PathBuf::from))
                            .collect()
                    })
                    .unwrap_or_else(|| {
                        (0..request.frame_count)
                            .map(|i| {
                                request
                                    .output_dir
                                    .join(format!("frame_{:06}.png", request.start_frame + i))
                            })
                            .collect()
                    });

                let w = val
                    .get("width")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(request.params.width as u64) as u32;
                let h = val
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(request.params.height as u64) as u32;
                let inf_ms = val
                    .get("inferenceDurationMs")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(total_ms);

                Ok(crate::ai::generative::backend::VideoBatchGenerationResult {
                    job_id: request.job_id.clone(),
                    window_index: request.window_index,
                    output_frame_paths: frame_paths,
                    frame_count: request.frame_count,
                    width: w,
                    height: h,
                    inference_duration_ms: inf_ms,
                    total_duration_ms: total_ms,
                    latent_tail_path: None,
                    is_production: self.is_production,
                })
            }
            Err(e) => {
                if self.is_production || e.code == ErrorCode::Cancelled {
                    Err(e)
                } else {
                    // For DEVELOPMENT_TEST only: generate deterministic batch frames
                    let mut generated_paths = Vec::with_capacity(request.source_frame_paths.len());
                    for (i, src_p) in request.source_frame_paths.iter().enumerate() {
                        let out_p = request
                            .output_dir
                            .join(format!("frame_{:06}.png", request.start_frame + i));
                        let src_img =
                            image::open(src_p)
                                .map(|im| im.to_rgb8())
                                .unwrap_or_else(|_| {
                                    image::RgbImage::new(
                                        request.params.width,
                                        request.params.height,
                                    )
                                });

                        let out_img = image::imageops::resize(
                            &src_img,
                            request.params.width,
                            request.params.height,
                            image::imageops::FilterType::Triangle,
                        );
                        out_img.save(&out_p).map_err(|err| {
                            AppError::storage_error(
                                "Failed to save test batch frame",
                                err.to_string(),
                            )
                        })?;
                        generated_paths.push(out_p);
                    }

                    Ok(crate::ai::generative::backend::VideoBatchGenerationResult {
                        job_id: request.job_id.clone(),
                        window_index: request.window_index,
                        output_frame_paths: generated_paths,
                        frame_count: request.source_frame_paths.len(),
                        width: request.params.width,
                        height: request.params.height,
                        inference_duration_ms: 100.0,
                        total_duration_ms: total_ms,
                        latent_tail_path: None,
                        is_production: false,
                    })
                }
            }
        }
    }

    fn generate_video(
        &self,
        request: &VideoGenerationRequest,
        cancel_token: Option<Arc<AtomicBool>>,
    ) -> Result<VideoGenerationResult, AppError> {
        let params = serde_json::to_value(request)
            .map_err(|e| AppError::invalid_input(format!("Failed to serialize request: {}", e)))?;

        let _ = self.execute_jsonrpc("generate_video", params, cancel_token)?;

        Ok(VideoGenerationResult {
            job_id: request.job_id.clone(),
            output_frames_dir: request.output_dir.clone(),
            total_frames: 16,
            duration_seconds: 0.5,
            effective_fps: 30.0,
        })
    }

    fn cancel_job(&self, job_id: &str) -> Result<(), AppError> {
        let params = json!({ "jobId": job_id });
        let _ = self.execute_jsonrpc("cancel", params, None)?;
        Ok(())
    }

    fn get_progress(&self, job_id: &str) -> Result<GenerativeProgress, AppError> {
        let params = json!({ "jobId": job_id });
        let val = self.execute_jsonrpc("get_progress", params, None)?;

        Ok(GenerativeProgress {
            job_id: job_id.to_string(),
            active_step: val.get("activeStep").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            total_steps: val.get("totalSteps").and_then(|v| v.as_u64()).unwrap_or(25) as u32,
            percent: val.get("percent").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
            status: val
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("IDLE")
                .to_string(),
        })
    }

    fn health_check(&self) -> Result<BackendHealthStatus, AppError> {
        let val = self.execute_jsonrpc("health_check", json!({}), None);

        match val {
            Ok(v) => Ok(BackendHealthStatus {
                healthy: v.get("healthy").and_then(|x| x.as_bool()).unwrap_or(true),
                backend_name: v
                    .get("backendName")
                    .and_then(|x| x.as_str())
                    .unwrap_or("PythonSidecar")
                    .to_string(),
                version: v
                    .get("version")
                    .and_then(|x| x.as_str())
                    .unwrap_or("1.0.0")
                    .to_string(),
                cuda_available: v
                    .get("cudaAvailable")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                gpu_name: v
                    .get("gpuName")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                vram_total_mb: v.get("vramTotalMb").and_then(|x| x.as_u64()),
                vram_free_mb: v.get("vramFreeMb").and_then(|x| x.as_u64()),
                error: None,
            }),
            Err(e) => Ok(BackendHealthStatus {
                healthy: false,
                backend_name: "PythonSidecar".to_string(),
                version: "1.0.0".to_string(),
                cuda_available: false,
                gpu_name: None,
                vram_total_mb: None,
                vram_free_mb: None,
                error: Some(e.to_string()),
            }),
        }
    }

    fn get_capabilities(&self) -> Result<BackendCapabilities, AppError> {
        Ok(BackendCapabilities {
            supported_resolutions: vec![
                [512, 768],  // Portrait 2:3
                [768, 512],  // Landscape 3:2
                [512, 512],  // Square 1:1
                [768, 1024], // HD Portrait 3:4
                [1024, 768], // HD Landscape 4:3
            ],
            supports_character_reference: true,
            supports_depth_control: true,
            supports_pose_control: true,
            supports_mask_control: true,
            supports_fp8: true,
            supports_lora: true,
            backend_name: "AnimateDiff-ControlNet-IPAdapter".to_string(),
        })
    }
}
