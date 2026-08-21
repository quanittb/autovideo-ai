# Phase 18 Engineering Report: Real Cloud Transformation Preview UI & Truthful Local Artifact Visualization

## 1. Executive Summary & Objectives
In Phase 18, we replaced the prototype mock transformation preview UX with a production-grade, authoritative frontend driven by:
- Real project source media probed via ffprobe facts (`SourceMediaFacts`).
- Authoritative backend preflight (`evaluate_cloud_submission_preflight` & `preflight_cloud_transformation`), ensuring 100% cost and routing parity between preflight and submission gates.
- Phase 15 persistent cloud lifecycle service, with canonical `internalJobId` state keying and monotonic revision merges (`mergeCloudJobSnapshot`).
- Exact-file runtime asset protocol authorization (`authorize_preview_asset` & `revoke_preview_asset`) using Tauri v2 `protocol-asset` and tight production CSP (`media-src 'self' asset: http://asset.localhost`).
- Format-aware, synchronized dual-player UI (`RealTransformPreview`), with transparent alpha checkerboard for WebM VP9 Alpha artifacts and truthful fallback openers (`open_cloud_artifact` & `open_cloud_artifact_folder`).

- **Starting Base HEAD**: `334862136631007e3ec4b54a4a66b20057305a92`
- **Zero-Live-Cost Policy**: Fully observed ($0.00 live cost, 0 real predictions, 0 real uploads)
- **Live Quality Verification**: `LIVE PROVIDER QUALITY VERIFIED: NO` (Provider quality evaluation rubric is deferred to Phase 20)
- **Interactive Runtime Verification**: `PREVIEW_RUNTIME_VERIFIED: NO` (Headless CI environment; manual verification steps documented in acceptance report)

---

## 2. Architecture & Design

### Unified Preflight & Submission Gate Pipeline
```
[User Selects Task / References / Budget]
                   │
                   ▼
  preflight_cloud_transformation(request, maxCost)
                   │
                   ▼
  evaluate_cloud_submission_preflight() (SHARED INTERNAL CORE)
  ├── Strict TaskClass parsing
  ├── Background Removal reference rejection (0 references required)
  ├── SourceMediaProbe (ffprobe duration, resolution, fps, audio facts)
  ├── GenerationRouter::route_with_facts (CostSaving policy)
  └── CostGuard budget limit check
                   │
                   ▼
    CloudSubmissionPreflight DTO
  { taskClass, sourceFacts, routingDecision, budgetLimit, budgetApproved, submittable, blockingCode }
                   │
       ┌───────────┴───────────┐
       ▼                       ▼
[submittable: true]     [submittable: false]
- Enabled Generate      - Disabled button
- Displays Cost         - Typed Error Banner
```

### Dynamic Asset Authorization & Scoping
```
[React Video Preview Mounts]
       │
       ▼
  authorize_preview_asset(projectId, assetKind, internalJobId?)
       │
       ▼
  Rust Security Enforcement:
  ├── Validate project_id & internal_job_id syntax
  ├── Canonicalize candidate file & canonicalize trusted root dir
  ├── Verify candidate starts_with trusted project directory
  ├── Verify job state == COMPLETED (for CloudArtifact)
  └── app.asset_protocol_scope().allow_file(canonical_file)
       │
       ▼
  AuthorizedAssetPreview DTO { localPath, container, videoCodec, alphaValidated, audioRequired, actualHasAudio }
       │
       ▼
  Frontend loads convertFileSrc(localPath) into <video>
       │
  [On Unmount / Job Switch]
       │
       ▼
  revoke_preview_asset() -> app.asset_protocol_scope().forbid_file(canonical_file)
```

---

## 3. Files Created & Modified

1. **`src-tauri/Cargo.toml`**:
   - Enabled `features = ["protocol-asset"]` for `tauri` dependency.
2. **`src-tauri/tauri.conf.json`**:
   - Configured `assetProtocol: { enable: true, scope: [] }`.
   - Configured strict production CSP (`default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' asset: http://asset.localhost data:; media-src 'self' asset: http://asset.localhost; style-src 'self' 'unsafe-inline';`).
   - Configured Vite HMR `devCsp`.
3. **`src-tauri/src/ai/cloud/job.rs`**:
   - Added `state_revision: u64` and `artifact_descriptor: Option<ArtifactDescriptor>` to `CloudJobEventPayload`.
   - Added DTOs: `CloudSubmissionPreflight`, `PreviewAssetKind`, `AuthorizedAssetPreview`.
