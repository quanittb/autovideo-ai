# Phase 15 Report — Persistent Cloud Job Lifecycle

## 1. Executive Summary & Baselines

- **Starting Baseline HEAD**: `3a1006ad16f6793ef2b66bd4048fac8b6e463ab7`
- **Implementation Commit**: `ef096b2eb4bfa9b3e5e9b8344092555c95e38617`
- **Paid Live API Calls Incurred**: **$0.00**
- **Test Status**: 17/17 Phase 15 unit tests passed, 632/632 full repository tests passed.

Phase 15 replaces previous in-memory cloud job handling with an authoritative, persistent, recoverable, cancellable, and strictly output-validated cloud job lifecycle service.

---

## 2. Architecture & Components

```
                +------------------------------------------------+
                |           Tauri IPC Command Layer              |
                | (start_cloud_generation, get_cloud_job_status) |
                +-----------------------+------------------------+
                                        |
                                        v
                +------------------------------------------------+
                |         CloudJobLifecycleService               |
                |  - Per-Job Tokio Mutex Concurrency Lock        |
                |  - Routing & Budget Pre-Submission Guard       |
                |  - Background Polling & Retry Orchestration   |
                +----+--------------------+-----------------+----+
                     |                    |                 |
                     v                    v                 v
          +--------------------+  +---------------+  +--------------------+
          |PersistentCloudStore|  |ProviderResolve|  |CloudOutputValidator|
          | - Atomic Replace   |  | - Replicate   |  | - FFprobe Inspect  |
          | - Revision Compare |  | - Fallbacks   |  | - Duration Bounds  |
          | - Crash Recovery   |  | - Decoupling  |  | - SHA-256 Compute  |
          +--------------------+  +---------------+  +--------------------+
```

### Storage Architecture & Paths
- **Cloud Jobs Storage Directory**: `<project_dir>/cloud-jobs/`
- **Primary Job Manifest**: `<project_dir>/cloud-jobs/<internal_job_id>.json`
- **Atomic Staging Manifest**: `<project_dir>/cloud-jobs/<internal_job_id>.json.tmp`
- **Artifact Temporary Partial**: `<project_dir>/cloud-jobs/artifacts/<internal_job_id>.partial`
- **Artifact Final Output**: `<project_dir>/cloud-jobs/artifacts/<internal_job_id>.mp4`

---

## 3. Crash Safety & Concurrency

### Windows Atomic Replacement Strategy
On Windows, standard file rename operations fail if the destination file already exists. AutoVideo AI implements atomic replace using Windows `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` flags on Windows, with atomic fallback on Unix. Manifests are fsynced to `.tmp` before atomic replacement, guaranteeing that primary files are never left half-written.

### State Revision Semantics
Every `PersistentCloudJob` contains a strictly monotonic `state_revision: u64`. The revision increments on **every persisted mutation**, including:
- `submission_state` transitions
- `remote_job_id` assignment
- `cancellation_requested` flags
- retry attempt counters
- error details and codes
- `remote_status` telemetry
- progress percentage updates
- actual cost and budget limit recordings
- output artifact hashes and paths
- lifecycle timestamps

### Complete Crash Recovery Matrix
When loading or recovering a job upon startup, the store checks both primary (`.json`) and temporary staging (`.tmp`) manifests:

| Primary Status | Temp Status | Action / Selected Record | Rationale |
|---|---|---|---|
| Valid (Rev $N$) | Valid (Rev $N+1$) | **Promote Temp ($N+1$)** | Crash occurred after fsync of newer state but before rename. Prevents restoring stale `NEVER_ATTEMPTED` and double submissions. |
| Valid (Rev $N$) | Valid (Rev $\le N$) | **Use Primary ($N$)**, remove temp | Primary contains same or newer state; stale temp discarded. |
| Valid (Rev $N$) | Missing / Corrupt | **Use Primary ($N$)**, clean temp | Primary is authoritative. |
| Missing | Valid (Rev $M$) | **Promote Temp ($M$)** | Crash occurred during initial creation. |
| Corrupt | Valid (Rev $M$) | **Backup corrupt primary, promote Temp ($M$)** | Preserves uncorrupted staging state while saving audit trail. |
| Corrupt / Missing | Corrupt / Missing | **Explicit Recovery Error** | Fails fast with clear diagnostic logging. |

