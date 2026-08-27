# AUTOVIDEO-AI — Phase FLOW-P4-A.1 / FLOW-P4-B0 Engineering Report
**Long Video Production Runtime Hardening, True Decoded-Pixel Seam Metrics, and Real Two-Segment Non-Submitting Preflight**

**Date:** August 27, 2026  
**Status:** Completed & Frozen  
**Preflight Live Cost Discovery:** 40 credits total (20 credits/segment)  
**Live Generations Incurred:** 0  
**Credits Spent:** 0  

---

## 1. Absolute Invariants Confirmation

| Invariant | Target | Observed | Status |
|:---|:---:|:---:|:---:|
| `FLOW_PAID_CLICKS` | 0 | 0 | PASSED |
| `FLOW_LIVE_GENERATIONS` | 0 | 0 | PASSED |
| `FLOW_CREDITS_SPENT` | 0 | 0 | PASSED |
| `INITIAL_CREDIT_BALANCE` | 1050 | 1050 | VERIFIED (`profile_2`) |
| `FINAL_CREDIT_BALANCE` | 1050 | 1050 | VERIFIED (`profile_2`) |

Under NO circumstances were any Generate or Submit buttons clicked during this phase. All preflight tickets generated were immediately invalidated and consumed without dispatching generation requests.

---

## 2. Source Asset Selection & Verification (Sections 19 & 20)

### 2.1 Baseline Acceptance Asset Details
- **Source File:** `test-assets/p4b_source_15s.mp4` (derived deterministically from user test video `video_test.mp4`)
- **Duration:** Exactly `15.000000` seconds
- **Frame Count:** Exactly `450` frames @ `30/1` FPS CFR
- **Geometry:** `1080x1920` (SAR 1:1, DAR 9:16 Portrait)
- **Audio:** AAC Stereo 48,000 Hz (`mp4a.40.2`)
- **Visual Subject Check:** Continuous single person, no cuts across the seam boundary (verified with `select='gt(scene,0.4)'` yielding 0 scene transitions in 15 seconds).

### 2.2 Segment Decomposition Matrix
- **Segment 0 (`segment_000.mp4`):**
  - Timeline: `0.000000s` to `10.000000s`
  - Frame Range: `0` to `300` (300 frames)
  - Audio: Silent stream for provider submission (`-an`)
- **Segment 1 (`segment_001.mp4`):**
  - Timeline: `10.000000s` to `15.000000s`
  - Frame Range: `300` to `450` (150 frames)
  - Audio: Silent stream for provider submission (`-an`)

---

## 3. P4-A.1 Architectural Hardening Implemented

### 3.1 True Decoded-Pixel Seam Metric (FFmpeg Rawvideo Pipe)
- **Problem Fixed:** Previous draft read compressed JPEG file bytes and computed byte deltas, which measured container/huffman encoding variance rather than visual content difference.
- **Solution:** Replaced byte comparison with a true decoded visual seam metric:
  $$\Delta = \frac{\sum_{i=1}^N |P_A[i] - P_B[i]|}{N \times 255.0}$$
  Decoded via FFmpeg rawvideo grayscale pipe (`-vf scale=256:256 -format=gray -f rawvideo -pix_fmt gray -`).
- **Semantic Classification:** Labeled strictly as `metricCategory: VISUAL_SEAM_METRIC`, never conflated with `FACE_IDENTITY_SIMILARITY`.
- **Honest Seam Status:** Without camera/lighting calibration, seams default to `FlowSeamStatus::Unverified` rather than claiming artificial pass/fail thresholds.
- **Contact Sheet Generation:** Extracted 6 keyframes around boundary:
  `[T - 250ms, T - 100ms, T_last, T_first, T + 100ms, T + 250ms]` tiled into `boundary_{idx:03}_contact_sheet.jpg` with manifest-relative path recorded.

### 3.2 End-to-End Rational FPS
- Replaced floating-point and four-decimal FPS strings (`-r 29.9700`) across all FFmpeg pipeline calls with exact rational struct:
  ```rust
  pub struct FlowRationalFrameRate {
      pub numerator: u32,
      pub denominator: u32,
  }
  ```
- Generates exact FFmpeg rate arguments (`-r 30/1`, `-r 30000/1001`, `fps=fps=30000/1001`).
- Exact frame-based duration calculation: `frames * denominator / numerator`.

