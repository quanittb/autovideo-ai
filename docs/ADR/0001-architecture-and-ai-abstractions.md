# ADR 0001: System Architecture, System Boundaries, and AI Engine Abstractions

- **Status**: Approved
- **Date**: 2026-08-15
- **Deciders**: AutoVideo AI Lead Architect

---

## Context

AutoVideo AI is a desktop application designed for automated AI video transformation (e.g. Fox → Rabbit, Winter → Autumn). The system requires video processing, neural model execution, cross-platform stability (macOS and Windows), and an asynchronous job manager.

---

## Decision

1. **System Boundary**: React UI will communicate exclusively with Rust via Tauri 2 commands and async event streams. React will NEVER directly spawn process shells, FFmpeg commands, or Python processes.
2. **AI Abstraction Layer**: All AI capabilities must be defined behind traits in Rust (`AnalysisEngine`, `TransformationEngine`, `CharacterTransformationEngine`, `BackgroundTransformationEngine`, `TemporalConsistencyEngine`, `AudioEngine`, `InferenceRuntime`, `ModelProvider`, `ModelManager`).
3. **Strict Non-Faking Enforcement**: Mocks/fixtures are restricted to dev/demo modes and must be explicitly tagged as `MOCK` / `FIXTURE`. In real pipeline modes, if model weights or GPU runtimes are unavailable, the system must immediately return `MODEL_NOT_AVAILABLE` or `RUNTIME_NOT_AVAILABLE`.
4. **Job Queue Engine**: Long-running jobs run in background Tokio tasks, updating state (`QUEUED`, `RUNNING`, `PAUSED`, `CANCELLING`, `CANCELLED`, `FAILED`, `COMPLETED`) and pushing progress via Tauri events.

---

## Consequences

- Clean separation between presentation UI and Rust media/AI logic.
- AI models can be upgraded, replaced, or swapped with cloud adapters without modifying frontend React code.
- User experience is transparent regarding AI model availability.
