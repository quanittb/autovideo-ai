# Phase FLOW-P3-A.4 Report: Final Single-Use Paid Boundary Audit & Fail-Closed Live Cost

## 1. Executive Summary & Core Invariants
In Phase FLOW-P3-A.4, the Google Flow automation paid boundary was audited and hardened with fail-closed defenses:
- **Zero Paid Clicks, Zero Live Generations, Zero Spent**:
  - `FLOW_PAID_CLICKS = 0`
  - `FLOW_LIVE_GENERATIONS = 0`
  - `FLOW_CREDITS_SPENT = 0`
- **Atomic Single-Use Preflight Tickets**:
  - Replaced lookup-then-consume with atomic `consume_ticket(preflight_id)`.
  - Concurrency hardened: when two concurrent tasks attempt to consume the same preflight ticket, at most one can succeed.
  - Second use yields `FLOW_PREFLIGHT_ALREADY_CONSUMED` and rejects before launching any browser or sidecar process.
- **Fail-Closed TTL DateTime Comparison**:
  - Replaced lexicographical RFC3339 string comparison with parsed `chrono::DateTime<Utc>`.
  - Invalid, unparseable, or expired timestamps immediately reject with `FLOW_PREFLIGHT_STALE`.
- **Full Configuration Verification**:
  - `configuration_verified` in preflight requires all 5 configuration dimensions to match: Model (`Omni Flash`), Resolution (`720p`), Generation Length (`10s`), Orientation (`PORTRAIT / 9:16`), and Output Count (`1`).
  - Canonical normalization implemented for orientation (e.g. `PORTRAIT / 9:16`, `9:16`, `portrait`), model (`omni flash`), and resolution (`720p`).
- **Zero Numeric Cost Fallback**:
  - Removed all `.unwrap_or(20)` and static integer fallbacks from prepare and submit execution paths.
  - `live_displayed_credit_cost` is mandatory; missing cost immediately causes `FLOW_LIVE_COST_UNVERIFIED` (`click_dispatched = false`, reserved credits rolled back).
- **Pre-Click Revalidation Guard**:
  - Immediately before dispatching the Generate click, the sidecar re-inspects active DOM state:
    1. Revalidates expected config (`model`, `resolution`, `durationSec`, `orientation`, `outputCount`) $\to$ `FLOW_CONFIGURATION_UNVERIFIED`.
    2. Re-computes canonical SHA-256 fingerprint $\to$ `FLOW_CONFIGURATION_CHANGED` on mismatch.
    3. Re-reads live cost $\to$ `FLOW_LIVE_COST_UNVERIFIED` if null, `FLOW_LIVE_COST_CHANGED` if differs from prepare.
    4. Budget check: `currentLiveCost <= maxCredits` $\to$ `FLOW_CREDIT_BUDGET_EXCEEDED` on violation.
    5. In all rejection cases: `clickDispatched = false`.
- **Account Balance Provenance Hardening**:
  - Completely deleted generic `body.innerText()` fallback.
  - Scoped regex requires credit/credits/tín dụng markers.
  - Balance returns `None` and `source = Unknown` if no dedicated account-balance element is proven.
- **Transport Error Classification**:
  - Network/RPC errors prior to click dispatch return `Failed` with `click_dispatched = false` (NOT `GenerationAmbiguous`).

---

## 2. Architecture & Design Decisions

### A. Atomic Ticket Consumption Flow
1. **Cheap Backend Validations First**:
   - `max_credits` presence and non-zero check.
   - `preflight_id` and `configuration_fingerprint` presence.
   - Project, profile, media matching.
   - DateTime TTL parse and expiration verification (`now > expires_at`).
   - Source media probe and segmentation feasibility.
2. **Atomic Pre-Job Consumption**:
   - Ticket is atomically removed from `FlowPreflightTicketStore` via `store.consume_ticket(preflight_id)`.
   - Concurrent calls fail immediately with `FLOW_PREFLIGHT_ALREADY_CONSUMED`.
   - No browser session or sidecar instance is spawned on replay or duplicate starts.

### B. Canonical Normalization & Fingerprint Construction
- Canonical normalizers standardize orientation strings:
  - `"PORTRAIT / 9:16"`, `"PORTRAIT"`, `"9:16"`, `"vertical"` $\to$ `"PORTRAIT / 9:16"`.
  - `"LANDSCAPE / 16:9"`, `"LANDSCAPE"`, `"16:9"`, `"horizontal"` $\to$ `"LANDSCAPE / 16:9"`.
  - `"SQUARE / 1:1"`, `"SQUARE"`, `"1:1"` $\to$ `"SQUARE / 1:1"`.
