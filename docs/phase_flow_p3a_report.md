# Phase FLOW-P3-A.3 Report: Paid Boundary Hardening & Live Cost Enforcement

## 1. Executive Summary & Core Invariants
In Phase FLOW-P3-A.3, the Google Flow automation pipeline was hardened against unverified assumptions, premature charges, and capability drift:
- **Zero Paid Clicks & Zero Spent**: Total paid clicks = 0, live generations = 0, Google Flow credits spent = 0.
- **True Live Cost Authority**: Replaced static planning estimates (e.g. 40 credits) with live displayed cost (20 credits) observed directly on the active uploaded video edit workspace.
- **Strict Preflight Authorization Tickets**: Issued short-lived (300s / 5min TTL) tickets (`FlowPreflightTicket`) bound to deterministic configuration fingerprints (`FlowConfigurationFingerprint`). Paid submission is strictly rejected without valid ticket and explicit `maxCredits`.
- **Two-Phase Submission Safety**: Decoupled submission into `prepare_video_edit` (non-modifying DOM preparation and live cost extraction) and `submit_prepared` (single guarded click with immediate semantic confirmation).
- **Correct Capability Provenance**: Capability observations are strictly segregated by `FlowCapabilityContext` (`UploadedVideoEdit` vs `TextToVideo`). Verified that 1080p is NOT proven for uploaded video edits and is never labeled as cached/live until real evidence is observed.

---

## 2. Architecture & Design Decisions

### A. Capability Provenance & Observation Store
- Implemented `FlowCapabilityObservationStore` keyed by `(profile_id, operation_context)`.
- `FlowCapabilityObservation` captures actual runtime observations: `model_id`, `supported_resolutions`, `supported_durations_sec`, `supported_orientations`, `supported_output_counts`, `observed_at`, and `adapter_version`.
- If no live observation exists for `(profile_id, UploadedVideoEdit)`, a clean `StaticFallback` is provided exposing only proven defaults (`Omni Flash`, `720p`, `10s`, `PORTRAIT`, `x1`) with a fixed historical sentinel timestamp `2026-08-26T00:00:00Z` (preventing synthetic freshness fabrication).

### B. Preflight Tickets & Paid Submission Boundary
- `FlowPreflightTicket` structure:
  - `preflight_id`: `pf_<uuid>`
  - `project_id`, `profile_id`, `source_media_id`
  - `configuration_fingerprint`: SHA-256 over model, resolution, duration, orientation, output count, prompt hash, and audio mode
  - `live_displayed_cost`: Authoritative cost read directly from Flow UI
  - `issued_at`, `expires_at`: Enforced 5-minute (300s) TTL
- Orchestrator checks at generation launch:
  1. `max_credits` is required; missing -> `FLOW_CREDIT_BUDGET_REQUIRED`.
  2. `preflight_id` is required; missing -> `FLOW_PREFLIGHT_REQUIRED`.
  3. `configuration_fingerprint` is required; missing -> `FLOW_PREFLIGHT_REQUIRED`.
  4. Ticket existence, profile/project matching, and TTL expiration check -> `FLOW_PREFLIGHT_STALE`.
  5. Fingerprint match between ticket and request -> `FLOW_PREFLIGHT_STALE`.

### C. Split Submission Pipeline (`prepare` + `submit_prepared`)
- `prepareVideoEditSubmission` in Playwright sidecar:
  - Verifies `/edit/` workspace and uploaded video card attachment.
  - Applies prompt and generation configuration.
  - Reads authoritative live displayed cost and verifies `unit_cost <= max_credits`.
  - Computes and returns DOM-level `preparedFingerprint`.
  - **Zero click dispatch during prepare.**
- `submitPreparedVideoEdit` in Playwright sidecar:
  - Re-verifies live cost, maxCredits, and configuration fingerprint before clicking.
  - Clicks `Generate` button **exactly once**.
  - Observes post-click transitions across 20-second timeout.
  - Returns structured `FlowSubmissionOutcome`:
    - `PreClickRejected`: click was NOT dispatched (`click_dispatched: false`). UI changes, errors, or budget mismatches do not cause job ambiguity.
    - `ProvenSubmitted`: click was dispatched and verified via generating spinner or completion indicator.
    - `PostClickAmbiguous`: click was dispatched but UI did not transition within 20s window.

### D. Authoritative Cost Reservation & Credit Balance
- Orchestrator reserves `unit_live_cost` (e.g. 20) instead of the static estimate (40).
- If `PreClickRejected`, reserved credits are rolled back (`saturating_sub(unit_live_cost)`).
- `refresh_flow_credit_balance`:
  - Pure non-submitting inspection path.
  - Robust regex requiring credit keyword units prevents unrelated DOM numbers from contaminating user balance.

---

## 3. Files Modified

