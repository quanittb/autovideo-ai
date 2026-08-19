# AutoVideo AI — Phase Cloud Live Report
## Real Provider Acceptance Only

---

## 1. Provider & Model Inventory

- **Selected Provider**: **Replicate**
- **Selected Model**: `minimax/video-01`
- **Real Request ID**: `N/A (Execution Blocked before Dispatch)`
- **Real Execution Status**: `REAL_CLOUD_LIVE_BLOCKED`

---

## 2. Input Asset Verification

- **Mandatory Input Image**: `C:\Users\quant\Dropbox\PC\Downloads\QuanPH.png`
- **Status**: Physically present on disk and readable.
- **SHA-256 Checksum**: `a95f9c4728569502b4895696d740c0344d567ee7f8a70c3952d4eec7dc5891ba`
- **MIME / Container**: Valid PNG image.

---

## 3. Credential & Environment Verification

- **Credential Target**: `REPLICATE_API_TOKEN`
- **Discovery Status**: **MISSING**
- **Zero-Fake Action**: In accordance with the **Absolute Zero-Fake Policy**, the runner refused to simulate predictions, fabricate fake prediction IDs, synthesize placeholder MP4 files, or report a false PASS.
- **Status File Generated**: [`outputs/cloud_live/status.json`](file:///d:/rustProject/autovideo-ai/outputs/cloud_live/status.json)
  ```json
  {
    "status": "REAL_CLOUD_LIVE_BLOCKED",
    "reason": "MISSING_PROVIDER_CREDENTIAL",
    "timestamp": 1786887723,
    "provider": "replicate",
    "zeroFakeVerified": true
  }
  ```
- **Metadata File Generated**: [`outputs/cloud_live/metadata.json`](file:///d:/rustProject/autovideo-ai/outputs/cloud_live/metadata.json)
- **Validation File Generated**: [`outputs/cloud_live/validation.json`](file:///d:/rustProject/autovideo-ai/outputs/cloud_live/validation.json)

---

## 4. Technical Artifact & Validation Telemetry

| Metric | Measured Value | Telemetry Note |
|---|---|---|
| **Output MP4 Artifact** | `N/A (Blocked)` | No file fabricated |
| **Output SHA-256** | `N/A` | No file fabricated |
| **Duration / FPS / Codec** | `N/A` | No file fabricated |
| **Submission Latency** | `N/A` | No dispatch performed |
| **Generation Latency** | `N/A` | No remote worker executed |
| **Download Latency** | `N/A` | No download performed |
| **Credential Check Latency** | `0.001 sec` | Instant local check |
| **Estimated Cost** | `UNKNOWN` | Unconfigured provider status |
| **Actual Cost** | `$0.00` | Zero billing incurred |

---

## 5. Performance Comparison: Local vs. Cloud

| Pipeline Path | Execution Latency | Resolution | Hardware Constraint | Cost / Notes |
|---|---|---|---|---|
| **Local Pipeline (GTX 1650)** | ~18.5 min for 60s video | 288x512 / 512x768 | Strict 4GB VRAM limit; requires FP32 sequential layer offload | Free ($0.00), but heavy compute load |
| **Cloud Pipeline (Replicate)** | ~15–30 sec per segment | 720x1280 / 1080x1920 | Zero local VRAM requirement (runs on CPU/low-end GPU) | ~$0.04/sec ($0.24 per 6s clip); requires API token |

---

## 6. How to Trigger Live Cloud Acceptance

Once a real Replicate API token is available, the live runner can be invoked in one command:

```powershell
$env:REPLICATE_API_TOKEN = "r8_your_actual_token_here"
& "d:\rustProject\autovideo-ai\.venv-generative\Scripts\python.exe" "d:\rustProject\autovideo-ai\src-tauri\scripts\cloud_live_acceptance.py"
```

The script will automatically:
1. Submit the prediction payload to Replicate `minimax/video-01`.
2. Poll until `status == "succeeded"`.
3. Download the real remote MP4 to `outputs/cloud_live/result/real_generated.mp4`.
4. Validate with FFprobe and extract `frame_first.png`, `frame_middle.png`, and `frame_last.png`.
5. Record latency timestamps ($T_0 \to T_4$) and emit `outputs/cloud_live/status.json` $\to$ `REAL_CLOUD_SUCCESS`.

---

## 7. Final Machine-Readable Classification

**CLASSIFICATION: `REAL_CLOUD_LIVE_BLOCKED`**
