use serde::{Deserialize, Serialize};

/// Production benchmark report measuring real end-to-end execution timing and throughput.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiJobBenchmarkReport {
    pub job_id: String,
    pub model_id: String,
    pub model_version: Option<String>,
    pub model_hash: Option<String>,
    pub is_production: bool,
    pub provider: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub total_frames: usize,
    pub selected_frames: usize,
    pub processed_frames: usize,
    pub reused_frames: usize,
    pub passthrough_frames: usize,
    pub model_load_ms: f64,
    pub decode_avg_ms: f64,
    pub preprocess_avg_ms: f64,
    pub inference_avg_ms: f64,
    pub inference_min_ms: f64,
    pub inference_max_ms: f64,
    pub postprocess_avg_ms: f64,
    pub reconstruction_ms: f64,
    pub total_duration_seconds: f64,
    pub effective_fps: f64,
    pub effective_inference_fps: f64,
}

impl AiJobBenchmarkReport {
    /// Computes benchmark report from real measured pipeline durations.
    pub fn compute(
        job_id: &str,
        model_id: &str,
        model_version: Option<&str>,
        model_hash: Option<&str>,
        is_production: bool,
        provider: &str,
        frame_width: u32,
        frame_height: u32,
        total_frames: usize,
        selected_frames: usize,
        processed_frames: usize,
        reused_frames: usize,
        passthrough_frames: usize,
        model_load_ms: f64,
        decode_total_ms: f64,
        preprocess_total_ms: f64,
        inference_total_ms: f64,
        inference_min_ms: f64,
        inference_max_ms: f64,
        postprocess_total_ms: f64,
        reconstruction_ms: f64,
        total_duration_ms: f64,
    ) -> Self {
        let count = (processed_frames - reused_frames).max(1) as f64;
        let decode_avg_ms = decode_total_ms / count;
        let preprocess_avg_ms = preprocess_total_ms / count;
        let inference_avg_ms = if processed_frames > 0 {
            inference_total_ms / processed_frames as f64
        } else {
            0.0
        };
        let postprocess_avg_ms = postprocess_total_ms / count;

        let total_duration_seconds = total_duration_ms / 1000.0;
        let effective_fps = if total_duration_seconds > 0.0 {
            total_frames as f64 / total_duration_seconds
        } else {
            0.0
        };

        let inference_total_sec = inference_total_ms / 1000.0;
        let effective_inference_fps = if inference_total_sec > 0.0 {
            processed_frames as f64 / inference_total_sec
        } else {
            0.0
        };

        Self {
            job_id: job_id.to_string(),
            model_id: model_id.to_string(),
            model_version: model_version.map(|s| s.to_string()),
            model_hash: model_hash.map(|s| s.to_string()),
            is_production,
            provider: provider.to_string(),
            frame_width,
            frame_height,
            total_frames,
            selected_frames,
            processed_frames,
            reused_frames,
            passthrough_frames,
            model_load_ms,
            decode_avg_ms,
            preprocess_avg_ms,
            inference_avg_ms,
            inference_min_ms: if inference_min_ms == f64::MAX {
                0.0
            } else {
                inference_min_ms
            },
            inference_max_ms,
            postprocess_avg_ms,
            reconstruction_ms,
            total_duration_seconds,
            effective_fps,
            effective_inference_fps,
        }
    }
}
