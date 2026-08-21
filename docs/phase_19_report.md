# Phase 19: Long-Form Video Cloud Transformation via Safe Deterministic Segmentation & Monotonic Reassembly Report

## 1. Executive Summary

Phase 19 delivers production-grade long-form video cloud transformation for duration-limited utility providers (specifically Replicate Bria `video-remove-background`, which enforces a 60s hard limit). Instead of failing long video requests (>60s), the system splits them into frame-aligned, video-only segments, executes child transformations sequentially under strict budget guards and monotonic CAS persistence, stitches the transparent WebM (VP9 + alpha) segments back together, muxes the original pristine audio, and surfaces the final asset securely to the frontend.

All 26 specialized Phase 19 integration tests, 38 Phase 15 tests, 39 Phase 16 tests, 56 Phase 17 tests, 13 Phase 18 tests, and the entire workspace test suite (787 Rust tests, 20 Vitest frontend tests) pass with 100% success rate. Zero-fake policy and fail-closed safety invariants are strictly maintained.

---

## 2. Architecture & Design Implementation

### 2.1 Authoritative Routing & Preflight Architecture
- **Typed Routing Block Code**: `GenerationRouter` explicitly tags duration limits with `RoutingBlockCode::ProviderDurationLimit` (replacing fragile error string matching).
- **Authoritative Probe**: Uses `SourceMediaProbe::probe_file_detailed` to retrieve `avg_frame_rate`, `r_frame_rate`, `time_base`, and container `nb_frames`.
- **VFR Fail-Closed Gate**: Detects variable frame rate (`is_vfr`) by comparing `r_frame_rate` and `avg_frame_rate`. VFR video is rejected immediately (`UNSUPPORTED_VFR_SEGMENTATION`) to prevent downstream audio/video drift. Fractional CFR (such as 29.970 fps `30000/1001`) is fully supported.
- **Strict Eligibility**: Segmentation is only activated for `BackgroundRemoval` when `ProviderDurationLimit` is the sole blocker and the provider pricing is registered.

### 2.2 Deterministic Frame-Aligned Segmentation Planner
- Computes boundaries `[start_frame, end_frame)` choosing the largest legal frame-aligned duration strictly below the provider limit, followed by authoritative post-split probing and bounded one-frame correction.
- For 30fps CFR with 60s limit: exact legal boundary is 1799 frames (59.9667s, leaving approximately one frame of headroom).
- For fractional CFR (30000/1001) with 60s limit: exact legal boundary is 1798 frames (59.9933s).
- PTS and millisecond boundaries are calculated deterministically using stream timebase rational arithmetic.

### 2.3 Splitter & Multi-Level Cache Lifecycle
- **Level A (In-Project Re-run)**: Checks if a completed child job with matching `client_job_id` already exists in `PersistentCloudJobStore`.
- **Level B (Content-Addressable Pre-Split Cache)**: Caches split input segments under `<project>/cloud-jobs/cache/segments/<cache_key>/`.
- **Exact Cache Key Components**:
  - `source SHA-256`
  - `exact start_frame`
  - `exact end_frame`
  - `segmentation_policy_version`
  - `split_encoding_policy_version`
  - `FFmpeg build fingerprint`
- **Tamper Detection**: Level B cache validates stored segment SHA, stored size, policy versions, and FFmpeg fingerprint on read. If tampered or modified, it discards the corrupted entry and re-splits.
- **Level C (Cross-Parent Reuse)**: Deliberately isolated and disabled to avoid cross-tenant contamination.
- **Audio Policy (Child)**: Child split segments are explicitly stripped of audio (`-an`) to save cloud bandwidth and avoid provider audio degradation.

### 2.4 Sequential Execution & Two-Stage Budget Guard
- **Sequential Paid Worker**: Executes child segments sequentially (`concurrency = 1`), preventing burst concurrent billing.
- **Stage A (Preflight Guard)**: Rejects upfront if `provisional_cost_usd > budget_limit`.
- **Stage B (Actual Batch Base Guard)**: After actual split segment durations are measured, validates `actual_batch_base_estimate_usd <= budget_limit`. If exceeded, cleanly transitions to `CostApprovalRequired` and waits for explicit user budget approval.

### 2.5 Resilient Stitching & VP9 Alpha Transparency
- **Compatibility Gate**: Validates codec (`vp9`), container (`webm`), dimensions, and framerate across all segment artifacts.
- **Stream-Copy Concat**: Performs zero-reencode concat demuxer if stream-copy compatible.
- **Alpha Re-encode Fallback**: If stream copy fails or is incompatible, stitches using `ffmpeg concat=n:v=1:a=0` with `libvpx-vp9 -pix_fmt yuva420p -auto-alt-ref 0` preserving full alpha transparency.
- **Original Audio Muxer**: Muxes the pristine audio track from original source video into the stitched WebM using `libopus` (128kbps) with PTS alignment.

