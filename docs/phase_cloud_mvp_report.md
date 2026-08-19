# AutoVideo AI — Phase Cloud MVP Report
## Real Cloud Video Generation, Fast Path & Cost-Aware Routing

---

## 1. Executive Summary

Phase Cloud MVP introduces the **first production-grade Cloud AI Video Generation Subsystem** into AutoVideo AI.

- **Primary Product Direction**: Transition from local-only GPU inference to **Cloud-First Generation** with local pre/post-processing and graceful local fallback.
- **Hardware Independence**: Cloud generation operates independently of local GPU VRAM constraints (e.g. low-end GTX 1650 or CPU-only machines can perform high-fidelity video generation via cloud).
- **Zero-Fake Enforcement**: Full transparency on credentials and execution. Unconfigured provider states strictly return `REAL_CLOUD_MVP_BLOCKED` with `status: UNKNOWN` cost without fabricating synthetic MP4s or mock responses.

---

## 2. Provider & Model Selection

- **Chosen Provider**: **Replicate**
- **Selected Model**: `minimax/video-01` (Alternative: `stability-ai/stable-video-diffusion`)
- **API Endpoint**: `https://api.replicate.com/v1/predictions`
- **Selection Rationale**: Standard REST endpoints, streaming polling, direct HTTPS MP4 download URLs, and predictable compute billing ($0.04/sec). Documented in [cloud_provider_selection.md](file:///d:/rustProject/autovideo-ai/docs/cloud_provider_selection.md).

---

## 3. Architecture & Data Flow

Detailed in [cloud_mvp_architecture.md](file:///d:/rustProject/autovideo-ai/docs/cloud_mvp_architecture.md):

1. **`CloudVideoProvider` Trait**: Unified abstraction for submitting jobs, polling status, cancelling predictions, and downloading artifacts.
2. **`GenerationRouter`**: Implements `AUTO` (Cloud-First for video generation), `CLOUD` (Strict Cloud, no silent local fallback), and `LOCAL` (Local execution).
3. **`CostGuard` & Cost Estimator**: Pre-submission budget validation against `max_cost_per_job` (default `$5.00`), preventing unauthorized cloud spend with `CLOUD_COST_LIMIT_EXCEEDED`.
4. **`LatencyTelemetry`**: Precision tracking from request initialization ($T_0$) to job submission ($T_1$), worker processing ($T_2$), remote completion ($T_3$), artifact download ($T_4$), and FFprobe validation ($T_5$).
5. **`SegmentPlanner`**: Partitions source videos into 4.0–8.0 second chunks for cloud processing.

---

## 4. Files Modified and Created

### Backend Core (`src-tauri/src/ai/cloud/`)
- [x] [`src-tauri/src/ai/cloud/error.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/error.rs): Machine-readable error taxonomy (`CLOUD_PROVIDER_UNAVAILABLE`, `CLOUD_AUTH_FAILED`, `CLOUD_COST_LIMIT_EXCEEDED`, `CLOUD_RATE_LIMITED`, `CLOUD_TIMEOUT`, `CLOUD_JOB_FAILED`, etc.).
- [x] [`src-tauri/src/ai/cloud/cost.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/cost.rs): `CostEstimate`, `CostStatus`, `CostGuard`, and `LatencyTelemetry`.
- [x] [`src-tauri/src/ai/cloud/job.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/job.rs): Asynchronous job lifecycle and `CloudJobManager`.
- [x] [`src-tauri/src/ai/cloud/segment.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/segment.rs): `VideoSegment` and `SegmentPlanner`.
- [x] [`src-tauri/src/ai/cloud/provider.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/provider.rs): `CloudVideoProvider` trait, `ProviderCapabilities`, `RemotePollResponse`.
- [x] [`src-tauri/src/ai/cloud/providers/replicate.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/providers/replicate.rs): Authenticated async client communicating with Replicate REST API.
- [x] [`src-tauri/src/ai/cloud/router.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/router.rs): `GenerationRouter` arbitrating `AUTO`, `CLOUD`, and `LOCAL` execution modes.
- [x] [`src-tauri/src/ai/cloud/mod.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/mod.rs): Re-exports.
- [x] [`src-tauri/src/ai/tests_cloud_mvp.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/tests_cloud_mvp.rs): 13 dedicated unit, contract, routing, and discovery tests.

### Tauri Commands & IPC
- [x] [`src-tauri/src/commands/mod.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/commands/mod.rs): Added `get_cloud_cost_estimate`, `get_generation_route`, `start_cloud_generation`, `get_cloud_job_status`, `cancel_cloud_generation`.
- [x] [`src-tauri/src/lib.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/lib.rs): Registered cloud commands in `invoke_handler`.
- [x] [`src/lib/ipc.ts`](file:///d:/rustProject/autovideo-ai/src/lib/ipc.ts): Exposed `cloudApi` TypeScript client interface.

### Project Dependencies
- [x] [`src-tauri/Cargo.toml`](file:///d:/rustProject/autovideo-ai/src-tauri/Cargo.toml): Added `reqwest` with `rustls-tls` and `json` features.

---

## 5. Security & Secret Protection

- `REPLICATE_API_TOKEN` is retrieved strictly from runtime environment variables.
- Secret tokens are **never** committed to version control, logged in diagnostics, stored in test fixtures, or sent to the frontend UI.
- All outbound authenticated requests are dispatched exclusively through the Rust backend.

---

## 6. Test & Regression Results

### Cloud MVP Test Suite
```
running 13 tests
test ai::tests_cloud_mvp::tests::test_cloud_01_provider_capabilities ... ok
test ai::tests_cloud_mvp::tests::test_cloud_02_cost_estimation_deterministic ... ok
test ai::tests_cloud_mvp::tests::test_cloud_03_cost_guard_budget_limit ... ok
test ai::tests_cloud_mvp::tests::test_cloud_04_latency_telemetry_tracking ... ok
test ai::tests_cloud_mvp::tests::test_cloud_05_job_state_machine_transitions ... ok
test ai::tests_cloud_mvp::tests::test_cloud_06_video_segment_planner ... ok
test ai::tests_cloud_mvp::tests::test_cloud_07_router_auto_mode_cloud_first ... ok
test ai::tests_cloud_mvp::tests::test_cloud_08_router_auto_mode_local_fallback ... ok
test ai::tests_cloud_mvp::tests::test_cloud_09_router_cloud_mode_strict_rejection ... ok
test ai::tests_cloud_mvp::tests::test_cloud_10_router_local_mode_explicit ... ok
test ai::tests_cloud_mvp::tests::test_cloud_11_error_taxonomy_serialization ... ok
test ai::tests_cloud_mvp::tests::test_cloud_12_replicate_response_status_parsing ... ok
test ai::tests_cloud_mvp::tests::test_cloud_13_real_cloud_acceptance_status_discovery ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; finished in 0.00s
```

### Full System Quality Checks
- `cargo fmt -- --check` $\rightarrow$ **PASS**
- `cargo check --all-targets` $\rightarrow$ **PASS (0 errors, 0 warnings)**
- `cargo test --test-threads=1` $\rightarrow$ **PASS (612 passed; 0 failed)**
- `npm run build` $\rightarrow$ **PASS (1859 modules transformed in 15.65s)**

---

## 7. Real Cloud Execution Acceptance Result

- **Execution Script**: [`src-tauri/scripts/cloud_mvp_acceptance.py`](file:///d:/rustProject/autovideo-ai/src-tauri/scripts/cloud_mvp_acceptance.py)
- **Report Path**: [`outputs/cloud_mvp/acceptance/report.json`](file:///d:/rustProject/autovideo-ai/outputs/cloud_mvp/acceptance/report.json)
- **Metadata Path**: [`outputs/cloud_mvp/acceptance/metadata.json`](file:///d:/rustProject/autovideo-ai/outputs/cloud_mvp/acceptance/metadata.json)
- **Discovery Status**: `REPLICATE_API_TOKEN` environment variable was not supplied in the local execution environment.
- **Zero-Fake Compliance**: Marked as `REAL_CLOUD_MVP_BLOCKED` without fabricating mock video files or fake price telemetry.

---

## 8. Final Classification

**STATUS: `REAL_CLOUD_MVP_BLOCKED`**

### Summary:
- The entire Cloud AI architecture, provider abstraction, asynchronous polling engine, cost guard, routing decision engine, and Tauri IPC integration are **100% implemented, compiled, and verified via automated tests**.
- In strict adherence to **Rule 0.1 (Zero-Fake)**, the live end-to-end cloud acceptance run is classified as `REAL_CLOUD_MVP_BLOCKED` until the user configures the `REPLICATE_API_TOKEN` environment variable with an active API token.
