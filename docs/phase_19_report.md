# Phase 19 Engineering Report: Segmented Cloud Transformation Architecture

**Status:** Completed  
**Zero-Fake Policy:** Strictly Compliant  
**Starting Baseline HEAD:** `3dfb2281c48f9707a91fc995735f9467e0e5ec8a`  
**Implementation Commit:** `da38d3e2cec14221530a65114cda822f82cbff9f`  
**Tests Passing:** 787/787 Rust Tests (`cargo test -- --test-threads=1`), 20/20 Frontend Unit Tests (`vitest run`), 100% Clean Formatting (`cargo fmt --check`), 100% Clean Check (`cargo check --all-targets`), 100% Clean Build (`npm run build`)  
**Live Testing Cost Incurred:** $0.00 (deterministic local FFmpeg fixtures, strict Stage B budget protection, unit tests with zero credit burn)  
**Segment Boundary Visual Quality:** NOT LIVE VERIFIED  
**Preview Runtime Verified:** NO  

---

## 1. Overview & Problem Statement

Phase 18 introduced production cloud generation with single-shot remote predictions. However, long-form videos (>60 seconds for background removal, or higher duration boundaries) were rejected at the routing gate due to provider-enforced hard duration limits (e.g. Bria on Replicate rejects videos >60s).

Phase 19 implements the complete, deterministic **Segmented Cloud Job Pipeline** enabling videos exceeding the provider's single-request duration limit (subject to local disk, supported CFR timing, budget, and orchestration limits) to be segmented, transformed via sequential provider child predictions by default (`max_active_paid_segments = 1`), stitched with alpha channels, and audio-muxed back into a pristine final asset.

---

## 2. Key Architecture & Invariants

1. **Typed Routing Block Codes:** Strongly typed `RoutingBlockCode` enum (`ProviderDurationLimit`, `ProviderResolutionLimit`, `ProviderFpsLimit`, `UnsupportedTask`, `CostBudgetExceeded`) set directly by router logic. No string parsing or regex over decision reasons.
2. **Single Authoritative Probe:** `SourceMediaProbe::probe_file_detailed` performs exactly one `ffprobe` invocation to extract duration, dimensions, framerate, audio presence, `r_frame_rate`, `avg_frame_rate`, `time_base`, and frame counts.
3. **Fail-Closed VFR Policy:** Any input with variable frame rate (`is_vfr == true`, `(fps - avg_fps).abs() > 0.05`) is rejected fail-closed (`UNSUPPORTED_VFR_SEGMENTATION`) to prevent boundary drift and audio/video desynchronization. Normal CFR and fractional CFR (e.g. 30000/1001) are fully accepted.
4. **Frame-Aligned Boundaries & Duration Correction:** Derived from rational/frame math (`< 60.0s`). Local split segments exceeding provider limits are iteratively corrected with deterministic max attempts (3 iterations) or fail closed with `SEGMENT_DURATION_LIMIT_VIOLATION`.
5. **Deterministic Child Client Identity:** Child jobs use canonical client IDs (`segjob:<parentId>:<index>:<configHash>:v1`). Internal job UUIDs are managed independently by the lifecycle service.
6. **Parent Request Idempotency & Conflict:** Deduplication via `client_request_id` + normalized configuration hash. Duplicate start with matching config resumes existing parent; differing config returns `REQUEST_ID_CONFLICT`.
7. **Storage Isolation & Atomic CAS Recovery:** Segmented manifests live strictly under `<project>/cloud-jobs/segmented/<parent-id>/manifest.json` ensuring no interference with `PersistentCloudJobStore::list_jobs_in_project()`. Full 5-case atomic recovery with monotonic `stateRevision`.
8. **Two-Stage Budget Guard:** Stage A preflight provisional estimate; Stage B actual batch base estimate calculated *only after all split segments exist and are probed*. Transitions to `COST_APPROVAL_REQUIRED` before dispatching any child prediction if budget is exceeded.
9. **Budget Approval Resume:** `approve_segmented_budget` command validates increased budget and resumes parent without re-splitting or creating a new parent.
10. **Child Lifecycle Delegation & Crash Recovery:** Children delegate strictly to `CloudJobLifecycleService`. Children created before parent mapping crash are recovered by `find_job_by_client_request_id`.
11. **Strict Retry & Concurrency Guard:** Ambiguous/failed predictions trigger `BLOCKED`/`FAILED` with 0 automatic paid resubmissions. Concurrency is sequential (`max_active_paid_segments = 1`).
12. **Cancellation Semantics:** Cancellation request is persisted first, cancels in-flight child prediction via lifecycle service, and reflects truthful state.
13. **Audio Preservation Policy:** Split child files are video-only (`-an`, `preserve_audio = false`). Final audio is muxed directly from original source into Opus track in final WebM. Sources without audio produce video-only outputs.
14. **Two-Tier Resumption Cache:** Level A (same-parent store resume) and Level B (split segment disk cache with SHA-256 and FFmpeg fingerprinting) enabled. Level C (cross-parent cloud output reuse) is **DISABLED**.
15. **Stitch Compatibility & Alpha Fallback:** Validates WebM, VP9, dimensions, framerate, and alpha before concat. Uses stream-copy concat for matching streams and VP9 alpha (`yuva420p`) re-encode fallback when needed.
16. **Crash After Promotion Recovery:** Promoted final `.webm` detected on startup directly promotes parent manifest from `ValidatingOutput` to `Completed` with 0 re-stitches and 0 provider calls.
17. **Frontend Hydration Race Safety:** Monotonic state revision merging protects UI state if older list hydration arrives after a newer event snapshot.

