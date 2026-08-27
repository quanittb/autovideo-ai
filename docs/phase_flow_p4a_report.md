# Phase FLOW-P4-A Report: Long Video Multi-Segment Production Architecture

## 1. Executive Summary & Phase Rule Compliance

- **Phase Objective**: Establish the deterministic multi-segment production architecture for videos exceeding Google Flow's 10-second per-generation limit, ensuring frame-accurate rational segmentation, timeline normalization, full-length original audio restoration, and honest continuity instrumentation without making any live provider calls.
- **Absolute Phase Rule**:
  - `FLOW_PAID_CLICKS = 0`
  - `FLOW_LIVE_GENERATIONS = 0`
  - `FLOW_CREDITS_SPENT = 0`
  - Zero paid Google Flow generations dispatched during this phase.

---

## 2. Architectural Design

### 2.1 Rational FPS Math & Segment Frame Invariants
- **Problem**: Nominal 30 fps media often operates at NTSC `30000 / 1001` (≈ 29.97003 fps). Assuming 300 frames results in a duration of `10.010` seconds, which strictly violates Google Flow's $\le 10.000$ second input restriction.
- **Architecture**:
  - Implemented exact rational frame calculation using `r_frame_rate` (`num`, `den`).
  - Maximum allowable frames per segment:
    $$\text{maxFramesPerSegment} = \left\lfloor \frac{10.0 \times \text{r\_num}}{\text{r\_den}} \right\rfloor$$
    Strictly enforced with loop clamp ensuring:
    $$\frac{\text{maxFramesPerSegment} \times \text{r\_den}}{\text{r\_num}} \le 10.0000000\text{ s}$$
  - **Results**:
    - For exact $30.0$ fps ($30/1$): $\text{maxFramesPerSegment} = 300$ ($10.0000$s $\le 10.0$s).
    - For NTSC $29.97$ fps ($30000/1001$): $\text{maxFramesPerSegment} = 299$ ($9.9766$s $\le 10.0$s; 300 frames is rejected).

### 2.2 Segment Count Authority & Logical Coverage
- Segmentation count is derived authoritative from total frames and max segment frames:
  $$\text{segmentCount} = \left\lceil \frac{\text{totalFrames}}{\text{maxFramesPerSegment}} \right\rceil$$
- Invariants validated on every plan:
  - $\text{segments}[0].\text{startFrame} = 0$
  - $\text{segments}[i].\text{endFrame} = \text{segments}[i+1].\text{startFrame}$
  - $\text{segments}[\text{last}].\text{endFrame} = \text{totalFrames}$
  - Contiguous, zero gap, zero overlap.

### 2.3 Variable Frame Rate (VFR) Working Proxy Contract
- If source media is probed as VFR (`timing_facts.is_vfr == true`), a deterministic CFR working proxy is generated locally:
  - Encoded to H.264 CFR 30 fps (`working_proxy_cfr.mp4`) without audio.
  - Proxy is hashed and tracked in manifest (`sourceTimingMode = "VFR"`, `workingProxyCreated = true`).
  - Source segments for Flow generation are extracted from the CFR proxy.
  - **Crucial Invariant**: Final audio is NEVER taken from the proxy; original source audio is restored directly from the untouched source media.

### 2.4 Child Video Normalization (`FlowVideoNormalizer`)
- Google Flow outputs may experience minor frame drift or variable resolutions.
- **Tolerance**: Allowable drift is strictly $\le 2$ frames. If drift $> 2$ frames, the pipeline rejects the output with `FLOW_CHILD_DURATION_DRIFT_EXCEEDED`.
- **Normalization Operations**:
  - Longer by 1–2 frames: deterministic trimming via `trim=start_frame=0:end_frame={planned},setpts=PTS-STARTPTS`.
  - Shorter by 1–2 frames: deterministic clone-frame padding via `tpad=stop_mode=clone:stop={deficit}`.
  - Canvas geometry: aspect-ratio preserved scaling and letterbox padding (`scale=W:H:force_original_aspect_ratio=decrease,pad=W:H:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1`).
  - Orientation check: incompatible orientation (e.g. portrait expected, landscape returned) fails with `FLOW_CHILD_ORIENTATION_MISMATCH`.