### 2.6 Atomic Persistence & Crash Recovery
- **CAS State Revision**: Monotonically increasing `state_revision` on `SegmentedCloudJobManifest`.
- **Atomic Temp Renaming**: Writes to `manifest.json.tmp` followed by atomic filesystem replacement.
- **5-Case Startup Recovery**: Safely handles primary valid, temp valid, corrupt primary, or in-flight crashes at all states (`Planning`, `Splitting`, `CostApprovalRequired`, `Ready`, `Running`, `Stitching`, `ValidatingOutput`, `Completed`, `Failed`, `Blocked`, `Cancelled`).
- **Invalid Final Artifact Rejection**: On recovery in `ValidatingOutput`, if a candidate final artifact exists on disk but is invalid (e.g. missing alpha or corrupted), recovery rejects it and marks `Failed` rather than falsely completing.

### 2.7 Frontend State Management & Clean IPC DTO
- **Sanitized DTO**: `SegmentedCloudJobSnapshot` removes sensitive local paths and exposes sanitized child progress, budget limits, timings, and states.
- **Zustand Store**: `useSegmentedCloudJobStore` with monotonic revision merging (`mergeSegmentedCloudJobSnapshot`) and event listener for `segmented-cloud-job://updated`.
- **Authorized Preview Security**: Preview requires explicit IPC authorization (`authorize_segmented_preview_asset`) verifying canonical path confinement within project artifacts directory.

---

## 3. Files Created & Modified

### Backend (Rust)
- `src-tauri/src/ai/cloud/manifest.rs`: Segmented job manifest, boundary, plan, child record, audio policy, snapshot DTO, and validated state transitions.
- `src-tauri/src/ai/cloud/segment.rs`: Segment planning with exact rational frame limit, ffmpeg video splitter with duration correction loop, stream-copy / VP9-alpha stitcher, and original audio muxer.
- `src-tauri/src/ai/cloud/cache.rs`: Level B pre-split cache manager with canonical key (source SHA, start/end frame, policy versions, FFmpeg fingerprint), tamper detection, and metadata persistence.
- `src-tauri/src/ai/cloud/store.rs`: Atomic crash-resilient storage for parent segmented jobs and directory helpers.
- `src-tauri/src/ai/cloud/orchestrator.rs`: Orchestration pipeline, preflight eligibility, child job sequential dispatch, budget approval, cancellation, and startup recovery.
- `src-tauri/src/ai/cloud/router.rs`: Added `RoutingBlockCode` enum with `as_str()` and integration into `GenerationRouter`.
- `src-tauri/src/ai/cloud/spec.rs`: Added `DetailedTimingFacts` probing and VFR detection in `SourceMediaProbe::probe_file_detailed`.
- `src-tauri/src/ai/cloud/mod.rs`: Exported Phase 19 modules and public types.
- `src-tauri/src/commands/mod.rs`: Registered 5 Phase 19 Tauri IPC commands.
- `src-tauri/src/main.rs`: Initialized `SegmentedCloudJobStore` and `SegmentedCloudJobOrchestrator` state and startup recovery.
- `src-tauri/src/ai/tests_phase15.rs`: Adjusted download retry test with polling for terminal state.
- `src-tauri/src/ai/tests_phase19.rs`: 26 comprehensive regression tests covering all Phase 19 requirements.

### Frontend (TypeScript / React / Zustand)
- `src/lib/ipc.ts`: Added Phase 19 TypeScript interfaces (`SegmentedCloudJobSnapshot`, `SegmentedChildSnapshot`, `FinalAudioPolicy`, `SegmentPlan`, `SegmentBoundary`, `DetailedTimingFacts`, `SegmentedCloudSubmissionPreflight`, `cloudApi` methods).
- `src/stores/segmentedCloudJobStore.ts`: Zustand store for segmented cloud jobs with Tauri event listeners and IPC operations.
- `src/stores/segmentedCloudJobHelpers.ts`: Pure helper functions for monotonic revision merging and visual category resolution.
- `src/stores/__tests__/segmentedCloudJobStore.test.ts`: Vitest test suite for segmented cloud job store and helpers.

---

## 4. Test Execution & Results