- Canonical fingerprint string format:
  `{operationContext}:{sourceIdentity}:{promptHash}:{normModel}:{normRes}:{durationSec}:{normOri}:{outputCount}`
- Encoded with SHA-256 hex digest and verified across both preflight and pre-click submission boundaries.

### C. Fail-Closed Live Cost Tooltip Parsing
- Tooltip reader requires scoped keywords:
  - English: `(\d+)\s*(?:credits?|tokens?)`
  - Vietnamese: `(\d+)\s*(?:tín dụng)` (and UTF-8 mojibake variants)
- Unrelated numbers or generic headers without explicit credit units are strictly ignored.

---

## 3. Files Modified

| File | Changes |
|------|---------|
| `src-tauri/sidecars/flow-playwright/src/flow_adapter.ts` | Added canonical normalization, deterministic SHA-256 fingerprinting, pre-click config & cost revalidation, deleted `body.innerText()` fallback in `readCreditBalance`, added `readLiveCostTooltip`. |
| `src-tauri/src/ai/flow/capability.rs` | Added `normalize_canonical_orientation`, `normalize_canonical_model`, `normalize_canonical_resolution`. Documented `CAPABILITY_CACHE_PERSISTENCE: IN_MEMORY`. |
| `src-tauri/src/ai/flow/orchestrator.rs` | Added `consume_ticket` for atomic removal; full duration & orientation config verification; RFC3339 DateTime comparison; zero numeric fallback (fail closed with `FLOW_LIVE_COST_UNVERIFIED`); rollback on `PreClickRejected`. |
| `src-tauri/src/ai/flow/playwright_bridge.rs` | Passed `promptHash` and `sourceIdentity` in `submit_prepared`. |
| `src-tauri/src/ai/flow/mock_flow_server.rs` | Added explicit UTF-8 `Content-Type` header to HTTP responses. |
| `src-tauri/src/ai/tests_phase_flow_p3a.rs` | Added 17 new comprehensive test cases (tests 26 to 42, A through Q). |

---

## 4. Test Execution & Verified Results

### Automated Test Suites

1. **Rust Code Formatting** (`cargo fmt --check`):
   - **Passed** with 0 diffs.
2. **Rust Compilation** (`cargo check`):
   - **Passed** with 0 warnings/errors.
3. **Phase Flow P3A Suite** (`cargo test --lib -- tests_phase_flow_p3a --test-threads=1`):
   - **42 / 42 passed**, 0 failed, 2 ignored (live acceptance).
   - Covered all required test scenarios A through Q:
     - Case A (Test 26): Single-use ticket rejected on second use (`FLOW_PREFLIGHT_ALREADY_CONSUMED`).
     - Case B (Test 27): Concurrent starts on single ticket: exactly one succeeds.
     - Case C (Test 28): Expired ticket DateTime comparison fails closed (`FLOW_PREFLIGHT_STALE`).
     - Case D (Test 29): Preflight duration mismatch fails `configuration_verified`.
     - Case E (Test 30): Preflight orientation mismatch fails `configuration_verified`.
     - Case F (Test 31): Prepare live cost `None` $\to$ zero fallback, click not dispatched.
     - Case G (Test 32): Submit final live cost `None` $\to$ `PRE_CLICK_REJECTED`.
     - Case H (Test 33): Prepared fingerprint mismatch $\to$ `PRE_CLICK_REJECTED`.
     - Case I (Test 34): Model mismatch $\to$ `PRE_CLICK_REJECTED`.
     - Case J (Test 35): Resolution mismatch $\to$ `PRE_CLICK_REJECTED`.
     - Case K (Test 36): Duration mismatch $\to$ `PRE_CLICK_REJECTED`.
     - Case L (Test 37): Orientation mismatch $\to$ `PRE_CLICK_REJECTED`.
     - Case M (Test 38): Output count mismatch $\to$ `PRE_CLICK_REJECTED`.
     - Case N (Test 39): Live cost changed between prepare and submit $\to$ `PRE_CLICK_REJECTED`.
     - Case O (Test 40): Balance probe ignores generic body generation cost $\to$ `balance: None`, `source: Unknown`.
     - Case P (Test 41): Pre-click transport error is not ambiguous (`click_dispatched: false`).
     - Case Q (Test 42): Post-click transport loss is ambiguous (`click_dispatched: true`).
4. **Prompt Tests Suite** (`cargo test --lib -- prompt_tests --test-threads=1`):
   - **32 / 32 passed**.
5. **Phase 20a Security & Mock Suite** (`cargo test --lib -- tests_phase20a --test-threads=1`):
   - **78 / 78 passed**.
6. **Phase 20b Lifecycle Suite** (`cargo test --lib -- tests_phase20b --test-threads=1`):
   - **27 / 27 passed**.
