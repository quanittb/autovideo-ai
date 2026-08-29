# Phase FLOW-P4-B / FLOW-P4-B.2 Report: Google Flow Long-Video Production Acceptance & Accounting

## Executive Summary

Phase FLOW-P4-B evaluated the Google Flow two-segment long-video production pipeline using `profile_2` and `test-assets/p4b_source_15s.mp4`.
All executions strictly adhered to the Zero-Fake Policy and explicit human budget authorization guards:
- **Historical Failed Attempt**: 20 credits / 1 click
- **Clean Rerun Authorization**: Max additional 40 credits / Max 2 new clicks
- **Clean Rerun Actually Consumed**: 20 credits / 1 new click (Segment 0)
- **Remaining Clean-Rerun Authorization**: 20 credits / 1 new click
- **Maximum Overall Experimentation Ceiling**: 60 credits / 3 clicks (20 historical + 40 clean rerun)
- **Total Observed Experimentation Spend**: 40 credits / 2 clicks (20 historical + 20 clean rerun)
- **Auto-Retries**: 0

---

## 1. Dual-Ledger Total Accounting Model

| Ledger Component | Dispatched Paid Clicks | Authoritative Credits Spent | Outcome / Status |
| :--- | :---: | :---: | :--- |
| **Historical Failed Attempt #1** | 1 | 20 | `FAILED_PRECONDITION / WRONG_SOURCE_MEDIA_SELECTED` (Quarantined) |
| **Clean Rerun Consumed So Far** | 1 | 20 | `PRE_CLICK_VERIFIED / SEGMENT_0_DISPATCHED / GENERATION_TIMEOUT` |
| **Total Observed Spend So Far** | **2** | **40** | **ACTIVE EXPERIMENTATION (40/60 Max Ceiling)** |
| **Remaining Clean Authorization** | **1** | **20** | **AVAILABLE FOR OPERATOR INVOCATION** |
| **Maximum Overall Authorized Ceiling** | **3** | **60** | **ABSOLUTE COMBINED EXPERIMENTATION CAP** |

### Historical Attempt #1 (Quarantined)
* **Run ID**: `P4B_RUN_1`
* **Status**: Excluded from pipeline; zero reuse as Segment 0, Segment 1, stitch input, or `DerivedMediaAsset`.
* **Dispatched Paid Clicks**: 1
* **Credits Spent**: 20

### Clean Rerun Attempt
* **Parent Run ID**: `flow_0d2ba55e-029d-4188-a294-c7ebd8f567c6` (`proj-8e8c37f2-8d6d-4689-8e3c-bb86685f02fc`)
* **Segment 0 Attempt ID**: `att_flow_0d2ba55e-029d-4188-a294-c7ebd8f567c6_0_1787934601818`
* **Source Media**: `test-assets/p4b_source_15s.mp4` (SHA-256: `03390797b5787a923bfd703c53cf0cec64680451ab8c36dc6fc43f9e9e04ddab`)
* **Segment 0 Input**: `segment_000.mp4` (10.0s, 300 frames, 720p, Portrait 9:16)
* **Pre-Click Exact Media Verification**: PASSED (`activeCardIdentity: segment_000`)
* **Pre-Click Fingerprint & Cost Gate**: PASSED (20 credits <= 20 credit limit)
* **Generate Click Dispatched**: 1 (Committed credits: 20)
* **Current Attempt Status**: `SEGMENT_0_SUBMITTED` / `POST_CLICK_POLL_TIMEOUT` / `RECOVERY_PENDING`
* **Auto-Retries Executed**: 0 (Strictly fail-closed; Segment #1 was never dispatched; Segment #3 was never generated).

---

## 2. Production Hotfixes & Architecture Enhancements

### A. Strict Media Card Matching & Upload
* Removed generic `play_circle` / `play_arrow` fallback from `locateMediaCard`.
* Media cards must strictly match the active segment stem (`segment_000`, `segment_001`). Unrelated project media is ignored.
* Automatic canvas transition via `add_2 Tạo` / `arrow_forward Tạo` and canvas node focus.

### B. Pre-Click Fail-Closed Safety Gates
* **Exact Media Revalidation**: Verified immediately before clicking Generate.
* **Prepared Fingerprint Gate**: Canonical hash matching `sourceStem`, `promptHash`, model (`Omni Flash`), resolution (`720p`), duration (`10`), orientation (`PORTRAIT / 9:16`), output count (`1`).
* **Authoritative Live Cost Gate**: Tooltip and composer readback cross-checked against per-segment budget (<= 20 credits) and parent ledger budget (<= 40 credits).

### C. Zero-Paid Recovery Engine (`recover_existing_submission`)
* Connects to exact persisted Google Flow workspace.
* Performs 0 generate clicks and 0 paid preflights.
* Evaluates scoped node completion evidence over global text markers.
* Correlates completed video artifact, downloads without scratch dependencies, normalizes to exactly 300 frames, and checkpoints manifest.

### D. Resumption Mechanism (`resume_flow_generation`)
* Registered IPC command `resume_flow_generation` in `src-tauri/src/lib.rs` and `commands/mod.rs`.
* Integrated into `FlowJobProgress.tsx` with dedicated "Tiếp tục công việc (Resume)" button for `FAILED`, `BLOCKED`, and `GENERATION_AMBIGUOUS` states.
* Resumption verifies completed segments, checks `raw_children` cache, recovers Segment 0 if proven submitted, and proceeds with remaining unsubmitted segments without duplicate spend.

---

## 3. Automated Test Verification & Regression Suite

All non-paid unit and integration regression suites passed 100%:

| Test Suite | Tests Passed | Status |
| :--- | :---: | :--- |
| `tests_phase_flow_p4b` | 19 / 19 (4 ignored live) | **PASS** |
| Frontend Vitest (`src/**/*.test.ts`) | 61 / 61 (7 files) | **PASS** |
| TypeScript & Vite Build (`npm run build`) | All modules bundled | **PASS** |
| Sidecar Build (`npm run build` in `flow-playwright`) | `tsc` compilation clean | **PASS** |
| Rust Quality Gates (`cargo fmt`, `cargo check`) | Clean formatting & check | **PASS** |

---

## 4. Source Media Immutability

* **Asset Path**: `test-assets/p4b_source_15s.mp4`
* **Baseline SHA-256**: `03390797b5787a923bfd703c53cf0cec64680451ab8c36dc6fc43f9e9e04ddab`
* **Post-Run SHA-256**: `03390797b5787a923bfd703c53cf0cec64680451ab8c36dc6fc43f9e9e04ddab`
* **Status**: **VERIFIED IMMUTABLE** (Exact match).

---

## 5. Security & Safety Compliance

* **Zero-Fake Policy**: Real Google Flow interaction was performed for live tests; mock tests strictly isolated in unit tests; zero fake balances or fake outputs created.
* **Secrets Security**: No API tokens, passwords, cookies, or user profile credentials stored in source code, repository logs, or frontend bundles.
* **Observed Spend So Far**: 40 credits / 2 clicks across entire experimentation.
* **Maximum Combined Authorized Ceiling**: 60 credits / 3 clicks.
* **Auto-Retries**: 0.
