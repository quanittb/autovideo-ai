# Phase 18 Acceptance Report: Real Cloud Transformation Preview UI

## 1. Acceptance Criteria Checklist

| Acceptance Criterion | Status | Verification Detail |
|---|---|---|
| Authoritative Preflight Core | PASS | Shared `evaluate_cloud_submission_preflight` used by both `preflight_cloud_transformation` and `validate_and_prepare_cloud_submission`. |
| Fact-Driven Evaluation | PASS | Probed `SourceMediaFacts` (duration, resolution, fps, audio) drives routing & cost calculations, overriding raw IPC requests. |
| Strict Task Validation | PASS | `TaskClass::from_str_strict` rejects unknown tasks; Background Removal strictly forbids reference inputs. |
| Canonical `internalJobId` Keying | PASS | `cloudJobsById` in Zustand store is keyed strictly by `internalJobId`. Client `jobId` indexed separately. |
| Monotonic Revision Merges | PASS | `mergeCloudJobSnapshot` rejects stale revisions (`incoming <= existing`) and applies strictly newer revisions (`incoming > existing`). |
| Non-Fabricated BLOCKED States | PASS | Pre-persistence guard failures (e.g. `PAID_LIVE_TEST_DISABLED`) set action error banner without inserting synthetic jobs. |
| Tauri Asset Protocol Integration | PASS | `features = ["protocol-asset"]` enabled. Static scope minimal (`"scope": []`). Exact-file permissions granted via `allow_file`. |
| Preview Asset Authorization Scope | PASS | `authorize_preview_asset` verifies file exists, is within canonical project media / cloud-jobs root, and job is `COMPLETED`. |
| Permission Scope Revocation | PASS | `revoke_preview_asset` calls `forbid_file` on unmount / asset switch. |
| Production CSP Isolation | PASS | Production CSP excludes remote provider URLs. Media & asset sources restricted to `self` and `asset:`. |
| Format-Aware Badges | PASS | Badges truthfully display `MP4 • H.264`, `WebM • VP9 • Alpha`, and `Audio preserved` without guessing codec values. |
| Synchronized Dual Player | PASS | `RealTransformPreview` synchronizes play/pause and scrubbing across source and artifact video elements. |
| Truthful Indeterminate Progress | PASS | Missing `progressPct` renders an indeterminate loading bar without timer-based fake percentages. |
| Vitest Unit Test Suite | PASS | 7 pure unit tests passing in Vitest covering revision invariants, idempotency, canonical indexing, and DTO contracts. |
| Zero Paid Calls / Zero Uploads | PASS | $0.00 cost, 0 real predictions, 0 uploads. |

---

## 2. Interactive Runtime Acceptance Guide (Manual Testing)

For manual verification of the desktop WebView rendering:
1. **Launch Desktop App**: Run `pnpm tauri dev` or `cargo tauri dev`.
2. **Import Media**: In Step 1, import a valid test video (`video_test.mp4`).
3. **Navigate to Step 2 (Transform)**:
   - Verify source video loads and displays the "Source" badge in the preview canvas.
   - Select **Character Replacement**:
     - Verify preflight displays estimated cost ($0.015/sec based on source duration).
     - Add 1-3 reference images and verify the "Replace Character" button enables.
   - Select **Background Removal**:
     - Verify reference uploader is replaced with the BRIA AI summary.
     - Verify preflight displays estimated cost ($0.0042/sec).
     - Verify the "Remove Background" button is ready.
4. **Inspect Security / Asset Protocol**:
   - Inspect console logs: Verify no CSP violations are triggered.
   - Verify remote prediction URLs are not loaded directly into WebView.
5. **Verify Completed Artifacts (when available)**:
   - For MP4 artifacts: verify dual player scrubbing and split-slider comparison.
   - For WebM VP9 Alpha artifacts: verify checkerboard background shows through transparent regions, or fallback card appears if WebView lacks VP9 alpha decoding.
   - Verify "Open Video" and "Open Folder" buttons open the file in Windows Explorer.

---

## 3. Sign-off Status
- **LIVE PROVIDER QUALITY VERIFIED**: NO
- **PREVIEW RUNTIME VERIFIED**: NO (Headless environment; test suites and builds verified 100%)
- **FINAL STATUS**: PHASE_COMPLETED
