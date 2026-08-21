# Phase 17 Engineering Report: Replicate BRIA Video Background Removal Provider & Alpha Transparent WebM/VP9 Pipeline

## 1. Executive Summary & Objectives
In Phase 17, we integrated the official **Replicate BRIA Video Background Removal (`bria/video-remove-background`)** provider to deliver specialized video-to-video background removal with alpha transparency. Following an independent post-acceptance audit, all architectural invariants were verified and hardened.

The integration adheres strictly to the project's zero-fake, cost-guarded, fail-closed architecture, enforcing truthful capabilities, strict input/output contracts, format-aware storage, two-stage transparent WebM (VP9 + Alpha) decodability verification, normalized configuration hashing, and single-probe execution.

- **Starting Base / Remote HEAD**: `0925d7a91298f717b8dbb6b7ec6b20e9e2553217`
- **Initial Implementation SHA**: `92771337408446bf4610fcb0650550ee271292f7`
- **Post-Acceptance Invariant Fix SHA**: `1d69bd9161921458de9b34a34095c12002196e6a`
- **Zero-Live-Cost Policy**: Fully observed ($0.00 live cost, 0 real predictions, 0 real uploads)
- **Live Quality Verification**: `LIVE QUALITY VERIFIED: NO` (Edge matting fidelity, hair detail, and temporal consistency are evaluated under Phase 20)

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
                                  |  (PROBED ONCE PER SUBMISSION)|
                                  |   -> SourceMediaFacts        |
                                  +--------------+---------------+
                                                 |
                                                 v
                                  +------------------------------+
                                  | GenerationRouter (I/O-Free)  |
                                  | (TaskClass::BackgroundRemoval|
                                  |  Authoritative Rate: $0.0042 |
                                  |  Max duration: 60s           |
                                  |  PreserveSource Resolution)  |
                                  +--------------+---------------+
                                                 |
                                                 v
                                  +------------------------------+
                                  |   ValidatedSubmissionPlan    |
                                  |   (carries source_facts)     |
                                  +--------------+---------------+
                                                 |
                                                 v
                     +---------------------------+---------------------------+
                     |                                                       |
                     v                                                       v
      +------------------------------+                       +------------------------------+
      | BackgroundRemovalSpec        |                       | ReplicateBriaBgRemovalProvider|
      | - Consumes plan.source_facts |                       | - Transparent mode           |
      | - 0 reference images strictly|                       | - webm_vp9 format            |
      | - Transparent background     |                       | - SSRF-guarded download      |
      | - WebM VP9 format            |                       | - Live Execution Guard       |
      | - preserve_audio flag        |                       | - Fails closed on raw cost   |
      +------------------------------+                       +------------------------------+
                     |                                                       |
                     +---------------------------+---------------------------+
                                                 |
                                                 v
                                  +------------------------------+
                                  |  Normalized Config Hash      |
                                  |  - Canonical task identity   |
                                  |  - Probed facts & SHA256     |
                                  |  - No fake/stale IPC fields  |
                                  +--------------+---------------+
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

### Key Architectural Invariants Verified & Hardened:
1. **Single Source Probe Invariant**:
   - `validate_and_prepare_cloud_submission` executes `SourceMediaProbe::probe_file` ONCE during preflight validation and attaches the resulting `SourceMediaFacts` directly to `ValidatedSubmissionPlan`.
   - `BackgroundRemovalSpec::build` consumes `plan.source_facts` directly, preventing redundant `ffprobe` subprocess executions.
2. **Normalized Configuration Hash Invariant**:
   - Background removal configuration identity is computed deterministically from:
     - Canonical task (`TASK:BACKGROUND_REMOVAL`)
     - Provider key (`replicate` / `bria/video-remove-background`)
     - Background mode (`Transparent`)
     - Output format (`WebM_VP9`)
     - `preserve_audio` boolean flag
     - Source media content SHA256
     - Probed authoritative `SourceMediaFacts` (duration, fps, width, height)
   - Configuration hashing strictly ignores raw/stale user IPC parameters (`request.duration_seconds`, `request.fps`, `request.resolution`), uploaded signed URIs, remote job IDs, and timestamps.
3. **Single Authoritative Pricing Authority**:
   - The single executable source of truth for BRIA pricing ($0.0042/s) is the `ProviderRegistry` record.
   - `ReplicateBriaBgRemovalProvider::capabilities()` sets `estimated_cost_per_second: None` to avoid duplicate billing authorities.
   - `ReplicateBriaBgRemovalProvider::estimate_cost` fails closed (`status: CostConfidence::Unknown`, `estimated_usd: None`) when called on raw requests lacking probed facts.
4. **Two-Step Alpha Transparency Validation**:
   - `Stage A`: Stream probe verifies `pix_fmt` and container tags (`TAG:alpha_mode=1`).
   - `Stage B`: Deterministic decode verification via `ffmpeg -c:v libvpx-vp9 -i <file> -vframes 1 -filter_complex "[0:v]alphaextract[a]" -map "[a]" -f null -`.
5. **Container-Safe Artifact Paths (Schema v2)**:
   - `ArtifactDescriptor` encapsulates `ArtifactContainer` (`Mp4`, `Webm`), `ArtifactVideoCodec` (`H264`, `Vp9`), `require_alpha`, and `require_audio`.
   - Artifact paths derive extensions directly from container descriptors (`.webm` vs `.mp4`).

---

## 3. Changes Made & Files Modified