### 2.5 Concat Demuxer & Original Audio Restoration (`FlowStitcher`)
- **Video Stitching**:
  - Concat demuxer concatenates normalized child videos in strict index order ($0 \to 1 \to \dots \to N-1$).
  - Intermediate concatenated video stream is probed to ensure total frame count drift is $\le 1$ canonical frame.
- **Audio Restoration (Once Only)**:
  - Concat demuxer strips all child audio tracks (`-an`).
  - Muxes original full-length audio from source media exactly once.
  - Mode is truthfully reported as `STREAM_COPY` (if direct AAC copy succeeds) or `DETERMINISTIC_TRANSCODE` (`-c:a aac -b:a 192k`).
  - If source has no audio, output has zero audio streams and mode is recorded as `NO_SOURCE_AUDIO`.

### 2.6 Honest Continuity Instrumentation (`FlowContinuityManager`)
- **Zero-Fake Policy Compliance**:
  - No local face embedding model is bundled in the repository.
  - `faceContinuityStatus` is recorded truthfully as `UNVERIFIED`.
  - `identityContinuityGuaranteed` is set to `false` (defaulting to `SamePromptBaseline`).
- **Visual Diagnostics**:
  - Extracts boundary frames: last frame of segment $i$, first frame of segment $i+1$.
  - Computes real normalized mean pixel difference, recorded as `metric_name = "mean_pixel_delta"`, and categorized as `VISUAL_SEAM_METRIC` (not face recognition).

### 2.7 Manifest Schema v5 & Checkpoint Rehydration
- Manifest bumped to `CURRENT_FLOW_MANIFEST_SCHEMA_VERSION = 5`.
- Added:
  - `jobKind`: `SingleSegment`, `LongVideoParent`, `LongVideoChild`.
  - `parentLedger`: tracking planned costs, committed credits, reserved credits, and max total credits.
  - `longVideoPlan`: containing full frame-aligned segment metadata and proxy facts.
  - `continuityStrategy`, `continuityEvidence`, `audioRestorationMode`, `canonicalGeometry`.
- **Backward Compatibility**: All new fields use `#[serde(default)]`, enabling seamless deserialization of legacy v1–v4 manifests.
- **Rehydration Invariants**:
  - Restoring parent manifest from disk preserves all completed children.
  - 0 automated provider calls triggered on restart.
  - Any child in `GenerationAmbiguous` remains ambiguous and is never auto-retried.

---

## 3. Files Modified and Created

| File | Status | Description |
|---|---|---|
| `src-tauri/src/ai/flow/manifest.rs` | Modified | Bumped schema to v5; added `FlowJobKind`, `FlowParentLedger`, `FlowLongVideoPlan`, `FlowPlannedSegment`, `FlowIdentityContinuityStrategy`, `FlowFaceContinuityStatus`, `FlowSeamStatus`, `FlowContinuityEvidence`, `FlowAudioRestorationMode`, `FlowCanonicalGeometry`. |
| `src-tauri/src/ai/flow/segment.rs` | Modified | Added `plan_long_video` with rational FPS CFR planning, VFR working proxy creation, and `extract_long_video_segments` enforcing duration $\le 10.000$s. |
| `src-tauri/src/ai/flow/stitcher.rs` | Modified | Added `FlowVideoNormalizer` (handling drift $\le 2$ via `tpad` clone or trim, aspect ratio scaling, orientation checks) and `stitch_long_video_timeline` with single-pass audio restoration. |
| `src-tauri/src/ai/flow/continuity.rs` | **New** | Added `FlowContinuityManager` extracting boundary frames and computing real `VISUAL_SEAM_METRIC` with truthful `UNVERIFIED` face status. |
| `src-tauri/src/ai/flow/mod.rs` | Modified | Exported new types and continuity module. |
| `src-tauri/src/ai/mod.rs` | Modified | Registered `tests_phase_flow_p4a` test module. |
| `src-tauri/src/ai/tests_phase_flow_p3a.rs` | Modified | Updated test 09 to assert schema $\ge 4$ and verify backward-compatible reading of v4 JSON. |
| `src-tauri/src/ai/tests_phase_flow_p4a.rs` | **New** | Added complete 17-test suite for all Phase 4A requirements. |
| `src/features/flow/FlowGenPanel.tsx` | Modified | Added Long Video Multi-Segment Plan banner with estimated cost labeled `ESTIMATE ONLY` when source video duration $> 10$s, with zero automated Chrome launches. |