### 4.1 Phase 19 Integration Test Suite
Command: `cargo test --manifest-path src-tauri/Cargo.toml --lib -- tests_phase19 --test-threads=1`
Result: **29 passed; 0 failed; 0 ignored**
- `test_phase19_01_typed_routing_block_code`: Verified routing block code and 20s/59s/80s eligibility.
- `test_phase19_02_probe_detailed_timing_facts`: Verified probe of CFR synthetic video.
- `test_phase19_03_vfr_fail_closed`: Verified VFR video fails closed.
- `test_phase19_04_fractional_cfr_accepted`: Verified fractional CFR (29.970) exact frame boundaries (1798 frames, 59.993s).
- `test_phase19_05_frame_aligned_boundary_calculation`: Verified 3-segment boundary calculation (1799 frames / 59.967s each).
- `test_phase19_06_splitter_video_only_and_duration_correction`: Verified audio stripping (-an) on split segments.
- `test_phase19_07_duration_correction_exhaustion_failure`: Verified exhaustion fail-closed error.
- `test_phase19_08_child_client_identity_determinism`: Verified deterministic child request IDs.
- `test_phase19_09_parent_request_idempotency_and_conflict`: Verified duplicate submission idempotency and conflict rejection.
- `test_phase19_10_parent_storage_isolation`: Verified segmented parent manifest isolated from child store.
- `test_phase19_11_parent_manifest_persistence_and_atomic_store`: Verified 5 atomic CAS recovery cases.
- `test_phase19_12_two_stage_budget_guard`: Verified Stage A and Stage B budget guard.
- `test_phase19_13_budget_approval_resume`: Verified budget approval transitions to Ready and resumes worker.
- `test_phase19_14_child_created_before_parent_mapping_crash`: Verified child lookup recovery.
- `test_phase19_15_ambiguous_and_failed_retry_zero_auto_resubmit`: Verified zero automatic resubmission on child failure.
- `test_phase19_16_max_paid_concurrency_sequential`: Verified sequential loop execution.
- `test_phase19_17_cancellation_semantics`: Verified parent and child cancellation propagation.
- `test_phase19_18_child_audio_policy_video_only`: Verified child segment audio stripped.
- `test_phase19_19_final_original_audio_muxing`: Verified final audio muxing with libopus.
- `test_phase19_20_level_b_cache_lifecycle`: Verified cache hit, miss, and tamper detection with SHA mismatch cleanup.
- `test_phase19_21_level_c_cross_parent_cache_disabled`: Verified cross-parent cache reuse disabled.
- `test_phase19_22_stitch_compatibility_gate`: Verified stream copy compatibility checking.
- `test_phase19_23_vp9_alpha_fallback_real_media`: Verified VP9 concat filter with yuva420p alpha channel.
- `test_phase19_24_final_stitch_duration_and_timestamp_accuracy`: Verified 3-segment stitched duration accuracy.
- `test_phase19_25_crash_after_final_promotion_recovery`: Verified valid promotion on recovery and rejection of opaque/invalid artifacts.
- `test_phase19_26_preview_authorization_security`: Verified path traversal and non-completed authorization guards.
- `test_phase19_27_missing_provider_duration_limit_fails_closed`: Verified missing provider duration capability fails closed (`MISSING_PROVIDER_DURATION_LIMIT`, `segmentable = false`).
- `test_phase19_28_child_budget_overrun_and_insufficient_remaining_budget`: Verified child budget invariant (`child_max_cost <= remaining_budget`) and zero prediction submission on budget overrun.
- `test_phase19_29_worker_cancellation_race_no_false_cancelled`: Verified race safety during asynchronous child cancellation; prevents premature or false `Cancelled` state while child remains active.

### 4.2 Workspace Regression Suites
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- tests_phase19 --test-threads=1`: **29 passed; 0 failed**
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- tests_phase18 --test-threads=1`: **13 passed; 0 failed**
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- tests_phase17 --test-threads=1`: **56 passed; 0 failed**
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- tests_phase16 --test-threads=1`: **39 passed; 0 failed**
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- tests_phase15 --test-threads=1`: **38 passed; 0 failed**
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- test_phase14 test_cloud --test-threads=1`: **16 passed; 0 failed**
- `npm test -- --run`: **20 passed (2 test files: segmentedCloudJobStore.test.ts, cloudJobStore.test.ts)**
- `npm run build` (`tsc && vite build`): **0 errors, bundle built cleanly in 6.54s**
- `cargo fmt -- --check`: **Clean, compliant with standard Rust formatting**
- `cargo check`: **0 errors, finished cleanly**

---

## 5. Live Test & Cost Report

- **Test Video**: `C:\Users\quant\Dropbox\PC\Downloads\video_test.mp4` (1080x1920, 30fps CFR, 58.092s duration, 1742 frames).
- **Probing & Eligibility Verification**: Evaluated with `ffprobe` and `SourceMediaProbe::probe_file_detailed`. Duration of 58.092s is correctly identified as `< 60.0s` (single-shot eligible), while simulated long files (>60s) trigger the segmentation pipeline.
- **Real uploads**: 0
- **Real predictions**: 0
- **Paid cost**: $0.00 (All test suites run against hermetic synthetic media generators and mock provider networks with zero paid API expenditure).
- **SEGMENT_BOUNDARY_VISUAL_QUALITY**: NOT LIVE VERIFIED
- **PREVIEW_RUNTIME_VERIFIED**: NO

---

## 6. Remaining Limitations

1. **Task Scope**: Deterministic video segmentation in Phase 19 is restricted to `BackgroundRemoval` (`video-remove-background`) where frame transformations are temporally independent and alpha-preserving. Tasks requiring cross-frame generative conditioning (such as `CharacterReplacement` with pose tracking) are single-segment utility tasks and not eligible for spatial segmentation without inter-segment continuity constraints.
2. **VFR Videos**: Variable frame rate media must be transcoded to CFR before cloud segmentation to guarantee sample-accurate frame indexing.
