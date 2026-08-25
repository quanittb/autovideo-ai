# Phase 20C: Face Replacement Provider Readiness & Benchmark Specification

## 1. Product Contract & Default Architecture
- **Primary / Default Transformation**: `FACE_REPLACE`
- **Default Identity Mode**: `GENERATED`
  - When the user submits a video without providing an extra face image, AutoVideo AI generates a **new synthetic identity** and replaces exactly ONE target face.
- **Reference Identity Mode**: `REFERENCE`
  - Activated ONLY when the user explicitly uploads a reference face image (`test-assets/phase20b/faces/face.jpg`).
- **Decoupling Rule**: The physical presence of `face.jpg` in the repository does not alter the runtime default `GENERATED` mode.

---

## 2. Authoritative Credential Contract

| Provider / Feature | Application Default Key | Precedence Order | Current Status |
|---|---|---|---|
| **Gemini Gen Prompt** | `DEFAULT_GEMINI_API_KEY = "Axxxxxxxxxxx"` (Sentinel) | 1. User Override (Settings)<br>2. `GEMINI_API_KEY` Env<br>3. `DEFAULT_GEMINI_API_KEY`<br>4. `NOT_CONFIGURED` | `APPLICATION_DEFAULT` (Sentinel `"Axxxxxxxxxxx"` treated as `NOT_CONFIGURED`) |
| **Pruna (`p-video-replace`)** | **NONE** (No built-in default key) | 1. User Override (Settings)<br>2. `REPLICATE_API_TOKEN` Env<br>3. `NOT_CONFIGURED` | `MISSING` (`PRUNA_CREDENTIAL_REQUIRED`) |
| **BRIA** | **NONE** (No built-in default key) | 1. User Override (Settings)<br>2. `BRIA_API_TOKEN` Env<br>3. `NOT_CONFIGURED` | `NOT_CONFIGURED` |
| **Google Flow** | **N/A** (Uses authenticated Chrome profile) | Local profile session (`profile_2`) | `READY` (`AUTHENTICATED_PROFILE`) |

---

## 3. Benchmark Asset Inventory

| Case ID | Physical Video File | Resolution / Orientation | Frames @ FPS | Duration | Codecs (V/A) | SHA-256 |
|---|---|---|---|---|---|---|
| **C1** | `test-assets/phase20c/videos/flow_acceptance_01.mp4` | 576x1024 (9:16 Portrait) | 299 @ 30.0 fps | 9.989s | h264 / aac | `68747585122B46F78168F951AA43E461DBAFE19E4DFBA6D519578A004F8D1694` |
| **C2** | `test-assets/phase20c/videos/flow_acceptance_02.mp4` | 1080x1920 (9:16 Portrait) | 291 @ 30.0 fps | 9.685s | h264 / aac | `2832B907BDDE50A875CC6A784E3505A3E545885B3D8AEFCB0238947A302A8D91` |
| **C3** | `test-assets/phase20c/videos/flow_acceptance_03.mp4` | 1080x1920 (9:16 Portrait) | 297 @ 30.0 fps | 9.899s | h264 / aac | `C2D030FCE3788E29C808B117A087F239D1E4B92B583EA9999CAF5191F76838DA` |

### Shared Reference Identity Asset
- **File**: `test-assets/phase20b/faces/face.jpg` (1200x1600 JPEG, 842,420 bytes, SHA: `48747DB972E0A7C3CC3517F24EF5A730136B280FCD46BDAA70D502A3D849C31E`)

---

## 4. Frozen C3 Multi-Face Target Confirmation

In `C3` (`flow_acceptance_03.mp4`), the user has frozen the target selection to **Candidate 1**:
- **Target Face Index**: `1`
- **Target Descriptor**: `PASSENGER_RIGHT`
- **Description**: Right passenger seat, black jacket, holding mobile phone, front / 3-quarter view, talking to driver.
- **Anchor Timestamp**: $t = 2.0\text{s}$
- **Normalized Bounding Box**: `[0.5370, 0.4479, 0.2222, 0.1458]`
- **C3_TARGET_CONFIRMED**: **`YES`**
- **Non-Target Invariant**: `Candidate 0 (DRIVER_LEFT)` must remain completely unchanged (`preserve_non_target_faces = true`).

Visual confirmation artifact: [test-assets/phase20c/c3_target_candidates.png](file:///D:/rustProject/autovideo-ai/test-assets/phase20c/c3_target_candidates.png).

---

## 5. Provider Capability & Benchmark Pricing

### Capability Audit
- **Pruna**: `PROMPT_BASED / INDIRECT` for generated face replacement (video-to-video conditioned on prompt with empty `images: []`).
- **Google Flow**: `PROMPT_BASED` on active `/edit/` timeline with 100% full-duration motion and background preservation.

### Unit Pricing & Benchmark Cost Matrix
- **Pruna**: $0.030/s (720p), $0.060/s (1080p)
- **Flow**: 20 credits per 10s video generation
- **Authorized Budget**: `$1.50 USD` (Pruna) / `60 credits` (Flow)

| Case | Resolution | Duration | Pruna Cost (USD) | Flow Cost (Credits) |
|---|---|---|---|---|
| **C1** | 576x1024 | 9.989 s | $\$0.300$ | 20 credits |
| **C2** | 1080x1920 | 9.682 s | $\$0.581$ | 20 credits |
| **C3** | 1080x1920 | 9.899 s | $\$0.594$ | 20 credits |
| **Total (C1 + C2 + C3)** | — | — | **`$1.475 USD`** | **`60 Flow credits`** |

---

## 6. Pre-Paid Execution Gate
- **Flow Auth Status**: `READY` (`profile_2`)
- **Pruna Credential Status**: `MISSING` (`REPLICATE_API_TOKEN` is not set)
- **Pre-Paid Policy**: To prevent asymmetric credit consumption and ensure fair benchmark comparison, execution is blocked until `REPLICATE_API_TOKEN` is configured (`PRUNA_CREDENTIAL_REQUIRED`).
- **Total Paid Calls Executed**: `0`
