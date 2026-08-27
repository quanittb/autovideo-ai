# Phase FLOW-P4-B / FLOW-P4-B.1 Report: Failed Run Forensics & Clean-Rerun Readiness

## Executive Summary

Phase FLOW-P4-B executed the first live paid run on Google Flow using profile `profile_2`.
During this execution, a loose media card fallback matched an existing 5-second video in the Flow project instead of uploading the intended 10-second `segment_000.mp4`.
1 paid click was dispatched and 20 credits were consumed.
In accordance with our zero-fake policy and behavioral guidelines, this run is strictly recorded as **FAILED_PRECONDITION / WRONG_SOURCE_MEDIA_SELECTED** and is NOT accepted.

All root causes have been fixed, hardened, and verified with 9 comprehensive automated tests.
No scratch scripts are referenced anywhere in production.
The codebase is frozen and verified ready for a clean rerun upon explicit operator authorization.

---

## 1. Attempt #1 — Forensic Analysis & Accounting

* **Run Identifier**: `P4B_RUN_1`
* **Status**: `FAILED_PRECONDITION / WRONG_SOURCE_MEDIA_SELECTED`
* **P4B Accepted**: `NO`
* **Paid Generate Clicks Dispatched**: `1`
* **Authoritative Credits Spent**: `20`
* **Initial Account Balance**: `1050`
* **Current Account Balance**: `1030`
* **Produced Output Status**: Excluded from pipeline; not a valid child segment; no `DerivedMediaAsset` created.

### Root Cause Breakdown
1. **Loose Media Card Fallback (`locateMediaCard`)**:
   `locateMediaCard` included a fallback rule matching any card containing `play_circle`. Because the test project already contained an existing video card, the fallback matched this card instead of uploading `segment_000.mp4`.
2. **Download URL Prerequisite Bug**:
   The Rust orchestrator called `poll_res.download_url.ok_or_else(...)`. Google Flow's edit page triggers a browser download event on button click rather than exposing a direct `href` download URL, causing the background worker to exit with an error.
3. **Polling Order Shadowing**:
   The sidecar checked for download buttons before checking for `Generating` / `Queued` markers. Since the edit toolbar persistently displays a download button for the input media, this shadowed the active generation state.

---

## 2. Production Hotfixes Implemented & Frozen

### A. Sidecar (`src-tauri/sidecars/flow-playwright/src/flow_adapter.ts`)
* **Strict Media Card Matching**: Removed the generic `play_circle` / `play_arrow` fallback. Cards must strictly match `baseStem` (`segment_000`, `segment_001`). If not found, upload is mandatory.
* **Pre-Click Revalidation**:
  * Added `sourceIdentity` matching check immediately before clicking Generate. Mismatch triggers `FLOW_ACTIVE_MEDIA_MISMATCH` (fail-closed, 0 clicks).
  * Added duration cross-check immediately before clicking Generate. Observable duration deviation $> 2.0$s triggers `FLOW_ACTIVE_MEDIA_DURATION_MISMATCH` (fail-closed, 0 clicks).
* **Generation Polling Sequence**: Reordered state checks so `Generating` / `Queued` indicators (`div:has-text("Đang tạo")`, `div:has-text("Generating...")`, `#progress-indicator`) are evaluated BEFORE download controls.
* **Download Event Handling**: `downloadArtifact` supports both direct URL download and button click + Playwright `download` event capture. Ambiguous or missing download controls fail with `FLOW_GENERATED_OUTPUT_NOT_UNIQUELY_IDENTIFIED`.

### B. Rust Backend (`src-tauri/src/ai/flow/`)
* **Orchestrator (`orchestrator.rs`)**:
  * Relaxed `download_url` requirement: calls `session_ref.download(poll_res.download_url.as_deref(), &raw_child).await?`.
  * Added pre-click `source_identity` verification against `expected_stem`.
  * Added worker task error propagation: uncaught errors in `tokio::spawn` worker immediately update manifest to `FlowJobState::Failed` with sanitized error message.
  * Hardened ledger accounting: committed credits and dispatched clicks are recorded immediately upon `ProvenSubmitted` so that downstream failures cannot erase credit expenditure.
* **Manifest (`manifest.rs`)**:
  * Added `FlowUploadedSourceEvidence` struct and `uploaded_source_evidence` field to `FlowPlannedSegment`.
  * Added `click_dispatched`, `preclick_cost` to `FlowPlannedSegment`.
  * Added `dispatched_paid_clicks` to `FlowParentLedger`.

---

## 3. Automated Test Verification

| Test Name | Scope | Result |
| :--- | :--- | :--- |
| `test_flow_p4b1_00_no_scratch_script_dependency` | Zero production dependencies on scratch files | **PASS** |
| `test_flow_p4b1_01_exact_matching_ignores_wrong_existing_card` | Media card matching ignores existing unrelated cards | **PASS** |
| `test_flow_p4b1_02_play_circle_fallback_removed` | No generic play_circle fallback when card missing | **PASS** |
| `test_flow_p4b1_03_download_button_while_generating_returns_generating` | Generating check precedes download button check | **PASS** |
| `test_flow_p4b1_04_button_based_download_without_href` | Download without href uses browser event path | **PASS** |
| `test_flow_p4b1_05_direct_url_download_supported` | Direct URL download supported when href present | **PASS** |
| `test_flow_p4b1_06_output_ambiguity_fails_closed` | Ambiguous output throws `FLOW_GENERATED_OUTPUT_NOT_UNIQUELY_IDENTIFIED` | **PASS** |
| `test_flow_p4b1_07_worker_terminal_failure_persists_error` | Worker error sets manifest state = Failed | **PASS** |
| `test_flow_p4b1_08_clean_rerun_dry_run` | Full 15s mock long video pipeline with unrelated project media | **PASS** |
| `test_flow_p4b_live_acceptance` | Live paid acceptance test (guarded, skipped in default suite) | **IGNORED** |

### Regression Suite Results
* `cargo fmt --check`: **PASS**
* `cargo check`: **PASS**
* `cargo test ... tests_phase_flow_p4a1`: **8/8 PASS**
* `cargo test ... tests_phase_flow_p4a`: **17/17 PASS**
* `cargo test ... tests_phase_flow_p4b`: **9/9 PASS** (1 ignored)
* `npm test`: **61/61 PASS** (7 test files)
* `npm run build`: **PASS** (Frontend and Sidecar)

---

## 4. Source Immutability Baseline

* **File**: `test-assets/p4b_source_15s.mp4`
* **Baseline SHA-256**: `03390797b5787a923bfd703c53cf0cec64680451ab8c36dc6fc43f9e9e04ddab`
* **Current SHA-256**: `03390797b5787a923bfd703c53cf0cec64680451ab8c36dc6fc43f9e9e04ddab`
* **Status**: **VERIFIED IMMUTABLE** (Exact match).

---

## 5. Budget & Authorization Status

* **Initial Authorization**: 40 credits, 2 clicks, 0 auto-retries.
* **Consumed in Attempt #1**: 20 credits, 1 click.
* **Remaining in Initial Authorization**: 20 credits, 1 click.
* **Original Authorization Insufficient for Clean Rerun**: **YES** (a clean rerun requires 2 segments = 40 credits, 2 clicks).
* **New Paid Authorization Included**: **NO** (Zero new paid clicks or generations performed in P4-B.1).

---

## 6. Clean Rerun Readiness Decision

* `CLEAN_P4B_RERUN_READY`: **YES**
* All hotfixes are implemented, verified by unit/integration regression suites, and frozen.
* The system is halted and awaiting operator authorization.
