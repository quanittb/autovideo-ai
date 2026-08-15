# AutoVideo AI — System Architecture

## Architectural Boundary

AutoVideo AI enforces a strict system boundary separating the UI presentation layer from core media/AI execution:

```
React UI (TypeScript + Tailwind CSS + Zustand + TanStack Query)
       │
       ▼  Tauri Commands (IPC) & Events (Async Progress)
Rust Core (Tauri 2 + Tokio + FFmpeg Rust Bindings + Pipeline Orchestration)
       │
       ▼  Engine Interfaces & Task Queues
Media / Jobs / AI Runtime / Models
       │
       ▼  Adapters
Local AI Runtime (ONNX / LibTorch / TensorRT / DirectML)  or  Future Cloud Adapter
```

### Boundary Constraints
- **React UI**: Responsible purely for rendering UI state, taking user inputs, rendering preview buffers, and displaying job progress. React NEVER directly executes shell commands, FFmpeg binaries, Python scripts, or raw model weights.
- **Rust Core**: Owns file system access, process lifecycle, job queuing, hardware detection, FFmpeg invocation, frame buffer management, and AI engine traits execution.

---

## AI Architecture & Engine Abstractions

To ensure replaceable, model-agnostic operation, the Rust Core defines key engine traits:

1. **`AnalysisEngine`**: Analyzes input video (scene detection, subject segmentation, camera movement analysis, keyframe selection).
2. **`TransformationEngine`**: High-level planner mapping user prompt/config to execution sub-pipelines.
3. **`CharacterTransformationEngine`**: Handles subject segmentation, pose tracking, inpainting, and character swap.
4. **`BackgroundTransformationEngine`**: Handles background segment isolation, environment diffusion, and depth consistency.
5. **`TemporalConsistencyEngine`**: Applies optical flow alignment, deflickering, and temporal cross-attention frame alignment.
6. **`AudioEngine`**: Audio demuxing, waveform matching, noise reduction, and final AV sync remuxing.
7. **`InferenceRuntime`**: Low-level executor abstraction for ONNX Runtime / Torch / DirectML / TensorRT.
8. **`ModelProvider`**: Provides access to local weights or cloud endpoints.
9. **`ModelManager`**: Manages downloading, verification, caching, and VRAM loading/unloading of model artifacts.

---

## Job State Machine

All transformation tasks run as asynchronous jobs in Rust managed by a thread-safe Job Manager.

```
       ┌──────────┐
       │  QUEUED  │
       └────┬─────┘
            │ start
            ▼
       ┌──────────┐ ◄──────┐ resume
       │ RUNNING  ├───────┤
       └─┬──┬──┬──┘ pause │
         │  │  └──────────┘
         │  │   ┌──────────┐
         │  ├──►│ PAUSED   │
         │  │   └──────────┘
  cancel │  │ failure
         │  └─────────────┐
         ▼                ▼
  ┌────────────┐   ┌──────────┐
  │ CANCELLING │   │  FAILED  │
  └─────┬──────┘   └──────────┘
        │
        ▼
  ┌────────────┐   ┌───────────┐
  │ CANCELLED  │   │ COMPLETED │
  └────────────┘   └───────────┘
```

### Job States
- `QUEUED`: Enqueued in processing queue.
- `RUNNING`: Active processing (frame extraction, inference, or encoding).
- `PAUSED`: Temporarily paused by user; VRAM cached.
- `CANCELLING`: Cancellation requested, cleaning up intermediate frame buffers.
- `CANCELLED`: Fully terminated and temporary files cleaned up.
- `FAILED`: Encountered error (reports structured `AiAvailabilityStatus` or process error).
- `COMPLETED`: Transformed video artifact finalized and verified.

---

## Cross-Platform Rules

- Path handling uses Rust `std::path::PathBuf` and platform-agnostic URI resolution.
- Hardware acceleration query supports DirectML / CUDA (Windows) and Metal / MPS (macOS Apple Silicon).