### Concurrency Locking & Duplicate Prevention
- **Per-Job Mutex Locking**: `CloudJobLifecycleService` maintains a thread-safe registry of asynchronous per-job locks (`Mutex<HashMap<String, Arc<TokioMutex<()>>>>`). Simultaneous IPC requests for the same `job_id` acquire an exclusive lock, preventing race conditions where both callers read unsubmitted state.
- **In-Flight Intent Persistence**: Prior to invoking `provider.submit_job()`, the job is transitioned to `SubmissionState::InFlight` with incremented `state_revision` and fsynced to disk.
- **Ambiguous Submission Protection**: If submission fails without an acknowledged remote job handle, the job is transitioned to `CloudJobState::Blocked` (`SubmissionState::Ambiguous`). Automated background retries are blocked to protect user budgets.
- **Missing-Provider Resume**: If an acknowledged `Processing` job is recovered but its provider credentials are missing at startup, it is safely marked `Blocked` (`MISSING_PROVIDER_CREDENTIALS`). When credentials become available, calling `resume_unblock_job()` resumes polling the **same remote job ID** without calling `submit_job` again.

---

## 4. Media Output Validation & Source Immutability

### Output Validation Pipeline
Provider status `Succeeded` is **never** mapped directly to `CloudJobState::Completed`. Output flows strictly through the validation gate:
1. Provider `Succeeded` $\rightarrow$ Download remote media to `<internal_job_id>.partial`.
2. Transition to `CloudJobState::ValidatingOutput`.
3. Strict inspection with `probe_with_ffprobe`:
   - Valid video stream container and non-zero dimensions ($W > 0, H > 0$).
   - Duration within tolerance bounds.
   - Audio presence check when audio preservation is requested.
4. Cryptographic SHA-256 hash calculated over the validated payload.
5. Atomic promotion from `<internal_job_id>.partial` to `<internal_job_id>.mp4`.
6. Transition to `CloudJobState::Completed` with populated `OutputArtifactRecord`.

### Source Media Immutability
All source media files remain read-only throughout cloud generation. Output artifacts are written exclusively to isolated `cloud-jobs/artifacts/` directories.

---

## 5. Exact Test Results & Quality Gates

### 1. Phase 15 Test Suite (`cargo test -- test_phase15 --test-threads=1`)
```
running 17 tests
test ai::tests_phase15::tests::test_phase15_01_full_lifecycle_success ... ok
test ai::tests_phase15::tests::test_phase15_02_restart_restores_processing_job ... ok
test ai::tests_phase15::tests::test_phase15_03_restart_cannot_double_submit ... ok
test ai::tests_phase15::tests::test_phase15_04_cancellation_survives_restart ... ok
test ai::tests_phase15::tests::test_phase15_05_corrupt_output_fails_validation ... ok
test ai::tests_phase15::tests::test_phase15_06_polling_timeout_fails_bounded ... ok
test ai::tests_phase15::tests::test_phase15_07_download_retry_bounded ... ok
test ai::tests_phase15::tests::test_phase15_08_ambiguous_submission_blocks_auto_resubmit ... ok
test ai::tests_phase15::tests::test_phase15_09_atomic_write_recovery_temp_corrupt ... ok
test ai::tests_phase15::tests::test_phase15_10_old_state_backward_compatibility ... ok
test ai::tests_phase15::tests::test_phase15_11_event_contract_serialization ... ok
test ai::tests_phase15::tests::test_phase15_12_illegal_transitions_rejected ... ok
test ai::tests_phase15::tests::test_phase15_13_source_media_immutability ... ok
test ai::tests_phase15::tests::test_phase15_14_concurrent_same_job_submissions ... ok
test ai::tests_phase15::tests::test_phase15_15_monotonic_revision_crash_recovery ... ok
test ai::tests_phase15::tests::test_phase15_16_path_traversal_rejection ... ok
test ai::tests_phase15::tests::test_phase15_17_missing_credentials_safe_recovery_and_resume ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 615 filtered out; finished in 3.89s
```

