# Phase FLOW-P1 Report: Flow Generation Productization & Gemini Sentinel Fix

## 1. Executive Summary & Acceptance Decision

Phase FLOW-P1 promotes Google Flow video generation/editing from an isolated benchmark experiment into a **first-class production feature** of AutoVideo AI. PRUNA and BRIA cloud providers are safely deferred without blocking development.

- **Zero-Fake / Zero-Paid Accounting**:
  - `PRUNA_CALLS`: `0`
  - `BRIA_CALLS`: `0`
  - `FLOW_LIVE_GENERATIONS`: `0`
  - `GEMINI_LIVE_PAID_CALLS`: `0`
- **Quality Gates & Test Results**:
  - `cargo check`: PASSED (0 errors, 0 warnings)
  - `cargo fmt --check`: PASSED
  - `prompt_tests`: 32/32 tests PASSED
  - `tests_phase20a`: 77/77 tests PASSED
  - `tests_phase20b`: 27/27 tests PASSED
  - `tests_phase20c`: 13/13 tests PASSED
  - Frontend Vitest suite: 56/56 tests PASSED
  - Frontend build (`pnpm build` / `tsc && vite build`): PASSED

---

## 2. Architectural Changes & Domain Model

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                               Frontend (React + Zustand)                               │
│  - FlowGenPanel: Intent selector (FACE_REPLACE default), budget limit input, auth alert│
│  - FlowJobProgress: Human-readable stage badges, Cancel button, Output Action buttons  │
│  - useFlowJobStore: FlowGenerationRequest, cancelFlowJob, derived output actions       │
└────────────────────────────────────────┬───────────────────────────────────────────────┘
                                         │ Tauri IPC (start_flow_generation, cancel, etc.)
                                         ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                              Tauri App State & Runtime                                 │
│  - Arc<FlowRuntimeService>: shared runtime managing orchestrator & cancellations       │
│  - Arc<FlowCancellationRegistry>: RwLock<HashSet<String>> for cooperative cancellation │
│  - Canonical Media Path Confinement: project media root enforcement (projectId/media)  │
└────────────────────────────────────────┬───────────────────────────────────────────────┘
                                         │
┌────────────────────────────────────────┴───────────────────────────────────────────────┐
│                           Rust Backend Core (src-tauri/src/ai)                         │
│  - transformation.rs: Canonical Domain Types (TransformationIntent, IdentityMode, etc.)│
│  - prompt_optimizer.rs: GEMINI_API_KEY_SENTINEL ("Axxxxxxxxxxx") immutable sentinel    │
│  - manifest.rs: Schema v2 with credit_budget_limit, reserved_credits, derived snapshot │
│  - orchestrator.rs: 11 Cancellation Checkpoints, Pre-Click Credit Budget Preflight     │
│  - store.rs: Atomic manifest persistence, cancel_job mutation                          │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.1. Domain Transformation Types Extraction
- Extracted `TransformationIntent`, `IdentityMode`, `TargetFaceSelection`, `TargetFaceCandidate`, and `TargetFacePolicy` into canonical module `src-tauri/src/ai/transformation.rs`.
- Re-exported domain types across `crate::ai` and `crate::ai::flow`.

### 2.2. Gemini Sentinel & Resolution Contract
- Defined immutable `GEMINI_API_KEY_SENTINEL = "Axxxxxxxxxxx"`.
- Fixed `is_valid_gemini_key`: rejects exact `GEMINI_API_KEY_SENTINEL` and standard placeholders (`"your_api_key_here"`, `"PLACEHOLDER"`, `""`), without incorrect `starts_with("Axxxx")` or equality checks against `DEFAULT_GEMINI_API_KEY`.
- Verified credential lifecycle: `ApplicationDefault` $\rightarrow$ `UserOverride` $\rightarrow$ `ApplicationDefault` upon clearance.

### 2.3. Shared Flow Runtime Service & Cancellation Registry
- Established `FlowRuntimeService` containing `Arc<FlowOrchestrator>` and `Arc<FlowCancellationRegistry>`, managed once in Tauri application state.
- Embedded 11 cooperative cancellation checkpoints:
  1. Before profile lock
  2. Before video splitting
  3. After split, before browser launch
  4. After browser launch
  5. Before each segment submission
  6. Before credit preflight
  7. Immediately before local attempt persistence & click
  8. Inside polling loop
  9. Before download
  10. Before final stitching
  11. After final validation

### 2.4. Pre-Click Credit Budget Enforcement & Submission Outcome Contract
- Budget check occurs before local attempt persistence and browser click.
- Budget violation yields `PRE_CLICK_REJECTED` (`state = Blocked`, `FLOW_CREDIT_BUDGET_EXCEEDED`, `clickDispatched = false`).
- Unconfirmed browser generation post-click yields `GENERATION_AMBIGUOUS` with zero automatic retry.

