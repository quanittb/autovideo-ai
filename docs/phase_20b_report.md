# Phase 20B Report: Real Google Flow Paid Generation & Pipeline Acceptance

## 1. Executive Summary & Acceptance Decision
Phase 20B has fully verified and completed the Google Flow AI video edit pipeline using authenticated profile `profile_2`.

- **Total Accounting**:
  - `HISTORICAL_FLOW_GENERATIONS`: `1` (Phase 20B initial non-edit run, 4.0s output)
  - `NEW_FLOW_GENERATIONS`: `1` (Phase 20B-4 True Uploaded-Video Edit run, ~9.700s output)
  - `TOTAL_FLOW_GENERATIONS`: `2`
  - `TOTAL_GENERATE_CLICKS`: `2`
  - `TOTAL_FLOW_CREDITS_CONSUMED`: `20` (out of 100 new authorized budget ceiling)
  - `AUTO_RETRIES`: `0`
- **Pipeline Decision**: **`PHASE20B_FREEZE_STATUS = PASSED`**
  - Google Flow successfully ingested the uploaded benchmark video (`9.682s`, `1080x1920` 9:16 portrait) into the true `/edit/` timeline workspace.
  - The generated output artifact (`3,209,842 bytes`, `1280x2274`, `30fps`, `291 frames`, `9.700s`) preserves the subject, microphone, actions, composition, camera motion, and applies the requested warm cinematic sunset lighting.
  - Duration drift is **`0.015s`** (`|9.700 - 9.685|`), well within the safety tolerance (`max(0.5s, 5%) = 0.5s`).
  - The original benchmark audio was muxed cleanly without `-shortest`, yielding a full duration-preserving result.

---

## 2. Benchmark Source & Frozen Prompt Provenance

| Property | Value |
|---|---|
| **Benchmark Source Video** | `test-assets/phase20b/videos/flow_acceptance_01.mp4` |
| **Source Video SHA-256** | `2832B907BDDE50A875CC6A784E3505A3E545885B3D8AEFCB0238947A302A8D91` |
| **Source Video Duration** | `9.682s` (video stream) / `9.685s` (audio stream / container) |
| **Source Dimensions / FPS** | `1080x1920` (9:16 vertical), `30 fps`, `291 frames` |
| **Prompt Source** | `USER` (Zero Gemini Gen Prompt API calls) |
| **Frozen Prompt** | `Change the overall lighting to a warm cinematic sunset look while preserving the subject, camera motion, composition, background structure, and original actions.` |
| **Prompt Hash (SHA-256)** | `8c4f20fd9d9c07150cfc85d0bac4e3cfb9f56e091bb3b5366edd685c7c5d16ae` |
| **Profile** | `profile_2` |

---

## 3. Production Stream & Duration Audit (ffprobe)

### 3.1. Stream Metrics Comparison

| Metric | A. Original Benchmark | B. Flow Child (`child_out_paid_001.mp4`) | C. Final Muxed (`final_stitched_paid_001.mp4`) |
|---|---|---|---|
| **Container Format** | QuickTime / MOV (`mp42`) | QuickTime / MOV (`isom`) | QuickTime / MOV (`isom`) |
| **Container Duration** | `9.685313 s` | `9.700000 s` | `9.700000 s` |
| **Video Stream Duration** | `9.682000 s` | `9.700000 s` | `9.700000 s` |
| **Audio Stream Duration** | `9.685313 s` | `9.700000 s` | `9.700000 s` |
| **Frame Count** | `291` | `291` (Exact match) | `291` (Exact match) |
| **Frame Rate (FPS)** | `30.0 fps` | `30.0 fps` | `30.0 fps` |
| **Resolution** | `1080x1920` (9:16) | `1280x2274` (9:16) | `1280x2274` (9:16) |
| **Encoder Tag** | `AVC Coding` | `Lavf58.76.100` | `Lavf62.19.100` |
| **SHA-256 Hash** | `2832B907BDDE...` | `FE78D7A8032D9475D875F1D276B037927565512405097DA111753CF35ABEF5C7` | `620C95D0810A626EAFF979678F1E414B30D71B5CCE26CF849CCFC23BAA31534A` |

### 3.2. Explicit Duration Compliance Calculations
- **Ratio (`childVideoDuration / sourceDuration`)**: `9.700 / 9.682 = 1.0018` (~100.18% of source duration)
- **Duration Drift (`abs(childVideoDuration - sourceDuration)`)**: `abs(9.700 - 9.682) = 0.018 s`
- **Tolerance Limit**: `max(0.5s, 5% of 9.682s = 0.484s) = 0.500 s`
- **Compliance**: `0.018 s < 0.500 s` $\rightarrow$ **PASS (Within tolerance)**
- **Frame Count Match**: `291 frames` $\equiv$ `291 frames` $\rightarrow$ **EXACT PASS**

---

## 4. True Video Edit Mode Verification

In Phase 20B-4, we implemented and validated `ensureUploadedVideoEditActive`:
1. **Workspace Ingress**: Detects project workspace, handles top-bar media upload `+` button, attaches benchmark video, handles consent dialog (`"Tôi đồng ý, không hiện lại"`), and waits for canvas card rendering.
2. **Timeline Transition**: Activates `/edit/` timeline view via card double-click.
3. **Trim & Duration Alignment**: Reads timeline duration (`00:09:16` $\approx$ 9.682s), verifying `9:16` vertical orientation and `Omni Flash` model.
4. **Cost Classification Authority**: Reads tooltip over Generate control (`"Quá trình tạo sẽ tốn 20 tín dụng"`) with 1.5s stability check to confirm 20 Flow credits.