---

## 3. Test Coverage Matrix

| Invariant | Test Function | Assertions & Specific Verification | Result |
|---|---|---|---|
| **1. Typed Routing Block Code** | `test_phase19_01_typed_routing_block_code` | Asserts `decision.block_code == Some(RoutingBlockCode::ProviderDurationLimit)` on 140s request | **PASSED** |
| **2. Single Source Probe** | `test_phase19_02_probe_detailed_timing_facts` | Asserts single `ffprobe` populates dimensions, duration, audio, rational fps, timebase | **PASSED** |
| **3. VFR Fail-Closed** | `test_phase19_03_vfr_fail_closed` | Asserts VFR timing facts return `Err(UNSUPPORTED_VFR_SEGMENTATION)` | **PASSED** |
| **4. Fractional CFR Acceptance** | `test_phase19_04_fractional_cfr_accepted` | Asserts 30000/1001 fractional CFR is accepted and planned into 2 valid segments | **PASSED** |
| **5. Frame-Aligned Boundaries** | `test_phase19_05_frame_aligned_boundary_calculation` | Asserts 140s / 30fps splits into 3 segments strictly bounded under 60.0s (1400 frames each) | **PASSED** |
| **6. Video-Only Split & Correction** | `test_phase19_06_splitter_video_only_and_duration_correction` | Asserts split segment has no audio (`!has_audio`) and correct dimensions/duration | **PASSED** |
| **7. Duration Correction Exhaustion** | `test_phase19_07_duration_correction_exhaustion_failure` | Asserts `SEGMENT_DURATION_LIMIT_VIOLATION` when provider limit is violated after 3 iterations | **PASSED** |
| **8. Child Identity Determinism** | `test_phase19_08_child_client_identity_determinism` | Asserts `segjob:<parent>:0:<configHash>:v1` format across all planned child segments | **PASSED** |
| **9. Parent Idempotency & Conflict** | `test_phase19_09_parent_request_idempotency_and_conflict` | Asserts duplicate submission resumes parent; modified config returns `REQUEST_ID_CONFLICT` | **PASSED** |
| **10. Storage Isolation** | `test_phase19_10_parent_storage_isolation` | Asserts `PersistentCloudJobStore::list_jobs_in_project` ignores segmented parent directory | **PASSED** |
| **11. Atomic 5-Case Store Recovery** | `test_phase19_11_parent_manifest_persistence_and_atomic_store` | Tests CAS newer tmp wins, stale tmp cleanup, corrupt primary recovery, fail-closed both corrupt | **PASSED** |
| **12. Two-Stage Budget Guard** | `test_phase19_12_two_stage_budget_guard` | Tests Stage A preflight rejection ($0.10) and Stage B actual segment cost approval ($1.00) | **PASSED** |
| **13. Budget Approval Resume** | `test_phase19_13_budget_approval_resume` | Asserts `CostApprovalRequired` transitions to `Ready` on sufficient approval ($2.00) without re-splitting | **PASSED** |
| **14. Crash Before Child Mapping** | `test_phase19_14_child_created_before_parent_mapping_crash` | Asserts existing child job is recovered via `find_job_by_client_request_id` with 0 duplicate submission | **PASSED** |
| **15. Zero Paid Auto-Resubmit** | `test_phase19_15_ambiguous_and_failed_retry_zero_auto_resubmit` | Asserts child failure transitions parent to `Failed` with 0 automatic predictions | **PASSED** |
| **16. Sequential Concurrency = 1** | `test_phase19_16_max_paid_concurrency_sequential` | Verifies sequential loop execution in orchestrator worker dispatch | **PASSED** |
| **17. Cancellation Semantics** | `test_phase19_17_cancellation_semantics` | Asserts cancellation persisted first, child cancelled via lifecycle, parent state is `Cancelled` | **PASSED** |
| **18. Video-Only Child Input** | `test_phase19_18_child_audio_policy_video_only` | Probes split child input file to verify audio track is completely stripped (`-an`) | **PASSED** |
| **19. Final Audio Muxing** | `test_phase19_19_final_original_audio_muxing` | Probes stitched + muxed final WebM to verify Opus audio track from original source | **PASSED** |
| **20. Level B Cache Lifecycle** | `test_phase19_20_level_b_cache_lifecycle` | Tests cache hit on identical SHA/FFmpeg fingerprint and fail-closed cleanup on corrupt file | **PASSED** |
| **21. Level C Cross-Parent Disabled**| `test_phase19_21_level_c_cross_parent_cache_disabled` | Proves Parent B cannot query or reuse Parent A's completed cloud output | **PASSED** |
| **22. Stitch Compatibility Gate** | `test_phase19_22_stitch_compatibility_gate` | Asserts stream-copy validator verifies WebM, VP9, dimensions, and FPS compatibility | **PASSED** |
| **23. VP9 Alpha Fallback** | `test_phase19_23_vp9_alpha_fallback_real_media` | Generates transparent VP9 segments, runs re-encode concat, probes decoded dimensions/duration | **PASSED** |
| **24. Final Stitch Accuracy** | `test_phase19_24_final_stitch_duration_and_timestamp_accuracy` | Stitches 3 synthetic segments (6.0s total) and probes final duration (~6.0s) | **PASSED** |
| **25. Promotion Crash Recovery** | `test_phase19_25_crash_after_final_promotion_recovery` | Asserts worker detects promoted `.webm` on disk and promotes manifest directly to `Completed` | **PASSED** |
| **26. Preview Security Roots** | `test_phase19_26_preview_authorization_security` | Rejects non-completed jobs, rejects path traversal outside root, authorizes valid artifact | **PASSED** |

---

## 4. Full Validation Results

- **Rust Test Suite (`cargo test -- --test-threads=1`):** **787 passed; 0 failed**
- **Frontend Vitest Suite (`npm test -- --run`):** **20 passed; 0 failed** (including stale hydration race test)
- **Formatting (`cargo fmt --check`):** **Clean (0 errors)**
- **Compile Check (`cargo check --all-targets`):** **Clean (0 errors, 0 warnings)**
- **Frontend Production Build (`npm run build`):** **Clean (built in 8.47s)**

---

## 5. Cost & Zero-Fake Compliance

- **Actual Live API Spend:** **$0.00**
- All segmentation planning, FFmpeg splitting, VP9 stitching, and audio muxing tests execute against deterministic local synthetic fixtures without burning cloud credits.
- `ALLOW_PAID_LIVE_TEST` remains disabled.
