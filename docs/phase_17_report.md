# Phase 17 Engineering Report: Replicate BRIA Video Background Removal Provider & Alpha Transparent WebM/VP9 Pipeline

## 1. Executive Summary & Objectives
In Phase 17, we integrated the official **Replicate BRIA Video Background Removal (`bria/video-remove-background`)** provider to deliver specialized video-to-video background removal with alpha transparency. The integration adheres strictly to the project's zero-fake, cost-guarded, fail-closed architecture, enforcing truthful capabilities, strict input/output contracts, format-aware storage, two-stage transparent WebM (VP9 + Alpha) decodability verification, and I/O-free routing.

---

## 2. Architecture & Design

```
                                  +------------------------------+
                                  | CloudJobRequest / Submissions|
                                  +--------------+---------------+
                                                 |
                                                 v
                                  +------------------------------+
                                  |  SourceMediaProbe (ffprobe)  |
                                  | (duration, width, height, fps|
                                  |   has_audio extraction)      |
                                  +--------------+---------------+
                                                 |
                                                 v
                                  +------------------------------+
                                  | GenerationRouter (I/O-Free)  |
                                  | (TaskClass::BackgroundRemoval|
                                  |  Pricing: $0.0042/sec        |
                                  |  Max duration: 60s           |
                                  |  PreserveSource Resolution)  |
                                  +--------------+---------------+
                                                 |
                                                 v
                     +---------------------------+---------------------------+
                     |                                                       |
                     v                                                       v
      +------------------------------+                       +------------------------------+
      | BackgroundRemovalSpec        |                       | ReplicateBriaBgRemovalProvider|
      | - 0 reference images strictly|                       | - Transparent mode           |
      | - Transparent background     |                       | - webm_vp9 format            |
      | - WebM VP9 format            |                       | - SSRF-guarded download      |
      | - preserve_audio flag        |                       | - Live Execution Guard       |
      +------------------------------+                       +------------------------------+
                     |                                                       |
                     +---------------------------+---------------------------+
                                                 |
                                                 v
                                  +------------------------------+
                                  |  CloudOutputValidator        |
                                  |  (Two-Step Alpha Validation) |
                                  |  - Stage A: stream probe     |
                                  |  - Stage B: ffmpeg alphaextract|
                                  +--------------+---------------+
                                                 |
                                                 v
                                  +------------------------------+
                                  | Format-Aware Storage & Store |
                                  | - .webm artifact extension   |
                                  | - Atomic promotion from .part|
                                  +------------------------------+
```

### Key Architectural Invariants Verified:
1. **I/O-Free Router (`GenerationRouter`)**:
   - `GenerationRouter` contains zero file I/O, `ffprobe`, `ffmpeg`, or `Command::new` invocations.
   - `SourceMediaProbe` executes in the submission/preflight layer prior to router invocation and passes `SourceMediaFacts` into `GenerationRouter::route_with_facts`.
2. **Probed Source Media Facts as Monetary & Limit Authority**:
   - Background removal enforces provider limits (60s duration limit, 16000x16000 resolution limit) based strictly on probed `SourceMediaFacts` (duration, width, height, FPS, audio stream presence). Client request values cannot bypass actual file facts.
3. **Container-Safe Artifact Paths (Schema v2)**:
   - `ArtifactDescriptor` encapsulates `ArtifactContainer` (`Mp4`, `Webm`), `ArtifactVideoCodec` (`H264`, `Vp9`), `require_alpha`, and `require_audio`.
   - `PersistentCloudJobStore::artifact_final_path_for_container` and `artifact_final_path_for_job` derive file extensions directly from `ArtifactContainer`, preventing arbitrary extension mismatches.
4. **Schema v1 Audio Migration Safety**:
   - Legacy Schema v1 jobs migrate `require_audio` directly from `validation_policy.require_audio`, rather than defaulting globally to `true`.
5. **Pricing Single Source of Truth**:
   - The authoritative pricing rate ($0.0042/s = $4.20 per 1,000s) is defined once in `ProviderRegistry`. `ReplicateBriaBgRemovalProvider::estimate_cost` dynamically queries the registry record.
6. **Two-Step Alpha Transparency Validation**:
   - `Stage A`: Stream probe inspects `pix_fmt` and container tags (`TAG:alpha_mode=1`).
   - `Stage B`: Deterministic decode verification via `ffmpeg -c:v libvpx-vp9 -i <file> -vframes 1 -filter_complex "[0:v]alphaextract[a]" -map "[a]" -f null -`.

---

## 3. Changes Made & Files Modified

### Backend Core (`src-tauri`):
1. **`src-tauri/src/ai/cloud/providers/replicate_bria.rs`** [NEW]:
   - Complete `ReplicateBriaBgRemovalProvider` implementation with cost estimation ($0.0042/s dynamically derived from registry), prediction creation, status polling, cancellation, SSRF validation, and download logic.
2. **`src-tauri/src/ai/cloud/providers/mod.rs`**:
   - Exported `replicate_bria` and `ReplicateBriaBgRemovalProvider`.
3. **`src-tauri/src/ai/cloud/job.rs`**:
   - Added `ArtifactContainer`, `ArtifactVideoCodec`, `ArtifactDescriptor`.
   - Extended `ValidationPolicy` with `expected_width`, `expected_height`, `expected_fps`, `require_alpha`, `expected_container`, `expected_video_codec`.
   - Updated `PersistentCloudJob` with `artifact_descriptor: Option<ArtifactDescriptor>` and Schema v1 `normalize_in_memory` migration.