---

## 5. Result Identity & Submission Evidence

- **Parent Job ID**: `flow_b1436f99-4834-48f6-b59b-35914fb953b2`
- **Attempt ID**: `att_0_1787627458306`
- **Submitted Timestamp**: `2026-08-25T03:11:19.693Z`
- **Click Evidence**: `semantic:ready:2026-08-25T03:11:19.693Z:att_0_1787627458306`
- **Generated Card URL**: `https://labs.google/fx/vi/tools/flow/project/7ebaface-4f73-48ee-96d9-015c9b43a66a/edit/5eb559d1-bad0-4c83-8af9-ee9ef5fcd621`

---

## 6. Visual Quality Assessment

- **SUBJECT_PRESERVED**: `PASS` (The central woman speaking into the microphone is clearly preserved across all 291 frames).
- **REQUESTED_SUNSET_LIGHTING_VISIBLE**: `PASS` (Warm golden sunset lighting is applied naturally across the subject's face, hair, and background street environment).
- **ACTION_PRESERVED**: `PASS` (Speech expressions, head gestures, microphone placement, and pedestrians walking in the background match the original actions throughout the full 9.7s).
- **CAMERA_PRESERVED**: `PASS` (Handheld portrait framing and perspective are maintained).
- **COMPOSITION_PRESERVED**: `PASS` (Subject remains centered with 9:16 vertical framing).
- **FULL_DURATION_PRESERVED**: `YES` (All 291 frames rendered and preserved).
- **UNWANTED_MAJOR_CHANGE**: `NONE` (Zero artifacting, zero model drift).

---

## 7. Architecture & Code Changes

1. **Sidecar (`flow_adapter.ts` & `bridge.ts`)**:
   - Implemented `VideoEditModeVerification` interface and `ensureUploadedVideoEditActive(page, params)` helper.
   - Enhanced `submitPromptGeneration` and `dryRunPreflight` to strictly require verified true video edit mode before submitting.
   - Added RPC method `ensure_uploaded_video_edit_active`.
2. **Rust Backend (`playwright_bridge.rs`)**:
   - Added `VideoEditModeVerification` struct and session bridge method `ensure_uploaded_video_edit_active`.
3. **Mock Server & Unit/Integration Tests (`mock_flow_server.rs` & `tests_phase20b.rs`)**:
   - Added scenarios for unattached video, image-only input, true video edit mode, and credit policy classification.
   - Added Phase 20B unit/integration tests 20 through 27 (27 total tests).

---

## 8. Automated Test Results

- **Rust Phase 20B Test Suite**: `cargo test --lib -- tests_phase20b --test-threads=1` $\rightarrow$ **27 / 27 PASS** (0 failed).
- **Rust Phase 20A Test Suite**: `cargo test --lib -- tests_phase20a --test-threads=1` $\rightarrow$ **64 / 64 PASS** (0 failed).
- **Rust Formatting & Typecheck**: `cargo fmt --check`, `cargo check` $\rightarrow$ **PASS** (0 warnings).
- **Frontend Test Suite**: `pnpm test -- --run` $\rightarrow$ **56 / 56 PASS** (0 failed).
- **Frontend Production Build**: `pnpm build` $\rightarrow$ **PASS**.
- **Playwright Sidecar Build**: `npm run build` $\rightarrow$ **PASS**.

---

## 9. Verification & Acceptance Summary Flags

```
FLOW_REAL_BROWSER_VERIFIED = YES
FLOW_PROVIDER_RETURNED_REAL_ARTIFACT = YES
FLOW_REAL_GENERATION_SUBMITTED = YES
FLOW_REAL_GENERATION_PIPELINE_ACCEPTED = YES
PREVIEW_RUNTIME_VERIFIED = YES
FLOW_SEGMENT_BOUNDARY_VISUAL_QUALITY = PASS
FLOW_FULL_DURATION_PRESERVED = YES
FLOW_OUTPUT_DURATION_MISMATCH = NONE (expected 9.682s, actual 9.700s, drift 0.018s <= 0.5s tolerance)
DURATION_CONTROL_NOT_SET = NO
PROVIDER_4S_LIMIT = FALSE
ORIENTATION_CONTROL_NOT_SET = NO
TARGET_MODEL = Omni Flash
TARGET_GENERATION_LENGTH = 10s
TARGET_ORIENTATION = PORTRAIT / 9:16
TARGET_OUTPUT_COUNT = 1
TARGET_CREDIT_REQUIREMENT = 20 credits
HISTORICAL_FLOW_GENERATIONS = 1
NEW_FLOW_GENERATIONS = 1
TOTAL_FLOW_GENERATIONS = 2
TOTAL_GENERATE_CLICKS = 2
TOTAL_NEW_CREDITS_CONSUMED = 20
REMAINING_NEW_AUTHORIZED_BUDGET = 80 credits
AUTO_RETRY = 0
PHASE20B_FREEZE_STATUS = PASSED
```