4. **`src-tauri/src/ai/cloud/submission.rs`**:
   - Extracted shared authoritative core `evaluate_cloud_submission_preflight`.
   - Unified `validate_and_prepare_cloud_submission` to consume `evaluate_cloud_submission_preflight`.
5. **`src-tauri/src/commands/mod.rs` & `src-tauri/src/lib.rs`**:
   - Implemented and registered 7 IPC commands:
     - `preflight_cloud_transformation`
     - `start_cloud_transformation`
     - `list_cloud_jobs`
     - `authorize_preview_asset`
     - `revoke_preview_asset`
     - `open_cloud_artifact`
     - `open_cloud_artifact_folder`
6. **`src-tauri/src/ai/tests_phase18.rs`**:
   - Created 9 Rust unit and integration tests covering preflight facts, limits, references fail-closed, budget exceeded, event payloads, DTO serialization, project job listing, and preview gating.
7. **`src/types/contracts.ts` & `src/lib/ipc.ts`**:
   - Added TypeScript interfaces and `cloudApi` method wrappers.
8. **`src/stores/cloudJobHelpers.ts`**:
   - Implemented pure monotonic revision merger `mergeCloudJobSnapshot` and `isNewerRevision`.
9. **`src/stores/cloudJobStore.ts`**:
   - Created dedicated Zustand store with canonical `internalJobId` indexing, event subscription race protection, preflight integration, and exact-file asset authorization.
10. **`src/stores/__tests__/cloudJobStore.test.ts`**:
    - Created Vitest unit test suite verifying monotonic revisions, idempotency, canonical indexing, and zero-fabrication of `BLOCKED` jobs.
11. **`src/features/transform/RealTransformPreview.tsx`**:
    - Implemented synchronized dual video player with format badges (`MP4 • H.264`, `WebM • VP9 • Alpha`), alpha checkerboard, and fallback actions.
12. **`src/features/transform/TransformPanel.tsx`**:
    - Implemented transformation configuration with live authoritative preflight and route/cost display.
13. **`src/features/transform/StepTransform.tsx`**:
    - Connected preview player, transform controls, and lifecycle subscription.

---

## 4. Test Verification & Actual Results

### Quality Checks Executed:
1. **Rust Formatting**: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
   - Result: **Passed with code 0** (0 formatting diffs).
2. **Rust Compilation**: `cargo check --all-targets --manifest-path src-tauri/Cargo.toml`
   - Result: **Passed with 0 errors, 0 warnings**.
3. **Phase 18 Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase18 --test-threads=1`
   - Result: **9 passed; 0 failed; 0 ignored; finished in 0.03s**.
4. **Phase 17 Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase17 --test-threads=1`
   - Result: **56 passed; 0 failed; 0 ignored; finished in 1.64s**.
5. **Phase 16 Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase16 --test-threads=1`
   - Result: **39 passed; 0 failed; 0 ignored; finished in 4.13s**.
6. **Phase 15 Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- tests_phase15 --test-threads=1`
   - Result: **38 passed; 0 failed; 0 ignored; finished in 9.96s**.
7. **Phase 14 Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- test_phase14 --test-threads=1`
   - Result: **10 passed; 0 failed; 0 ignored; finished in 0.16s**.
8. **Cloud MVP Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- test_cloud --test-threads=1`
   - Result: **6 passed; 0 failed; 0 ignored; finished in 0.00s**.
9. **Full Rust Test Suite**: `cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1`
   - Result: **757 passed; 0 failed; 0 ignored; finished in 56.88s**.
10. **Frontend Vitest Unit Suite**: `npm.cmd test -- --run`
    - Result: **7 passed; 0 failed; finished in 0.24s**.
11. **Frontend Production Build**: `npm.cmd run build`
    - Result: **Built in 4.79s (0 errors)**.

---

## 5. Live Test Cost Incurred & Policy Compliance
- **Real Replicate Predictions**: 0
- **Real Replicate Uploads**: 0
- **Paid API Cost Incurred**: $0.00 USD
- **LIVE PROVIDER QUALITY VERIFIED**: NO (Visual quality rubric for output media is evaluated in Phase 20).
- **PREVIEW RUNTIME VERIFIED**: NO (Interactive desktop verification steps provided in acceptance documentation).

---

## 6. Next Steps
- Phase 18 is COMPLETE. DO NOT BEGIN PHASE 19.