4. **`src-tauri/src/ai/cloud/spec.rs`**:
   - Added `SourceMediaFacts` and `SourceMediaProbe::probe_file`.
   - Added `BackgroundMode`, `BackgroundRemovalOutputFormat`, and `BackgroundRemovalSpec`.
   - Updated `PreparedProviderSubmission` enum (`CharacterReplacement` vs `BackgroundRemoval`).
   - Added `ProviderTaskSpec` enum with polymorphic build dispatch.
5. **`src-tauri/src/ai/cloud/registry.rs`**:
   - Added `ResolutionPolicy` (`ExplicitTiered` vs `PreserveSource`).
   - Registered BRIA Provider Record (`supports_video_background_removal: true`, rate: $0.0042/s, max duration: 60s).
6. **`src-tauri/src/ai/cloud/router.rs`**:
   - Implemented `route_with_facts` supporting `TaskClass::BackgroundRemoval`.
   - Upgraded `check_resolution_supported` to evaluate `ResolutionPolicy`.
   - Enforced 60-second duration cutoff for background removal.
7. **`src-tauri/src/ai/cloud/submission.rs`**:
   - Implemented reference image rejection for background removal.
   - Added automatic source probing before routing.
8. **`src-tauri/src/ai/cloud/store.rs`**:
   - Replaced arbitrary extension paths with container-safe `artifact_final_path_for_container` and `artifact_final_path_for_job`.
9. **`src-tauri/src/ai/cloud/validator.rs`**:
   - Implemented `validate_artifact_with_policy` with two-stage alpha transparency verification (stream tag / format inspection + `libvpx-vp9` `alphaextract` decoding).
10. **`src-tauri/src/ai/cloud/resolver.rs`**:
    - Registered BRIA provider resolution for `("replicate", "bria/video-remove-background")`.
11. **`src-tauri/src/ai/cloud/lifecycle.rs`**:
    - Wired polymorphic `ProviderTaskSpec` and `PreparedProviderSubmission` execution.
12. **`src-tauri/src/ai/cloud/mod.rs`**:
    - Exported all new types and modules.
13. **`src-tauri/src/media/mod.rs`**:
    - Added `.webm` format support and `.partial` container autodetection from ffprobe `format_name`.
14. **`src-tauri/src/ai/tests_phase17.rs`** [NEW]:
    - 50 comprehensive unit and integration tests for Phase 17.
15. **`src-tauri/src/ai/tests_phase16.rs`**:
    - Updated to new Schema v2 types and polymorphic submission specifications.
16. **`src-tauri/src/ai/tests_cloud_mvp.rs`**:
    - Updated Phase 14 submission guard test to assert background removal input validation in Phase 17.
17. **`docs/phase_17_background_removal_benchmark.md`** [NEW]:
    - Benchmark evaluation protocol for video background removal quality, matting fidelity, and temporal stability.

---

## 4. Test Verification & Actual Results

### Quality Checks Executed:
1. **Rust Formatting**: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
   - Result: **Passed with code 0** (0 formatting diffs).
2. **Rust Compilation**: `cargo check --all-targets --manifest-path src-tauri/Cargo.toml`
   - Result: **Passed with 0 errors, 0 warnings**.
3. **Phase 17 Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase17 --test-threads=1`
   - Result: **50 passed; 0 failed; 0 ignored; finished in 0.56s**.
4. **Phase 16 Test Suite Regression Check**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase16 --test-threads=1`
   - Result: **39 passed; 0 failed; 0 ignored; finished in 6.70s**.
5. **Phase 15 Test Suite Regression Check**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase15 --test-threads=1`
   - Result: **38 passed; 0 failed; 0 ignored; finished in 10.31s**.
6. **Phase 14 Test Suite Regression Check**: `cargo test --manifest-path src-tauri/Cargo.toml -- test_phase14 --test-threads=1`
   - Result: **10 passed; 0 failed; 0 ignored; finished in 1.09s**.
7. **Cloud MVP Test Suite Check**: `cargo test --manifest-path src-tauri/Cargo.toml -- test_cloud --test-threads=1`
   - Result: **6 passed; 0 failed; 0 ignored; finished in 0.01s**.
8. **Frontend Compilation**: `npm.cmd run build`
   - Result: **Built in 10.95s (0 errors)**.

### Synthetic Media Acceptance Results:
- **Transparent Fixture** (`transparent_vp9.webm`):
  - FFprobe: `pix_fmt=yuv420p`, `TAG:alpha_mode=1`.
  - Production `CloudOutputValidator`: **PASSED** (Decodable alpha confirmed, metadata extracted: 64x64, 1.0s duration).
  - FFmpeg alpha decode command exit code: `0` (clean exit, empty stderr).
- **Opaque Fixture** (`opaque_vp9.webm`):
  - FFprobe: `pix_fmt=yuv420p`, no alpha tag.
  - Production `CloudOutputValidator`: **FAILED as expected** with `CLOUD_OUTPUT_INVALID: Output lacks decodable alpha transparency`.

---

## 5. Live Test Cost Incurred
- **Live Tests Incurred**: $0.00 USD (All automated tests executed with live guard disabled / mock HTTP policies in compliance with the Zero-Fake and Zero-Unintended-Cost rules).

---

## 6. Remaining Limitations & Next Steps
1. **Local Utility Removal Alternative**: Currently, cloud background removal is served exclusively by Replicate BRIA. Local deterministic background removal (e.g. Rembg / RobustVideoMatting) can be integrated as an offline fallback in subsequent phases.
2. **Extended Video Durations (>60s)**: For videos longer than 60 seconds, the router fails closed with `PROVIDER_DURATION_LIMIT`. Chunk-and-stitch segmentation for background removal can be added if long-form background removal is required.