---

## 4. Test Matrix & Verification Results

### 4.1 Unit Test Suite (`tests_phase_flow_p4a`)
Executed serially with `--test-threads=1`:
```
running 17 tests
test ai::tests_phase_flow_p4a::test_flow_p4a_01_rational_fps_30000_1001_rejects_300_frames_and_caps_at_299 ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_02_segment_boundary_matrix_and_count_authority ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_03_logical_coverage_contiguous_no_gaps_no_overlaps ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_04_vfr_detection_creates_working_proxy_and_preserves_original_audio ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_05_frozen_prompt_across_all_segments ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_06_raw_child_short_by_2_frames_normalized_with_clone_pad ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_07_raw_child_long_by_2_frames_normalized_with_trim ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_08_raw_child_drift_exceeding_tolerance_fails_parent ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_09_different_child_resolutions_normalized_preserving_aspect_ratio ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_10_incompatible_child_orientation_fails_normalizer ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_11_strict_segment_index_order_stitching ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_12_source_without_audio_produces_zero_audio_streams ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_13_audio_restoration_stream_copy_vs_transcode ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_14_continuity_truth_unverified_and_visual_seam_distinction ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_15_checkpoint_rehydration_preserves_completed_and_zero_auto_provider_calls ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_16_ambiguous_child_never_auto_retries_on_restart ... ok
test ai::tests_phase_flow_p4a::test_flow_p4a_17_full_mock_acceptance_25s_source_to_project_derived_asset ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; finished in 16.58s
```

### 4.2 Regression Test Suites
- `prompt_tests`: 32 passed, 0 failed.
- `tests_phase_flow_p3a`: 42 passed, 0 failed (2 ignored for real Google Flow live credentials).
- `tests_phase_flow_p3b`: 0 failed (1 ignored for real paid Google Flow production acceptance).

### 4.3 Code Quality & Build Checks
- `cargo fmt --check`: Passed with zero formatting warnings.
- `cargo check`: Passed with zero errors and zero warnings.
- `npm.cmd test`: All 7 test files / 61 tests passed in 621ms.
- `npm.cmd run build`: Client bundle built cleanly in 1m 28s (`dist/index.html` 0.49 kB).

---

## 5. Live Test Costs & Accounting Confirmation

- **Paid Provider Clicks**: 0
- **Live Video Generations**: 0
- **Credits Expended in Phase 4A**: 0
- **Live Account Balance Check**: Skipped by design (Phase 4A is 100% non-paid).

---

## 6. Residual Limitations

1. **Facial Identity Continuity**: Without an integrated local face embedding model or official Google Flow multi-turn video extension API, cross-segment identity relies on prompt freezing (`SamePromptBaseline`). Face continuity is therefore reported as `UNVERIFIED`.
2. **Phase Boundary**: All long-video architecture is verified locally and under mock tests. No live multi-segment generation on Google Flow was submitted (reserved for Phase FLOW-P4-B under human authorization).