### Backend Core (`src-tauri`):
1. **`src-tauri/src/ai/cloud/submission.rs`**:
   - Added `source_facts: Option<SourceMediaFacts>` to `ValidatedSubmissionPlan`.
   - Returned probed facts from `validate_and_prepare_cloud_submission`.
2. **`src-tauri/src/ai/cloud/spec.rs`**:
   - Updated `BackgroundRemovalSpec::build` to consume `plan.source_facts` without re-probing.
   - Added `BackgroundRemovalSpec::build_with_facts`.
3. **`src-tauri/src/ai/cloud/lifecycle.rs`**:
   - Implemented `compute_inputs_and_configuration_hash` with task-specific normalized hashing for `BackgroundRemoval`.
4. **`src-tauri/src/ai/cloud/providers/replicate_bria.rs`**:
   - Set `capabilities.estimated_cost_per_second: None`.
   - Set `estimate_cost` to return `CostConfidence::Unknown` / `estimated_usd: None` (fail-closed for raw requests).
5. **`src-tauri/src/ai/cloud/registry.rs`**:
   - Set `capabilities.estimated_cost_per_second: None` in BRIA provider record, maintaining `pricing_amount: Some(0.0042)` as single executable price source.
6. **`src-tauri/src/ai/tests_phase17.rs`**:
   - Added 5 new invariant test suites:
     - `test_phase17_51`: Single probe execution & plan facts reuse in spec build.
     - `test_phase17_52`: Normalized config hash invariance across raw IPC duration/fps/resolution changes.
     - `test_phase17_53`: Config hash sensitivity to `preserve_audio` toggling.
     - `test_phase17_54`: Config hash sensitivity to provider/model key changes.
     - `test_phase17_55`: Config hash sensitivity to source video content changes.
   - Updated test assertions for single pricing source and fail-closed raw cost estimation.
7. **`src-tauri/src/ai/tests_phase15.rs` & `src-tauri/src/ai/tests_phase16.rs`**:
   - Updated `ValidatedSubmissionPlan` test initializers to include `source_facts: None`.
8. **`docs/phase_17_background_removal_benchmark.md`**:
   - Clarified latency target (<3.0s/s) is an internal acceptance target.
   - Clarified billing data recording vs uninferred cost.
   - Clarified audio stream preservation tolerance and codec informational status.

---

## 4. Test Verification & Actual Results

### Quality Checks Executed:
1. **Rust Formatting**: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
   - Result: **Passed with code 0** (0 formatting diffs).
2. **Rust Compilation**: `cargo check --all-targets --manifest-path src-tauri/Cargo.toml`
   - Result: **Passed with 0 errors, 0 warnings**.
3. **Phase 17 Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase17 --test-threads=1`
   - Result: **55 passed; 0 failed; 0 ignored; finished in 1.66s**.
4. **Phase 16 Test Suite Regression Check**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase16 --test-threads=1`
   - Result: **39 passed; 0 failed; 0 ignored; finished in 6.70s**.
5. **Phase 15 Test Suite Regression Check**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase15 --test-threads=1`
   - Result: **38 passed; 0 failed; 0 ignored; finished in 14.34s**.
6. **Phase 14 Test Suite Regression Check**: `cargo test --manifest-path src-tauri/Cargo.toml -- test_phase14 --test-threads=1`
   - Result: **10 passed; 0 failed; 0 ignored; finished in 1.05s**.
7. **Cloud MVP Test Suite Check**: `cargo test --manifest-path src-tauri/Cargo.toml -- test_cloud --test-threads=1`
   - Result: **6 passed; 0 failed; 0 ignored; finished in 0.00s**.
8. **Full Rust Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1`
   - Result: **747 passed; 0 failed; 0 ignored; 0 measured; finished in 119.41s**.
9. **Frontend Compilation**: `npm.cmd run build`
   - Result: **Built in 23.46s (0 errors)**.

### Synthetic Media Acceptance Results:
- **Transparent Fixture** (`transparent_vp9.webm`):
  - FFprobe: `pix_fmt=yuv420p`, `TAG:alpha_mode=1`.
  - Production `CloudOutputValidator`: **PASSED** (Decodable alpha confirmed, metadata extracted: 64x64, 1.0s duration).
  - FFmpeg alpha decode command exit code: `0` (clean exit).
- **Opaque Fixture** (`opaque_vp9.webm`):
  - FFprobe: `pix_fmt=yuv420p`, no alpha tag.
  - Production `CloudOutputValidator`: **FAILED as expected** with `CLOUD_OUTPUT_INVALID: Output lacks decodable alpha transparency`.

---

## 5. Live Test Cost Incurred & Policy Compliance
- **Real Replicate Predictions**: 0
- **Real Replicate Uploads**: 0
- **Paid API Cost Incurred**: $0.00 USD
- **LIVE QUALITY VERIFIED**: NO (Visual quality rubric for boundary hair matting, fine details, and temporal stability is deferred to Phase 20).

---

## 6. Remaining Limitations & Next Steps
1. **Local Utility Removal Fallback**: Cloud background removal currently relies on Replicate BRIA. Local deterministic background removal (e.g. Rembg / RobustVideoMatting) can be integrated as an offline fallback.
2. **Extended Video Durations (>60s)**: Videos exceeding 60 seconds fail closed with `PROVIDER_DURATION_LIMIT`. Chunking and stitching can be explored in future pipelines if extended background removal is required.
3. **Phase Scope**: Complete. DO NOT BEGIN PHASE 18.
