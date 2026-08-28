# Phase FLOW-P4-B / FLOW-P4-B.1 Report: Google Flow Long-Video Production Acceptance & Accounting

## Executive Summary

Phase FLOW-P4-B evaluated the Google Flow two-segment long-video production pipeline using `profile_2` and `test-assets/p4b_source_15s.mp4`.
All executions strictly adhered to the Zero-Fake Policy and explicit human budget authorization guards:
- **Maximum Approved Total Spend**: 40 credits
- **Maximum Approved Paid Clicks**: 2
- **Auto-Retries**: 0
- **Total Paid Clicks Dispatched**: 2 (1 in Attempt #1, 1 in Clean Rerun)
- **Total Authoritative Credits Spent**: 40 (20 in Attempt #1, 20 in Clean Rerun)

---

## 1. Dual-Ledger Total Accounting Model

| Ledger Component | Dispatched Paid Clicks | Authoritative Credits Spent | Outcome / Status |
| :--- | :---: | :---: | :--- |
| **Historical Failed Attempt #1** | 1 | 20 | `FAILED_PRECONDITION / WRONG_SOURCE_MEDIA_SELECTED` (Quarantined) |
| **Clean Rerun Attempt** | 1 | 20 | `PRE_CLICK_VERIFIED / SEGMENT_0_DISPATCHED / GENERATION_TIMEOUT` |
| **Total Cumulative Experimentation** | **2** | **40** | **BUDGET LIMIT REACHED (40/40 Credits, 2/2 Clicks)** |

### Historical Attempt #1 (Quarantined)
* **Run ID**: `P4B_RUN_1`
* **Status**: Excluded from pipeline; zero reuse as Segment 0, Segment 1, stitch input, or `DerivedMediaAsset`.
* **Dispatched Paid Clicks**: 1
* **Credits Spent**: 20

### Clean Rerun Attempt
* **Run ID**: `flow_792c6813-4c0d-485b-8e13-81a942a2e169` / `flow_6e304484-bcb9-458f-9790-2d07f18fc621`
* **Source Media**: `test-assets/p4b_source_15s.mp4` (SHA-256: `03390797b5787a923bfd703c53cf0cec64680451ab8c36dc6fc43f9e9e04ddab`)
* **Segment 0 Input**: `segment_000.mp4` (10.0s, 300 frames, 720p, Portrait 9:16)
* **Pre-Click Exact Media Verification**: PASSED (`activeCardIdentity: segment_000`)
* **Pre-Click Fingerprint & Cost Gate**: PASSED (20 credits <= 20 credit limit)
* **Generate Click Dispatched**: 1
* **Credits Spent**: 20
* **Polling Outcome**: Dispatched Segment 0 generation polled for 10 minutes in `Generating` state before reaching safety polling timeout.
* **Auto-Retries Executed**: 0 (Strictly fail-closed; Segment #1 was never dispatched; Segment #3 was never generated).

---

## 2. Production Hotfixes Implemented & Verified

### A. Strict Media Card Matching & Upload
* Removed generic `play_circle` / `play_arrow` fallback from `locateMediaCard`.
* Media cards must strictly match the active segment stem (`segment_000`, `segment_001`). Unrelated project media is ignored.
* Automatic canvas transition via `add_2 Tạo` / `arrow_forward Tạo` and canvas node focus.

### B. Pre-Click Fail-Closed Safety Gates
* **Exact Media Revalidation**: Verified immediately before clicking Generate.
* **Prepared Fingerprint Gate**: Canonical hash matching `sourceStem`, `promptHash`, model (`Omni Flash`), resolution (`720p`), duration (`10`), orientation (`PORTRAIT / 9:16`), output count (`1`).
* **Authoritative Live Cost Gate**: Tooltip and composer readback cross-checked against per-segment budget (<= 20 credits) and parent ledger budget (<= 40 credits).

### C. Download & Polling Subsystem
* `detectGenerationState` checks terminal error, eligibility, and generating/queued markers without fabricating progress.
* `downloadArtifact` supports direct HTTP fetch, browser download event interception, and in-page `blob:` URL evaluation and binary streaming.

---

## 3. Automated Test Verification & Regression Suite

All non-paid unit and integration regression suites passed 100%:

| Test Suite | Tests Passed | Status |
| :--- | :---: | :--- |
| `tests_phase_flow_p4a1` | 8 / 8 (1 ignored live) | **PASS** |
| `tests_phase_flow_p4a` | 25 / 25 (1 ignored live) | **PASS** |
| `tests_phase_flow_p4b` | 9 / 9 (1 ignored live) | **PASS** |
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
* **Budget Limits**: Never exceeded 40 credits; exactly 2 paid clicks dispatched across entire Phase 4B experimentation; zero auto-retries.
