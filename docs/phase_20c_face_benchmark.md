# Phase 20C: Face Replacement Provider Readiness & Benchmark Specification

## 1. Product Contract & Default Architecture
- **Primary / Default Transformation**: `FACE_REPLACE`
- **Default Identity Mode**: `GENERATED`
  - When the user submits a video without providing an extra face image, AutoVideo AI generates a **new synthetic identity** and replaces exactly ONE target face.
- **Reference Identity Mode**: `REFERENCE`
  - Activated ONLY when the user explicitly uploads a reference face image (`test-assets/phase20b/faces/face.jpg`).
- **Decoupling Rule**: The physical presence of `face.jpg` in the repository does not alter the runtime default `GENERATED` mode.

---

## 2. Benchmark Asset Inventory

| Case ID | Physical Video File | Resolution / Orientation | Frames @ FPS | Duration | Codecs (V/A) | SHA-256 |
|---|---|---|---|---|---|---|
| **C1** | `test-assets/phase20c/videos/flow_acceptance_01.mp4` | 576x1024 (9:16 Portrait) | 299 @ 30.0 fps | 9.989s | h264 / aac | `68747585122B46F78168F951AA43E461DBAFE19E4DFBA6D519578A004F8D1694` |
| **C2** | `test-assets/phase20c/videos/flow_acceptance_02.mp4` | 1080x1920 (9:16 Portrait) | 291 @ 30.0 fps | 9.685s | h264 / aac | `2832B907BDDE50A875CC6A784E3505A3E545885B3D8AEFCB0238947A302A8D91` |
| **C3** | `test-assets/phase20c/videos/flow_acceptance_03.mp4` | 1080x1920 (9:16 Portrait) | 297 @ 30.0 fps | 9.899s | h264 / aac | `C2D030FCE3788E29C808B117A087F239D1E4B92B583EA9999CAF5191F76838DA` |

### Shared Reference Identity Asset
- **File**: `test-assets/phase20b/faces/face.jpg` (1200x1600 JPEG, 842,420 bytes, SHA: `48747DB972E0A7C3CC3517F24EF5A730136B280FCD46BDAA70D502A3D849C31E`)

---

## 3. C3 Multi-Face Target Confirmation

In `C3` (`flow_acceptance_03.mp4`), two persons are detected in a car interior setting:
- **Candidate 0 (`DRIVER_LEFT`)**:
  - Position: Left foreground (Driver's seat).
  - Appearance: White Hello Kitty t-shirt, ponytail, side profile facing passenger.
  - Normalized Box: `[0.0185, 0.3229, 0.4259, 0.2500]` at anchor $t = 2.0\text{s}$.
- **Candidate 1 (`PASSENGER_RIGHT`)**:
  - Position: Right passenger seat.
  - Appearance: Black jacket, holding mobile phone, front/3-quarter view talking to driver.
  - Normalized Box: `[0.5370, 0.4479, 0.2222, 0.1458]` at anchor $t = 2.0\text{s}$.

**Target Blocker Status**: `C3_TARGET_CONFIRMED = NO`. C3 execution remains strictly blocked until the user selects either Candidate 0 or Candidate 1.

---

## 4. Provider Capability Audit

### Pruna (`prunaai/p-video-replace`)
- **Task Type**: Video character & subject replacement (video-to-video conditioned on prompt and optional reference image).
- **Generated Face Support**: `PROMPT_BASED / INDIRECT` (Runs with empty image list `images: []` conditioned purely on natural language prompt).
- **Reference Face Support**: `NATIVE` (Accepts `images: [face_image_url]`).
- **Face-Only Preservation**: `PARTIAL` (Transforms at character region level; subject body/clothing may undergo secondary stylization).
- **Duration & Resolution**: Arbitrary segment duration; 720p and 1080p resolution tiers supported.

### Google Flow (True Uploaded-Video Edit Mode)
- **Task Type**: Natural language video-to-video editing on active timeline.
- **Generated Face Support**: `PROMPT_BASED` (Direct semantic prompt on `/edit/` timeline without image attachment).
- **Reference Face Support**: `INDIRECT / UNSUPPORTED` (Active edit composer does not accept multi-modal reference image attachments in edit timeline).
- **Face-Only Preservation**: `EXPECTED` (Demonstrated 100% full-duration geometry, background, and motion preservation in Phase 20B).

---

## 5. Provider Pricing Evidence & Cost Projections

### Authoritative Unit Rates
- **Pruna**:
  - 720p: `$0.030 / sec`
  - 1080p: `$0.060 / sec`
- **Google Flow**:
  - `20 credits` per 10s video generation (live UI tooltip readback)

### Per-Case Cost Breakdown

| Case | Resolution | Duration | Pruna Cost (USD) | Flow Cost (Credits) |
|---|---|---|---|---|
| **C1** | 576x1024 (720p tier) | 9.989 s | $\$0.300$ | 20 credits |
| **C2** | 1080x1920 (1080p tier) | 9.682 s | $\$0.581$ | 20 credits |
| **C3** | 1080x1920 (1080p tier) | 9.899 s | $\$0.594$ | 20 credits |

### Projection Scenarios
- **PROJECTION A (C1 + C2 Only — 4 calls total)**:
  - **Pruna**: $\$0.881\text{ USD}$
  - **Flow**: $40\text{ credits}$
- **PROJECTION B (C1 + C2 + Confirmed C3 — 6 calls total)**:
  - **Pruna**: $\$1.475\text{ USD}$
  - **Flow**: $60\text{ credits}$

---

## 6. Planned 13-Signal Quality Evaluation Matrix
When paid benchmark runs are authorized, outputs will be evaluated across:
1. `IDENTITY_CHANGED_FROM_SOURCE`
2. `IDENTITY_TEMPORAL_CONSISTENCY`
3. `FACE_GEOMETRY_STABILITY`
4. `EXPRESSION_PRESERVATION`
5. `HEAD_POSE_PRESERVATION`
6. `MOUTH_MOVEMENT_PRESERVATION`
7. `TARGET_ONLY_CORRECTNESS`
8. `NON_TARGET_FACE_PRESERVATION`
9. `BODY_PRESERVATION`
10. `CLOTHING_PRESERVATION`
11. `BACKGROUND_PRESERVATION`
12. `ACTION_PRESERVATION`
13. `DURATION_PRESERVATION`
