# Phase 16 — Character Replacement Benchmark Protocol

**Model**: `prunaai/p-video-replace` (Replicate official candidate)  
**Status**: Specification & Offline Mock Protocol (Phase 16)  
**Live Cost Spent in Phase 16**: **$0.00** (Zero live inference calls)

---

## 1. Objective & Scope

This protocol establishes a standardized benchmark methodology for evaluating AI character replacement models across video quality, facial/identity consistency, temporal stability, motion preservation, and cost/latency efficiency.

In Phase 16, all lifecycle adapters, serializers, SSRF validators, upload paths, and recovery guards are verified offline. Live execution is guarded by `LiveExecutionPolicy` (`ALLOW_PAID_LIVE_TEST=0` by default).

---

## 2. Test Media & Fixture Standards

### 2.1 Default Test Fixture
- **Path**: `"C:\Users\quant\Dropbox\PC\Downloads\video_test.mp4"`
- **Development / Smoke Test Duration**: First **10.0 seconds** (optimizes iteration speed & cost).
- **Full Acceptance Duration**: Entire video duration.

### 2.2 Character Reference Images
- **Single Reference**: 1 high-resolution frontal portrait (512x512 to 1024x1024, JPEG/PNG).
- **Multi-Reference (Up to 3)**: Frontal, 45-degree angle, and expression reference for enhanced identity lock.

---

## 3. Evaluation Matrix & Scoring Criteria

| Metric | Target | Evaluation Method | Weight |
|---|---|---|---|
| **Identity Consistency** | ≥ 8.5/10 | Facial feature retention against reference portrait(s) | 25% |
| **Temporal Stability** | Zero flicker | Frame-to-frame boundary jitter, facial warp | 20% |
| **Motion Fidelity** | ≥ 9.0/10 | Pose, gesture, mouth movement matching source | 20% |
| **Background Preservation** | 100% untouched | SSIM/PSNR on non-character masked regions | 15% |
| **Audio Preservation** | Bit-exact stream | Audio track passthrough or re-muxing | 10% |
| **Latency & Cost Efficiency** | ≤ 2.5x real-time | Wall-clock execution time vs pricing tier ($0.03/s @ 720p, $0.06/s @ 1080p) | 10% |

---

## 4. Benchmark Matrix

| Suite ID | Task Class | Resolution | Target FPS | Reference Images | Audio Preservation | Expected Unit Cost |
|---|---|---|---|---|---|---|
| `CR-01` | Character Replacement | 720p (720x1280) | 24 fps | 1 image | Enabled (`true`) | $0.03 / sec |
| `CR-02` | Character Replacement | 720p (720x1280) | Original | 3 images | Enabled (`true`) | $0.03 / sec |
| `CR-03` | Character Replacement | 1080p (1080x1920) | 24 fps | 1 image | Enabled (`true`) | $0.06 / sec |
| `CR-04` | Character Replacement | 1080p (1080x1920) | 48 fps | 3 images | Disabled (`false`) | $0.06 / sec |

---

## 5. Security & Isolation Protocol

1. **SSRF Guard**:
   - Output download URIs are strictly validated against `https://replicate.delivery` and `*.replicate.delivery`.
   - `api.replicate.com` and generic web/IP endpoints are rejected.
2. **Crash-Window Promotion**:
   - Downloads occur to `.partial.mp4`. If process restarts during `ValidatingOutput` or download completion, the artifact is validated and promoted without duplicate provider calls.
3. **Paid Live Guard**:
   - All live predictions and uploads require explicit `ALLOW_PAID_LIVE_TEST=1`. In default mode, `PAID_LIVE_TEST_DISABLED` halts submissions before reaching the network.
