# AutoVideo AI — AI Pipeline Specification

## Pipeline Phases

The transformation pipeline converts an input video $V_{\text{in}}$ into a transformed output video $V_{\text{out}}$ through a multi-stage sequential pipeline:

```
[Input Video] 
     │
     ▼
Phase 1: Video Analysis & Frame Extraction
  - Demux audio & video streams via FFmpeg.
  - Extract frames to temporary buffer workspace.
  - Perform scene detection & keyframe indexing via `AnalysisEngine`.
     │
     ▼
Phase 2: Transformation Planning
  - Parse prompt & configuration via `TransformationEngine`.
  - Validate AI runtime & model weight presence via `ModelManager`.
  - If model weights missing, emit `MODEL_NOT_AVAILABLE` status & halt pipeline.
     │
     ▼
Phase 3: Region & Subject Isolation
  - Generate object masks / character bboxes via `CharacterTransformationEngine`.
  - Extract background depth maps via `BackgroundTransformationEngine`.
     │
     ▼
Phase 4: Frame Inpainting & Diffusion Inference
  - Run frame-level inference using `InferenceRuntime`.
  - Enforce cross-frame consistency via `TemporalConsistencyEngine` (optical flow + deflicker).
     │
     ▼
Phase 5: Audio Re-stitching & Final Encoding
  - Re-align original audio track via `AudioEngine`.
  - Encode output frames using FFmpeg H.264/HEVC hardware encoder into $V_{\text{out}}$.
```

---

## AI Model Availability & Non-Faking Protocol

The backend pipeline enforces strict runtime status checks prior to job execution:

```rust
pub enum AiAvailabilityStatus {
    Available,
    ModelNotAvailable { missing_model: String, guidance: String },
    ModelBlocked { reason: String },
    RuntimeNotAvailable { runtime_type: String },
}
```

If `AiAvailabilityStatus` is not `Available`, the Job Manager aborts execution with a structured error, preventing any simulated or fake completion.

---

## Engine Trait Definitions

```rust
pub trait AnalysisEngine: Send + Sync {
    fn analyze_video(&self, video_path: &Path) -> Result<AnalysisReport, AiError>;
}

pub trait CharacterTransformationEngine: Send + Sync {
    fn replace_character(
        &self, 
        frames: &[PathBuf], 
        prompt: &str, 
        config: &CharacterConfig
    ) -> Result<Vec<PathBuf>, AiError>;
}

pub trait TemporalConsistencyEngine: Send + Sync {
    fn smooth_frames(&self, frames: &[PathBuf]) -> Result<Vec<PathBuf>, AiError>;
}
```
