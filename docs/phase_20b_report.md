# Phase 20B Report: Real Google Flow Paid Generation & Pipeline Acceptance Freeze Audit

## 1. Executive Summary & Acceptance Decision
Phase 20B executed exactly ONE authorized live production generation against Google Flow using `profile_2`.

- **Historical Accounting**:
  - `FLOW_GENERATIONS`: `1`
  - `GENERATE_CLICKS`: `1`
  - `AUTO_RETRY`: `0`
  - `SECOND_GENERATION`: `NO` (Strict single-run policy enforced)
- **Pipeline Decision**: **`PHASE20B_FREEZE_STATUS = BLOCKED_BY_DURATION_MISMATCH`**
  - While Google Flow successfully accepted the prompt and returned a genuine Google-encoded artifact (`951,153 bytes`, `1280x720`, `24fps`), the generated video stream is **`4.000s`** whereas the benchmark source is **`9.682s`** (drift `5.682s`).
  - Muxing original `9.685s` benchmark audio onto a `4.000s` video creates a duration-mismatched container. The updated safety gates (`FlowOutputValidator` and `FlowStitcher`) now strictly **FAIL** and **BLOCK** this mismatch from being promoted as a successful edit.

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

| Metric | A. Original Benchmark | B. Flow Child (`child_out_000.mp4`) | C. Muxed Test (`final_flow_output.mp4`) |
|---|---|---|---|
| **Container Format** | QuickTime / MOV (`mp42`) | QuickTime / MOV (`isom`) | QuickTime / MOV (`isom`) |
| **Container Duration** | `9.685313 s` | `4.010000 s` | `9.685000 s` |
| **Video Stream Duration** | `9.682000 s` | `4.000000 s` | `4.000000 s` |
| **Audio Stream Duration** | `9.685313 s` | `4.010000 s` | `9.685000 s` |
| **Frame Count** | `291` | `96` | `96` |
| **Frame Rate (FPS)** | `30.0 fps` (`145500/4841`) | `24.0 fps` (`24/1` CFR) | `24.0 fps` (`24/1` CFR) |
| **Resolution** | `1080x1920` (9:16) | `1280x720` (16:9) | `1280x720` (16:9) |
| **Encoder Tag** | `AVC Coding` | `Google` | `Lavf62.19.100` |
| **SHA-256 Hash** | `2832B907...8D91` | `9A040812...C0F9` | `CB1ACC97...4605` |

### 3.2. Explicit Duration Mismatch Calculations
- **Ratio (`childVideoDuration / sourceDuration`)**: `4.000 / 9.682 = 0.4131` (~41.31% of source duration)
- **Duration Drift (`abs(childVideoDuration - sourceDuration)`)**: `abs(4.000 - 9.682) = 5.682 s`
- **Result**: The current final artifact has a 4-second video track with a 9.685-second audio track (5.685s audio overhang). It is **NOT** a valid duration-preserving edit.

---

## 4. Root Cause Analysis: Why Flow Returned 4 Seconds
- **FLOW_CHILD_DURATION_CAUSE**: `DURATION_CONTROL_NOT_SET` / `PROVIDER_BEHAVIOR`
- **Evidence**:
  1. The Flow UI automation dispatched the video upload and prompt without interacting with a duration picker control.
  2. Google Flow's underlying video generation model (Veo) intrinsically defaults to 4.0-second (96-frame at 24fps) or 8.0-second discrete clips for prompt-driven generation.
  3. Single-clip requests for arbitrary durations (e.g. 9.682s) are truncated by Google Flow to its standard model output length unless structured multi-segment chaining is applied.

---

## 5. Result Identity & Submission Evidence

- **RESULT_IDENTITY**: `PROVEN`
  - **Evidence**: The project workspace card (`/project/95ae3d59-9e7e-4786-9a9e-8a116aa06772/edit/18ccba4e-f2d8-4cca-a140-745a42c89137`) matches the exact frozen prompt string, timestamp (`2026-08-24T10:40:55.926Z`), attempt ID `att_0_1787568041711`, and contains genuine `encoder: Google` metadata.
- **Evidence Terminology Breakdown**:
  - `CLICK_DISPATCH_EVIDENCE`: `semantic:btn_dispatched:2026-08-24T10:40:55.926Z:att_0_1787568041711` (Proves the one paid click was dispatched).
  - `PROVIDER_SUBMISSION_EVIDENCE`: Generation card created in project workspace with progress indicator.
  - `PROVIDER_COMPLETION_EVIDENCE`: Transition to `Xong` (Done) status with playable stream URL (`/fx/api/trpc/media.getMediaUrlRedirect?name=e17ec2a6-aaed-445c-95ba-eec206da0961`) and functional download control.

---

## 6. Visual Quality Assessment

*Visual assessment is restricted strictly to the available 4-second generated output:*
- **SUBJECT_PRESERVED**: `PASS_FOR_AVAILABLE_OUTPUT`
- **REQUESTED_SUNSET_LIGHTING_VISIBLE**: `PASS_FOR_AVAILABLE_OUTPUT` (Golden warm sunset lighting visible through trees and ambient scene)
- **ACTION_PRESERVED**: `PARTIAL_ONLY` (Only first 4.0 seconds generated out of 9.682s)
- **CAMERA_PRESERVED**: `PARTIAL_ONLY` (Only first 4.0 seconds generated out of 9.682s)
- **FULL_DURATION_PRESERVED**: `NO`
- **UNWANTED_MAJOR_CHANGE**: `NO` (Within available 4-second output)

---

## 7. Pipeline Safety Hardening

1. **`FlowOutputValidator` Duration Guard**:
   - Compares video stream duration (`v:0`) against `expected_duration_sec`.
   - Enforces conservative tolerance: `max(0.5s, 5% of expected duration)`.
   - Validates audio stream duration against video stream duration to reject misaligned muxes.
   - Throws `FLOW_OUTPUT_DURATION_MISMATCH` on drift violations.

2. **`FlowStitcher` Audio Safety**:
   - Validates the intermediate concatenated video stream duration **BEFORE** invoking audio muxing.
   - Refuses to produce mismatched video+audio artifacts.

---

## 8. Verification & Acceptance Summary Flags

```
FLOW_REAL_BROWSER_VERIFIED = YES
FLOW_PROVIDER_RETURNED_REAL_ARTIFACT = YES
FLOW_REAL_GENERATION_SUBMITTED = YES
FLOW_REAL_GENERATION_PIPELINE_ACCEPTED = NO
PREVIEW_RUNTIME_VERIFIED = YES
FLOW_SEGMENT_BOUNDARY_VISUAL_QUALITY = NOT LIVE VERIFIED
FLOW_FULL_DURATION_PRESERVED = NO
FLOW_OUTPUT_DURATION_MISMATCH = expected 9.682s, actual 4.000s
FLOW_GENERATIONS = 1
GENERATE_CLICKS = 1
FLOW_CREDITS <= 40
PHASE20B_FREEZE_STATUS = BLOCKED_BY_DURATION_MISMATCH
```