| File | Changes |
|------|---------|
| `src-tauri/sidecars/flow-playwright/src/flow_adapter.ts` | Added `prepareVideoEditSubmission`, `submitPreparedVideoEdit`, improved `readCreditBalance` and `parseLocalizedCreditNumber`. |
| `src-tauri/sidecars/flow-playwright/src/bridge.ts` | RPC routing for `prepare_video_edit_submission` and `submit_prepared_video_edit`. |
| `src-tauri/src/ai/flow/capability.rs` | Observation store, context separation, clean static fallback with 720p only. |
| `src-tauri/src/ai/flow/orchestrator.rs` | Preflight tickets store, 5-minute TTL, strict validation, worker loop prepare + submit. |
| `src-tauri/src/ai/flow/playwright_bridge.rs` | Bridge types `PreparedFlowSubmission`, `FlowSubmissionOutcome`, session methods. |
| `src-tauri/src/ai/flow/mock_flow_server.rs` | Updated interactive mock workspace with true video edit tags and header credit element. |
| `src-tauri/src/ai/flow/mod.rs` | Re-exports for ticket store, observations, and submission outcomes. |
| `src/types/contracts.ts` | Added `preflightId`, `expiresAt` to preflight and request interfaces. |
| `src/features/flow/FlowGenPanel.tsx` | Propagated `preflightId` and `expiresAt` in preflight and start flow job. |
| `src/stores/flowJobStore.ts` | Added `preflightId` to `startFlowJob` options and request dispatch. |
| `src/stores/__tests__/flowJobStore.test.ts` | Updated mock preflight fixture with `preflightId` and `expiresAt`. |
| `src-tauri/src/ai/tests_phase20a/prompt_tests.rs` | Updated pipeline integration tests to issue tickets and validate submission outcomes. |
| `src-tauri/src/ai/tests_phase_flow_p2.rs` | Updated budget rejection tests for preflight tickets. |
| `src-tauri/src/ai/tests_phase_flow_p3a.rs` | Comprehensive suite of 25 unit/integration tests + real credit refresh acceptance test. |

---

## 4. Test Execution & Verified Results

### Automated Test Suites
1. **Frontend Vitest Suite** (`npm test`):
   - **61 / 61 passed** across 7 test files (`100% pass`).
2. **Frontend Production Build** (`npm run build`):
   - Passed with zero errors (`vite v7.3.6`, built in 6.01s).
3. **Rust Code Formatting** (`cargo fmt --check`):
   - Clean (`exit code 0`).
4. **Rust Compilation** (`cargo check`):
   - Clean (`exit code 0`, 6.32s).
5. **Prompt Tests Suite** (`cargo test --lib -- prompt_tests --test-threads=1`):
   - **32 / 32 passed** (`exit code 0`).
6. **Phase 20a Security & Mock Suite** (`cargo test --lib -- tests_phase20a --test-threads=1`):
   - **78 / 78 passed** (`exit code 0`).
7. **Phase 20b Lifecycle Suite** (`cargo test --lib -- tests_phase20b --test-threads=1`):
   - **27 / 27 passed** (`exit code 0`).
8. **Phase 20c Face Benchmarks Suite** (`cargo test --lib -- tests_phase20c --test-threads=1`):
   - **13 / 13 passed** (`exit code 0`).
9. **Phase Flow P2 Suite** (`cargo test --lib -- tests_phase_flow_p2 --test-threads=1`):
   - **5 / 5 passed** (`exit code 0`).
10. **Phase Flow P3A Suite** (`cargo test --lib -- tests_phase_flow_p3a --test-threads=1`):
    - **25 / 25 passed** (1 ignored: live acceptance runner).

### Live Acceptance Test (`test_flow_p3a_real_google_flow_live_credit_refresh_acceptance`)
- Profile: `profile_2`
- Command: `cargo test --lib test_flow_p3a_real_google_flow_live_credit_refresh_acceptance --% -- --ignored --nocapture`
- Result:
  ```
  [FLOW-P3-A.3 LIVE CREDIT REFRESH] Starting real non-submitting refresh for profile_2...
  Invariants: 0 video uploads, 0 generate clicks, 0 paid submissions, 0 credits spent.
  ==================================================
  FLOW-P3-A.3 LIVE CREDIT REFRESH ACCEPTED FACTS:
  Profile ID: profile_2
  Credit Status: Ready
  Live Balance: None
  Source: Unknown
  Checked At: 2026-08-26T04:57:54.605590800+00:00
  Paid Clicks: 0 (GUARANTEED: refresh path cannot submit)
  Credits Spent: 0
  ==================================================
  test ai::tests_phase_flow_p3a::test_flow_p3a_real_google_flow_live_credit_refresh_acceptance ... ok
  ```
- **Cost Incurred**: $0.00 / 0 credits.
- **Paid Clicks**: 0.

---

## 5. Remaining Limitations
- While `profile_2` authenticated session was successfully validated as `Ready` without submitting, Google Flow's current web UI does not expose credit balance in an unstyled top-level header text element when the project workspace is open without active popovers. Live credit balances will continue to report `None` until observed via an opened popover or until Flow exposes an explicit header metric.
- FLOW-P3-B (live generation with real paid clicks) remains deliberately untouched as required by user directive.
