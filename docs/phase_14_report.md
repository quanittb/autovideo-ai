# AutoVideo AI — Phase 14 Report
## Task-Specific, Capability-Aware & Cost-Aware Routing Model

---

## 1. Executive Summary

Phase 14 replaces broad cloud-first routing with a unified **Task-Specific, Capability-Aware, and Cost-Aware Routing Model**. The system strictly prioritizes **local deterministic processing** (e.g. FFmpeg) whenever it can satisfy the user request, reserves specialized cloud video engines for complex generative transformations, and enforces authoritative backend budget guards.

---

## 2. Architecture & Domain Model

### 2.1 Execution Classes (`ExecutionClass`)
- `LOCAL_DETERMINISTIC`: Universal zero-cost offline operations (FFmpeg / native media engine).
- `UTILITY_CLOUD`: Low-cost utility models (e.g. background removal, BiRefNet).
- `SPECIALIZED_VIDEO_TRANSFORMATION`: High-fidelity cloud video models (e.g. Replicate Minimax Video-01).
- `GENERATIVE_FALLBACK`: Local neural generative inference (SD1.5 / AnimateDiff).
- `LOCAL_EXPERIMENTAL`: Experimental local pipelines.

### 2.2 Task Classes (`TaskClass`)
- `CHARACTER_REPLACEMENT`: Routes to `SPECIALIZED_VIDEO_TRANSFORMATION` (defaults to 720p).
- `BACKGROUND_REMOVAL`: Routes to `UTILITY_CLOUD` / local ONNX utility.
- `BACKGROUND_COMPOSITE`: Routes to `LOCAL_DETERMINISTIC` ($0.00 local FFmpeg).
- `STYLE_FILTER`: Routes to `LOCAL_DETERMINISTIC` ($0.00 local FFmpeg).
- `AUDIO_TRANSFORMATION`: Routes to `LOCAL_DETERMINISTIC` ($0.00 local media engine).
- `ACTION_REGENERATION`: Routes to `SPECIALIZED_VIDEO_TRANSFORMATION`.
- `FULL_GENERATIVE_TRANSFORMATION`: Blocked in `COST_SAVING` mode to protect budget unless explicitly approved.

### 2.3 Single Unified `ProviderRegistry`
Located in [`src-tauri/src/ai/cloud/registry.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/registry.rs):
- Stores provider ID, model ID, version policy, execution class, capabilities, resolution/FPS limits, pricing metadata (`pricing_unit`, `pricing_amount`, `currency`, `source_url`, `observed_at`).
- Dynamic price updating (`registry.update_price(...)`) allows runtime price updates without modifying routing code.

### 2.4 Structured `CostBreakdown` & Authoritative Budget Enforcement
Located in [`src-tauri/src/ai/cloud/cost.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/cost.rs):
- Details billable duration, resolution, segment count, overlap duration, retry allowance, inference cost, transfer cost, total cost, and confidence (`EXACT`, `ESTIMATED`, `UNKNOWN`).
- `CostConfidence::Unknown` / missing price strictly blocks auto-submission.
- Authoritative backend `CostGuard`:
  - Default Preview Budget: **USD 0.25**
  - Default Standard Job Budget: **USD 3.00**
  - Exact boundary ($3.00 / $3.00) passes; one cent over ($3.01 / $3.00) fails with `CLOUD_COST_LIMIT_EXCEEDED`.

---

## 3. Files Modified and Created

### Backend Rust Core
- [x] [`src-tauri/src/ai/cloud/registry.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/registry.rs): Single `ProviderRegistry`, `ExecutionClass`, `PricingUnit`, `ProviderRecord`.
- [x] [`src-tauri/src/ai/cloud/cost.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/cost.rs): `CostBreakdown`, `CostConfidence`, `CostGuard` with authoritative budget thresholds.
- [x] [`src-tauri/src/ai/cloud/router.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/router.rs): `TaskClass`, `RoutingPreference`, capability & resolution/FPS validation, local deterministic preference.
- [x] [`src-tauri/src/ai/cloud/mod.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/mod.rs): Clean re-exports for Phase 14 domain types.
- [x] [`src-tauri/src/ai/tests_cloud_mvp.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/tests_cloud_mvp.rs): 16 comprehensive tests including 10 dedicated Phase 14 mandatory tests.

### Frontend TypeScript Bridge
- [x] [`src/lib/ipc.ts`](file:///d:/rustProject/autovideo-ai/src/lib/ipc.ts): Aligned `ExecutionClass`, `TaskClass`, `RoutingPreference`, `CostBreakdown`, `CostConfidence`, and `RoutingDecision` interfaces.

---

## 4. Test Suite Execution & Real Results

```
Testing Environment:
- Host: Windows 11 x86_64
- Rust toolchain: 1.84.0
- Starting HEAD: 08a8c5e6fbdf54fc6f4b3aeaf6fe3732dcabc731

Test Results:
- cargo fmt --manifest-path src-tauri/Cargo.toml -- --check -> PASS (0 diffs)
- cargo check --manifest-path src-tauri/Cargo.toml --all-targets -> PASS (0 errors, 0 warnings)
- cargo test --manifest-path src-tauri/Cargo.toml -- test_phase14 --test-threads=1 -> PASS (10 passed; 0 failed)
- cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1 -> PASS (615 passed; 0 failed; 0 ignored)
- npm.cmd run build -> PASS (1859 modules built in 14.81s)
```

### Verified Test Matrix:
1. `test_phase14_01_task_execution_classes` $\rightarrow$ PASS
2. `test_phase14_02_local_tasks_never_route_to_paid_providers_in_cost_saving` $\rightarrow$ PASS
3. `test_phase14_03_capability_resolution_mismatch_rejected` $\rightarrow$ PASS
4. `test_phase14_04_unsupported_fps_rejected` $\rightarrow$ PASS
5. `test_phase14_05_exact_budget_boundary_passes` $\rightarrow$ PASS
6. `test_phase14_06_one_cent_over_budget_fails` $\rightarrow$ PASS
7. `test_phase14_07_unknown_price_blocks_submission` $\rightarrow$ PASS
8. `test_phase14_08_disabled_full_generative_blocks_submission_in_cost_saving` $\rightarrow$ PASS
9. `test_phase14_09_serialized_project_data_backward_compatibility` $\rightarrow$ PASS
10. `test_phase14_10_provider_registry_price_refresh` $\rightarrow$ PASS

---

## 5. Incurred Cost

- **Live Paid Cloud Calls**: `$0.00` (Zero paid cloud calls made during Phase 14).

---

## 6. Final Status

**STATUS: `PHASE_COMPLETED`**