### 2.5. Manifest Schema Version 2 & Derived Output Actions
- Updated schema version to `2`.
- Added `credit_budget_limit: Option<u32>` and `reserved_credits: u32` to `FlowCreditRecord`.
- Derived snapshot fields `final_output_path` and `error_code` from manifest structures, computing `total_segments = max(segment_plan.len(), child_segments.len())`.
- Provided Tauri IPC commands: `open_flow_output_artifact`, `reveal_flow_output_in_folder`, `use_flow_output_in_project` (which copies the completed output as `derived_flow_{jobId}.mp4` into the project media folder without mutating source media).

---

## 3. Files Modified

| File | Nature of Changes |
|---|---|
| `src-tauri/src/ai/transformation.rs` | **NEW**: Canonical transformation domain types (`TransformationIntent`, `IdentityMode`, etc.) |
| `src-tauri/src/ai/mod.rs` | Exported `transformation` module |
| `src-tauri/src/ai/phase20c.rs` | Re-exported domain types from `crate::ai::transformation` |
| `src-tauri/src/ai/flow/prompt_optimizer.rs` | Introduced `GEMINI_API_KEY_SENTINEL`, corrected `is_valid_gemini_key` |
| `src-tauri/src/ai/flow/capability.rs` | Added `credit_budget_limit` and `reserved_credits` to `FlowCreditRecord` |
| `src-tauri/src/ai/flow/manifest.rs` | Schema version 2, derived snapshot fields, accurate `total_segments` |
| `src-tauri/src/ai/flow/store.rs` | Added `cancel_job` implementation |
| `src-tauri/src/ai/flow/orchestrator.rs` | `FlowCancellationRegistry`, `FlowRuntimeService`, `FlowGenerationRequest`, 11 checkpoints, budget enforcement |
| `src-tauri/src/ai/flow/mock_flow_server.rs` | Real probing-valid mp4 generation for mock download validation |
| `src-tauri/src/ai/flow/mod.rs` | Exported `FlowCancellationRegistry`, `FlowGenerationRequest`, `FlowRuntimeService` |
| `src-tauri/src/commands/mod.rs` | Registered `start_flow_generation`, `cancel_flow_generation`, `get_flow_job_status`, `list_flow_jobs`, `open_flow_output_artifact`, `reveal_flow_output_in_folder`, `use_flow_output_in_project` |
| `src-tauri/src/lib.rs` | Registered `Arc<FlowRuntimeService>` in Tauri managed state and command handlers |
| `src-tauri/src/ai/tests_phase20a/manifest_tests.rs` | Fixed `FlowCreditRecord` struct initializers |
| `src-tauri/src/ai/tests_phase20a/security_mock_tests.rs` | Updated `run_flow_worker` invocation signatures |
| `src-tauri/src/ai/tests_phase20a/prompt_tests.rs` | Added Phase FLOW-P1 unit and integration test suite |
| `src/lib/ipc.ts` | Added `TransformationIntent`, `IdentityMode`, `FlowGenerationRequest`, updated `flowApi` |
| `src/stores/flowJobStore.ts` | Supported request options, cancellation, and output action methods |
| `src/features/flow/FlowGenPanel.tsx` | Added intent selector, budget input, login notice, corrected Gemini badge binding |
| `src/features/flow/FlowJobProgress.tsx` | Human-readable stage badges, Cancel button, Open/Reveal/Use output buttons |
| `src/features/settings/SettingsView.tsx` | Updated Gemini default key status description |

---

## 4. Test Verification Summary

### 4.1. Rust Test Suites
- **`prompt_tests` (32 tests)**:
  - `test_phase_flow_p1_01_gemini_sentinel_exact_rejection_and_real_key_acceptance`: PASSED
  - `test_phase_flow_p1_02_app_default_to_user_override_and_fallback_lifecycle`: PASSED
  - `test_phase_flow_p1_03_flow_production_request_e2e_acceptance`: PASSED
  - `test_phase_flow_p1_04_pre_click_budget_exceeded_rejects_before_click`: PASSED
  - `test_phase_flow_p1_05_flow_cancellation_stops_worker`: PASSED
- **`tests_phase20a` (77 tests)**: All 77 passed (0 failures).
- **`tests_phase20b` (27 tests)**: All 27 passed (0 failures).
- **`tests_phase20c` (13 tests)**: All 13 passed (0 failures).

### 4.2. Frontend Tests & Build
- **Vitest (56 tests across 6 files)**: All 56 passed (0 failures).
- **Vite Build (`tsc && vite build`)**: Clean build, 0 errors.

---

## 5. Next Steps

- Proceed to user verification of the integrated UI and Flow production pipeline.
- Continue tracking cloud provider tokens when PRUNA/BRIA credentials become available in a future phase.
