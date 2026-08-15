# AutoVideo AI — System Architecture Specification

## 1. System Overview & Layered Architecture

AutoVideo AI is architected with clear boundaries, separation of concerns, and loose coupling between presentation, media operations, and AI inference engines:

```
┌────────────────────────────────────────────────────────────────────────┐
│ 1. FRONTEND LAYER (React 19 + TypeScript + Vite + Tailwind + Zustand) │
│    - Feature-Oriented UI Components & Step Wizard                      │
│    - Zustand Stores: uiStore, projectStore, jobStore, settingsStore    │
│    - Typed IPC Client via Tauri 2 API (@tauri-apps/api)                │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Tauri Commands & Typed Events
┌───────────────────────────────────▼────────────────────────────────────┐
│ 2. RUST APPLICATION CORE (Tauri 2 + Tokio Async Runtime)              │
│    - Command Handlers & Event Dispatch                                 │
│    - Structured Error Domain (`AppError`, standard error codes)        │
│    - Hardware Profiling & System Storage Management                    │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
       ┌────────────────────────────┼────────────────────────────┐
       │                            │                            │
┌──────▼─────────────┐   ┌──────────▼──────────┐   ┌─────────────▼───────┐
│ 3. PROJECT LAYER   │   │ 4. MEDIA LAYER      │   │ 5. JOB LAYER        │
│ - Project Config   │   │ - FFmpeg Pipelines  │   │ - Async Job Engine  │
│ - Metadata Storage │   │ - Frame Extraction  │   │ - Stage Transitions │
│ - Persistence      │   │ - Audio Extraction  │   │ - Cancellable Tasks │
└────────────────────┘   └─────────────────────┘   └─────────────┬───────┘
                                                                 │
       ┌─────────────────────────────────────────────────────────┼───────┐
       │                                                         │       │
┌──────▼─────────────┐   ┌─────────────────────┐   ┌─────────────▼───────▼───────┐
│ 6. AI LAYER        │   │ 7. MODEL LAYER      │   │ 8. RUNTIME LAYER    │ 9. EXPORT LAYER
│ - AnalysisEngine   │   │ - ModelDescriptor   │   │ - InferenceRuntime  │ - H.264/HEVC
│ - TransformEngine  │   │ - ModelProvider     │   │ - DirectML / Metal  │ - AV Sync
│ - CharTransform    │   │ - ModelManager      │   │ - Cloud Adapters    │ - Multi-res
│ - TempConsistency  │   │ - Weight Validation │   │                     │
└────────────────────┘   └─────────────────────┘   └─────────────────────┘
```

---

## 2. Layer Descriptions

### Layer 1: Frontend Layer (`src/`)
- Pure client-side UI rendering with React 19, TypeScript, and Tailwind CSS.
- Organised by features: `home`, `project`, `transform`, `processing`, `result`, `export`, `settings`, `models`.
- State managed through modular Zustand stores (`uiStore`, `projectStore`, `jobStore`, `settingsStore`).
- Communicates with Rust Core exclusively through typed Tauri commands and listenable event streams.
- **Rule**: React NEVER executes shell commands, spawns child processes, or accesses raw file systems directly.

### Layer 2: Rust Application Core (`src-tauri/src/`)
- Serves as the central coordinator and security boundary.
- Manages application lifecycle, hardware detection, error translation, and inter-process communication.

### Layer 3: Project Layer (`src-tauri/src/projects/`)
- Manages project metadata, input source paths, transformation configurations, and project serialization.
- Independent of media codecs and AI inference models.

### Layer 4: Media Layer (`src-tauri/src/media/`)
- Encapsulates FFmpeg operations for frame extraction, video demuxing, video metadata extraction, and audio alignment.
- Operates on temporary workspace directories managed by the System layer.

### Layer 5: Job Layer (`src-tauri/src/jobs/`)
- Manages background asynchronous long-running tasks.
- Enforces the Job State Machine: `QUEUED`, `RUNNING`, `PAUSED`, `CANCELLING`, `CANCELLED`, `FAILED`, `COMPLETED`.
- Emits progress events across stages: `ExtractingFrames`, `Analyzing`, `GeneratingMasks`, `Inpainting`, `TemporalSmoothing`, `StitchingAudio`, `EncodingVideo`, `Finalizing`.

### Layer 6: AI Layer (`src-tauri/src/ai/`)
- Defines abstract traits for video intelligence:
  - `AnalysisEngine`: Subject segmentation, scene detection, keyframe selection.
  - `TransformationEngine`: Planning sub-pipelines from prompts.
  - `CharacterTransformationEngine`: Subject inpainting and character swap (MVP focus).
  - `BackgroundTransformationEngine`: Background inpainting and depth alignment (Post-MVP).
  - `TemporalConsistencyEngine`: Optical flow deflickering and cross-frame attention.
  - `AudioEngine`: Audio track isolation and sync restoration.

### Layer 7: Model Layer (`src-tauri/src/models/`)
- Defines `ModelDescriptor`, `ModelProvider`, and `ModelManager`.
- Validates model weight checksums, file sizes, and VRAM memory requirements.
- Strictly returns `MODEL_NOT_AVAILABLE` when model weights are not loaded.

### Layer 8: Runtime Layer (`src-tauri/src/runtime/`)
- Abstraction over low-level execution backends (ONNX Runtime, LibTorch, DirectML, Metal, or future Cloud AI endpoints).

### Layer 9: Export Layer (`src-tauri/src/export/`)
- Final video encoding and muxing with user-specified resolution (1080p, 4K), bitrate, FPS, and format container.

---

## 3. Storage Strategy

All file storage is platform-independent, resolved via standard system paths:
- **Projects**: `{AppData}/AutoVideoAI/projects/`
- **Models**: `{AppData}/AutoVideoAI/models/`
- **Cache / Temp Frames**: `{CacheDir}/AutoVideoAI/temp/`
- **Logs**: `{AppData}/AutoVideoAI/logs/`