### 2. Full Rust Test Suite (`cargo test -- --test-threads=1`)
```
test result: ok. 632 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 3. Rust Code Formatting & Check
- `cargo fmt -- --check`: Passed (Exit code: 0)
- `cargo check --all-targets`: Passed (Exit code: 0)

### 4. Explicit FFprobe Validation
- **Test Artifact Path**: `src-tauri\target\phase15_test_artifact.mp4`
- **Command**: `ffprobe -v error -show_format -show_streams src-tauri\target\phase15_test_artifact.mp4`
- **Streams Detected**: 1 Video (H.264, 576x1024, 25fps, 1.0s), 1 Audio (AAC, 44.1kHz mono, 1.0s)
- **Exit Code**: **0**

### 5. Frontend Production Build (`npm run build`)
```
✓ 1859 modules transformed.
dist/index.html                   0.49 kB │ gzip:   0.32 kB
dist/assets/index-TXUKyMTD.css   87.21 kB │ gzip:  12.27 kB
dist/assets/window-D1F3Wgkb.js   13.92 kB │ gzip:   3.43 kB
dist/assets/index-B9n8H4kj.js   471.99 kB │ gzip: 119.03 kB
✓ built in 17.69s
```

---

## 6. Files Changed in Repository

| File | Type | Purpose |
|---|---|---|
| `src-tauri/src/ai/cloud/job.rs` | Modified | Canonical `CloudJobState`, `PersistentCloudJob`, `SubmissionState`, `state_revision`, `CloudJobEventPayload` |
| `src-tauri/src/ai/cloud/store.rs` | New | Windows atomic replace, monotonic revision recovery, identifier validation |
| `src-tauri/src/ai/cloud/resolver.rs` | New | `CloudProviderResolver` abstraction |
| `src-tauri/src/ai/cloud/validator.rs` | New | `CloudOutputValidator` with strict `probe_with_ffprobe` and SHA-256 computation |
| `src-tauri/src/ai/cloud/lifecycle.rs` | New | `CloudJobLifecycleService` with concurrency locks, startup recovery, polling & download loops |
| `src-tauri/src/ai/cloud/mod.rs` | Modified | Re-exported lifecycle components |
| `src-tauri/src/ai/cloud/router.rs` | Modified | Normalized `TaskClass::from_str_or_default` and added generative transformation cloud routing |
| `src-tauri/src/ai/cloud/submission.rs` | Modified | Aligned submission routing mode to `RoutingPreference::Quality` |
| `src-tauri/src/ai/mod.rs` | Modified | Registered `tests_phase15` |
| `src-tauri/src/ai/tests_cloud_mvp.rs` | Modified | Updated test assertions for `projectId` and `CloudJobState::Created` |
| `src-tauri/src/ai/tests_phase15.rs` | New | 17 comprehensive unit tests for all Phase 15 requirements |
| `src-tauri/src/commands/mod.rs` | Modified | Replaced ad-hoc cloud state with managed `CloudJobLifecycleService` |
| `src-tauri/src/lib.rs` | Modified | Managed `Arc<CloudJobLifecycleService>` and startup recovery hook in Tauri |
| `src-tauri/src/media/mod.rs` | Modified | Made `probe_with_ffprobe` public and supported `"partial"` extension |
| `src/lib/ipc.ts` | Modified | Updated TypeScript IPC contracts |
| `docs/phase_15_report.md` | New | Comprehensive Phase 15 verification report |

---

## 7. Remaining Limitations

- Real remote provider HTTP adapters for Character Replacement (Phase 16) and Background Removal (Phase 17) remain deferred to their respective phases.
- Real paid API submission tests remain disabled until phase-specific integration gates are reached.