### 3.3 Two-Pass Child Normalization
- **Pass 1:** Stream canonicalization to rational FPS, canonical geometry, SAR 1:1, yuv420p, no audio.
- **Pass 2:** Exact timeline drift alignment:
  - If drift == 0: finalize.
  - If short $\le 2$ frames: clone-pad final frame.
  - If long $\le 2$ frames: trim excess frames.
  - If $|\text{drift}| > 2$: fail immediately with `FLOW_CHILD_DURATION_DRIFT_EXCEEDED`.
- Immediate cleanup of Pass 1 intermediate files.

### 3.4 Explicit Stitch Ordering
- Segment inputs typed as `FlowNormalizedSegment { segment_index, path, frame_count, sha256 }`.
- `stitch_long_video_timeline` sorts inputs by `segment_index` and strictly validates:
  - First index == 0
  - Monotonically contiguous (no gaps, no duplicates)
  - Last index == $N - 1$

### 3.5 Production Orchestrator Parent/Child Path
- Probes media duration upon job initiation: if $>10.000$s, assigns `FlowJobKind::LongVideoParent`.
- Enforces explicit parent budget guard (`FLOW_TOTAL_CREDIT_BUDGET_REQUIRED`).
- Sequential child worker execution with atomic manifest checkpoints.
- Rehydration safety: completed children remain completed and immutable; 0 provider calls on restart; ambiguous children never auto-retry.

---

## 4. Real P4-B0 Preflight Discovery (Live Google Flow)

Real non-submitting preflights were executed against Google Flow via `profile_2`:

```
==================================================
FLOW-P4-B0 REAL TWO-SEGMENT NON-SUBMITTING PREFLIGHT
FLOW_PAID_CLICKS = 0, FLOW_LIVE_GENERATIONS = 0, FLOW_CREDITS_SPENT = 0
==================================================
[P4-B0 STEP 1] Refreshing live credit balance before preflight...
INITIAL_PROFILE_STATUS: Ready
INITIAL_CREDIT_BALANCE: Some(1050)

--------------------------------------------------
[P4-B0 STEP 2] Executing Preflight for Segment 0 (0-10s)...
SEGMENT_0_PREFLIGHT_READY: true
SEGMENT_0_VIDEO_EDIT_ACTIVE: true
SEGMENT_0_CONFIG_VERIFIED: true
SEGMENT_0_LIVE_COST: 20

--------------------------------------------------
[P4-B0 STEP 3] Executing Preflight for Segment 1 (10-15s)...
SEGMENT_1_PREFLIGHT_READY: true
SEGMENT_1_VIDEO_EDIT_ACTIVE: true
SEGMENT_1_CONFIG_VERIFIED: true
SEGMENT_1_LIVE_COST: 20

--------------------------------------------------
[P4-B0 STEP 4] Refreshing final credit balance...
FINAL_CREDIT_BALANCE: 1050
CREDITS_SPENT: 0

==================================================
P4-B0 DISCOVERY SUMMARY & AUTHORIZATION FORMAT
FLOW_PAID_CLICKS = 0
FLOW_LIVE_GENERATIONS = 0
FLOW_CREDITS_SPENT = 0
SEGMENT_0_LIVE_COST = 20
SEGMENT_1_LIVE_COST = 20
PROJECTED_CURRENT_LIVE_COST = 40

PROPOSED AUTHORIZATION FORMAT:
Approve FLOW-P4-B: max 40 credits total, exactly 2 generations.
==================================================
```

---

## 5. Automated Verification Summary

| Test Suite | Tests Run | Result | Notes |
|:---|:---:|:---:|:---|
| `cargo test ... tests_phase_flow_p4a1` | 8 | PASSED | Rational FPS, 2-pass norm, decoded pixel metric, budget guards |
| `cargo test ... tests_phase_flow_p4a` | 17 | PASSED | End-to-end long video architecture, full mock 25s pipeline |
| `cargo test ... test_flow_p4b0_real...` | 1 (live) | PASSED | Real 2-segment non-submitting preflight on `profile_2` |
| `npm run build` | - | PASSED | Frontend bundle build clean |
| `npm test` | 61 | PASSED | Frontend store & UX test suite clean |
| `cargo check` | - | PASSED | 0 warnings, 0 errors |
| `cargo fmt --check` | - | PASSED | Perfectly formatted |

---

## 6. Proposed Authorization Format for P4-B

To proceed with real paid generation for Phase FLOW-P4-B:

> **Approve FLOW-P4-B: max 40 credits total, exactly 2 generations.**