7. **Phase 20c Face Benchmarks Suite** (`cargo test --lib -- tests_phase20c --test-threads=1`):
   - **13 / 13 passed**.
8. **Phase Flow P2 Suite** (`cargo test --lib -- tests_phase_flow_p2 --test-threads=1`):
   - **5 / 5 passed**.
9. **Frontend Vitest Suite** (`npm test`):
   - **61 / 61 passed** across 7 test files.
10. **Sidecar & Frontend Production Builds**:
    - `flow-playwright` sidecar `npm run build`: **Passed** (clean `tsc`).
    - App frontend `npm run build`: **Passed** (`vite v7.3.6`, built in 6.15s).

---

## 5. Live Acceptance Verification (0 Clicks, 0 Spent)

### A. Live Credit Balance Refresh Acceptance
- Command: `cargo test --lib test_flow_p3a_real_google_flow_live_credit_refresh_acceptance --% -- --ignored --nocapture`
- Result:
  ```
  ==================================================
  [FLOW-P3-A.3 LIVE CREDIT REFRESH] Starting real non-submitting refresh for profile_2...
  Invariants: 0 video uploads, 0 generate clicks, 0 paid submissions, 0 credits spent.
  ==================================================
  FLOW-P3-A.3 LIVE CREDIT REFRESH ACCEPTED FACTS:
  Profile ID: profile_2
  Credit Status: Ready
  Live Balance: None
  Source: Unknown
  Checked At: 2026-08-26T06:54:18.340743900+00:00
  Paid Clicks: 0 (GUARANTEED: refresh path cannot submit)
  Credits Spent: 0
  ==================================================
  test result: ok. 1 passed; 0 failed
  ```
- **Validation**:
  - `Live Balance: None`, `Source: Unknown`: Verifies the removal of `body.innerText()` fallback. Without an explicit account-balance UI element, the parser safely returns `None` instead of misattributing unrelated numbers.
  - Zero paid clicks, zero credits spent.

### B. Live Preflight Acceptance
- Command: `cargo test --lib test_flow_p3a_real_google_flow_live_preflight_acceptance --% -- --ignored --nocapture`
- Result:
  ```
  ==================================================
  FLOW-P3-A LIVE PREFLIGHT RESULT:
  Project ID: proj-824c1ae7-7658-4a0d-8665-bd0960d53862
  Source Media ID: media_454f7d2c-5fce-42d6-a096-873cc8c13259
  Profile ID: profile_2
  Transformation Intent: FaceReplace
  Identity Mode: Generated
  Prompt Source: SystemDefault
  Resolved Prompt: Replace only the selected target person's facial identity with a new, temporally consistent synthetic identity. Strictly preserve: body, clothing, hair where practical, pose, expression dynamics, mouth movement, head movement, action, camera motion, background, lighting, composition, timing, and all non-target people.
  Prompt Hash: 2e39321365a792f3f735938d88165d0cf1e486fa71b33172c79bc16c448215ef
  Video Attached: true
  Video Edit Active: true
  Config Verified: true
  Cost Provenance: UploadedVideoEdit
  Observed Source Title: Some("flow_acceptance_01.mp4")
  Observed Source Duration: Some(9.767)
  Observed Model: Some("Omni Flash")
  Observed Resolution: Some("720p")
  Observed Orientation: Some("PORTRAIT / 9:16")
  Observed Output Count: Some(1)
  Observed Generation Length: Some(10.0)
  Live Displayed Credit Cost: Some(20)
  Diagnostic Composer Credit Cost: None
  Live Credit Balance: None
  Configuration Fingerprint: f8f0b6502e2ba2b60d1aa296dd65540b03bcf8d649187bc0da820e4ac0470c91
  Ready For Paid Submission: true
  Blocking Code: None
  Checked At: 2026-08-26T06:55:20.842457900+00:00
  ==================================================
  test result: ok. 1 passed; 0 failed
  ```
- **Validation**:
  - `Config Verified: true`: All 5 configuration attributes verified in the active Google Flow edit workspace.
  - `Live Displayed Credit Cost: Some(20)`: Directly extracted from the active edit workspace tooltip.
  - Zero clicks dispatched, zero credits spent.

---

## 6. Accounting Summary
- `FLOW_PAID_CLICKS = 0`
- `FLOW_LIVE_GENERATIONS = 0`
- `FLOW_CREDITS_SPENT = 0`
- `CAPABILITY_CACHE_PERSISTENCE = IN_MEMORY`

## 7. Remaining Limitations
- Live credit balance remains `None` (`source: Unknown`) when Flow's account credit popover is not opened, strictly preserving zero-fake policy.
- FLOW-P3-B (live paid submission dispatch) has NOT been started.
