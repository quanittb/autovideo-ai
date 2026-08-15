# ADR 0001: System Architecture, Boundaries, and AI Engine Abstraction

- **Status**: Approved
- **Date**: 2026-08-15
- **Deciders**: AutoVideo AI Lead Architect

---

## Context

AutoVideo AI is a desktop application that automates complex video transformations using local/cloud AI and media pipelines. A robust, secure, and maintainable architectural foundation is required before adding AI inference and FFmpeg bindings.

---

## Key Decisions & Rationales

### 1. Why Tauri Owns System & Media Operations
- **Security & Sandboxing**: Exposing direct shell execution or raw filesystem APIs to the React web context introduces severe security vulnerabilities (command injection, accidental deletion).
- **Process Management**: FFmpeg and video decoding require tight OS process control, thread affinity, and signal handling (for pause/cancel), which Rust and Tokio handle natively and safely.

### 2. Why React Does Not Directly Control FFmpeg
- FFmpeg tasks are resource-intensive, long-running, and can consume gigabytes of memory.
- Running or orchestrating FFmpeg inside the webview JavaScript thread would lead to UI stutter, event loop blocking, and webview crashes.
- Rust owns job orchestration, frame buffer recycling, and progress calculation, streaming lightweight progress events to the React UI.

### 3. Why AI Models Are Abstracted Behind Traits
- AI research moves rapidly (new diffusion backends, ONNX optimizations, transformer architectures).
- Hardcoding a specific neural network architecture creates technical debt.
- Abstracting tasks into traits (`CharacterTransformationEngine`, `TemporalConsistencyEngine`) allows swapping underlying model implementations without modifying application logic or the UI.

### 4. Why Inference Is Separated from the UI
- Neural network inference operates on tensors and frame buffers in GPU VRAM.
- Separating the inference layer ensures that model loading, VRAM allocation, and batch execution are isolated from frontend rendering.
- Enables clear, non-blocking asynchronous state reporting (`QUEUED`, `RUNNING`, `PAUSED`, `FAILED`, `COMPLETED`).

### 5. Why the System Is Designed Local-First with Future Cloud Adapters
- **Privacy & Speed**: Processing video locally protects user media privacy and avoids costly bandwidth transfer of 1080p/4K raw footage.
- **Modularity**: By abstracting `InferenceRuntime` and `ModelProvider`, the system can plug in a Cloud Adapter for users with low-spec hardware without altering any UI components.

---

## Consequences

- Clean separation of concerns with predictable communication via typed Tauri commands and events.
- Zero fake progress or mock hallucination in production paths: missing model files immediately trigger structured `MODEL_NOT_AVAILABLE` responses.
