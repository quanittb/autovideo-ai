# Phase 17: Video Background Removal Benchmark Protocol

## 1. Objective & Scope
This benchmark protocol defines the evaluation methodology, metrics, and execution steps for benchmarking the **Replicate BRIA Video Background Removal (`bria/video-remove-background`)** provider when live testing is explicitly authorized (`ALLOW_PAID_LIVE_TEST=1`).

> **Safety Notice**: This document specifies the experimental protocol only. No live API calls or paid predictions are executed automatically as part of standard CI/offline verification.

---

## 2. Evaluation Metrics & Quality Rubric

| Dimension | Metric / Criterion | Threshold / Target | Evaluation Method |
| :--- | :--- | :--- | :--- |
| **Alpha Transparency Decodability** | Presence of decodable alpha channel in VP9 container | 100% Pass | FFprobe `alpha_mode=1` + `ffmpeg -c:v libvpx-vp9 -filter_complex "[0:v]alphaextract[a]"` |
| **Edge / Hair Matting Fidelity** | Subject boundary crispness, fine-edge preservation (hair, fur, motion blur) | Score >= 4.0 / 5.0 | Visual evaluation against source RGB plate |
| **Temporal Stability & Flicker** | Inter-frame consistency of alpha matte, absence of temporal buzzing/holes | Score >= 4.0 / 5.0 | Sequential frame difference analysis |
| **Audio Preservation** | Unaltered preservation of source audio stream when requested | 100% Pass | Stream codec & duration match via FFprobe |
| **Latency & Throughput** | Server processing time per second of video input | < 3.0s latency / input second | Timestamped lifecycle telemetry |
| **Pricing Predictability** | Observed cost vs estimated cost formula ($0.0042/s) | 100% match | CostRecord ledger verification |

---

## 3. Test Fixture Suite & Sampling Matrix

| Category | Description | Target Resolution | Duration | Audio Present |
| :--- | :--- | :--- | :--- | :--- |
| **Talking Head / Portrait** | Standard presenter talking in front of static background | 1080x1920 (9:16) / 1920x1080 (16:9) | 10.0s | Yes (AAC 48kHz) |
| **Dynamic Motion / Dance** | Rapid limb movement, variable depth, complex background | 1280x720 (16:9) | 10.0s | Yes |
| **Fine Details / Hair** | Windblown hair, transparent glasses, intricate silhouettes | 1920x1080 (16:9) | 10.0s | No |
| **High Resolution (4K)** | High-density 4K source video testing resolution preservation | 3840x2160 (16:9) | 5.0s | No |
| **Boundary Limit Case** | Maximum allowed single-clip duration | 1920x1080 (16:9) | 60.0s | Yes |

---

## 4. Execution Protocol (When Live Testing Enabled)

### Prerequisites:
1. Valid `REPLICATE_API_TOKEN` environment variable configured.
2. `ALLOW_PAID_LIVE_TEST=1` explicitly exported.
3. Network access to `https://api.replicate.com` and `https://replicate.delivery`.

### Steps:
1. **Preflight Probe**: Run `SourceMediaProbe::probe_file` on test media to extract exact duration, dimensions, FPS, and audio characteristics.
2. **Cost Gate Check**: Compute estimated cost via `GenerationRouter` / `ProviderRegistry` ($0.0042/s * probed duration) and confirm `reserved_budget <= max_budget`.
3. **Submission**: Submit prepared request with `background_color: "Transparent"`, `output_container_and_codec: "webm_vp9"`, `preserve_audio: bool`.
4. **Polling & Download**: Poll prediction status until `succeeded`, then download output WebM artifact to temporary storage with SSRF verification.
5. **Two-Stage Alpha Validation**:
   - Stage A: Verify `TAG:alpha_mode=1` or alpha pixel format via FFprobe.
   - Stage B: Execute `ffmpeg -c:v libvpx-vp9 -i <file> -vframes 1 -filter_complex "[0:v]alphaextract[a]" -map "[a]" -f null -`.
6. **Artifact Promotion**: Atomically promote validated WebM artifact to `<project>/artifacts/<internal_job_id>.webm`.
7. **Report Compilation**: Log metrics to benchmark registry and verify CostRecord accounting.
