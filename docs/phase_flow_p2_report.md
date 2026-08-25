# AutoVideo AI — Phase FLOW-P2 Report
**Real Project Media Workflow, Derived Flow Asset Model & Zero-Paid Acceptance**

---

## 1. Executive Summary

Phase FLOW-P2 transitions Google Flow from an isolated generation runtime into a **complete, end-to-end Project Media Workflow** in AutoVideo AI.

Key milestones achieved:
1. **Source Media ID Resolution Bug Eliminated**: Frontend and backend strictly operate on canonical `mediaId` keys rather than raw paths or frontend temporary asset IDs.
2. **Project Schema Version 2**: Introduced `DerivedMediaProvenance` and `DerivedMediaAsset` models to store Flow-generated artifacts with rich generation lineage (parent job ID, source media ID, prompt hash, transformation intent, identity mode, timestamp) inside `project.json` without mutating or overwriting original source media.
3. **Active Working Media Tracking**: Added `active_media_id` in `ProjectEditorState` with backward-compatible fallback to `source_media.media_id`.
4. **Canonical Project Media Resolver**: Implemented `resolve_project_media_by_id(projectId, mediaId)` strictly enforcing path confinement within `projects/<projectId>/media/` and blocking path traversal attempts.
5. **Idempotent Output Ingestion**: `use_flow_output_in_project` verifies artifacts, copies them to `projects/<projectId>/media/derived/flow_<jobId>_<assetId>.mp4`, probes video facts via ffprobe/MediaService, appends to `derived_media_assets`, activates the new media, and returns `UseFlowOutputResult`. Repeated calls return existing derived media assets idempotently.
6. **Chained Flow Editing**: Allows chaining multiple Flow operations (e.g. Generation 1 on original video $\to$ Use in Project $\to$ Generation 2 on Generation 1 output) with full provenance.
7. **Flow Manifest Schema Version 3**: Persists `transformation_intent`, `identity_mode`, and `target_face` fields. Added `PromptSource::SystemDefault` for empty prompts on `FACE_REPLACE` + `GENERATED` transformations.
8. **Removed Fake Fixtures in Production**: `activeProject` defaults to `null` on fresh startup, and creating a new project starts with clean state (no injected fake scenes like "Woodland Overview").
9. **Zero-Paid Invariant Maintained**: 100% verified using local tests, unit suites, and mock bridges with \$0.00 spent.

---

## 2. Architecture & Domain Model

### 2.1 Derived Media & Project Schema V2

```
Project Directory Layout:
projects/
  └── <projectId>/
        ├── project.json              (Schema version 2)
        ├── media/
        │     ├── <original_file>.mp4 (Original SourceMedia)
        │     └── derived/
        │           ├── flow_<job1>_<assetId>.mp4 (DerivedMediaAsset 1)
        │           └── flow_<job2>_<assetId>.mp4 (DerivedMediaAsset 2)
        └── flow-jobs/
              └── flow_<jobId>/
                    ├── manifest.json (Schema version 3)
                    ├── output.mp4
                    └── ...
```

### 2.2 Domain Entities

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedMediaProvenance {
    pub provider: String,                  // "FLOW"
    pub provider_job_id: String,           // Parent flow job ID
    pub source_media_id: String,           // Input media ID used for this run
    pub transformation_intent: TransformationIntent,
    pub identity_mode: IdentityMode,
    pub prompt_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedMediaAsset {
    pub media: SourceMedia,
    pub provenance: DerivedMediaProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UseFlowOutputResult {
    pub derived_asset: DerivedMediaAsset,
    pub project: Project,
}
```

### 2.3 Prompt Resolution Hierarchy for Flow

| Transformation Intent | Identity Mode | Prompt Provided? | Outcome / Prompt Source |
| :--- | :--- | :--- | :--- |
| `FACE_REPLACE` | `GENERATED` | Empty / Whitespace | **Accepted** $\to$ `SYSTEM_DEFAULT` deterministic preservation prompt |
| `FACE_REPLACE` | `GENERATED` | Non-empty | **Accepted** $\to$ `USER` or `GEMINI_OPTIMIZED` |
| `FACE_REPLACE` | `REFERENCE` | Any | **Rejected** $\to$ `FLOW_REFERENCE_IDENTITY_NOT_SUPPORTED` |
| `STYLE_EDIT` / Other | Any | Empty | **Rejected** $\to$ `REQUEST_INVALID` (prompt required) |
| `STYLE_EDIT` / Other | Any | Non-empty | **Accepted** $\to$ `USER` or `GEMINI_OPTIMIZED` |

---

## 3. Files Changed

### Backend (Rust)
- `src-tauri/src/projects/mod.rs`: Incremented `CURRENT_SCHEMA_VERSION = 2`, added `DerivedMediaProvenance`, `DerivedMediaAsset`, `UseFlowOutputResult`, `active_media_id`.
- `src-tauri/src/ai/flow/prompt_optimizer.rs`: Added `PromptSource::SystemDefault`.
- `src-tauri/src/ai/flow/manifest.rs`: Incremented `CURRENT_FLOW_MANIFEST_SCHEMA_VERSION = 3`, added transformation and budget snapshot fields.
- `src-tauri/src/ai/flow/orchestrator.rs`: Integrated prompt defaults, capability verification, and manifest v3 creation.
- `src-tauri/src/commands/mod.rs`: Added `resolve_project_media_by_id`, `authorize_project_media_preview`, updated `start_flow_generation` and idempotent `use_flow_output_in_project`.
- `src-tauri/src/lib.rs`: Registered `authorize_project_media_preview` in Tauri invoke handler.
- `src-tauri/src/media/mod.rs`: Initialized `active_media_id: None` in project editor state tests.
- `src-tauri/src/ai/mod.rs`: Registered `tests_phase_flow_p2`.
- `src-tauri/src/ai/tests_phase20a/manifest_tests.rs`: Updated manifest instantiation fixtures with new schema v3 fields.
- `src-tauri/src/ai/tests_phase20a/security_mock_tests.rs`: Updated crash test fixture with new schema v3 fields.
- `src-tauri/src/ai/tests_phase_flow_p2.rs`: New unit and integration test suite (5 tests).

### Frontend (TypeScript / React)
- `src/types/contracts.ts`: Added `DerivedMediaProvenance`, `DerivedMediaAsset`, `UseFlowOutputResult`, `TransformationIntent`, `IdentityMode`, `TargetFaceSelection`, and `activeMediaId`.
- `src/lib/ipc.ts`: Added `SYSTEM_DEFAULT` prompt source, `useInProject`, `authorizeProjectMediaPreview`, and updated `useOutputInProject`.
- `src/stores/projectStore.ts`: Set default `activeProject: null`, removed hardcoded default scene injections.
- `src/stores/flowJobStore.ts`: Added `jobs: FlowJobSnapshot[]`, `loadFlowJobs(projectId)`, and updated `useOutputInProject`.
- `src/features/flow/FlowGenPanel.tsx`: Added Working Media dropdown selector (Original vs Derived), empty prompt handling for `FACE_REPLACE`, and attention state polling lifecycle.
- `src/features/flow/FlowJobProgress.tsx`: Updated "Use in Project" handler to update `projectStore` active project state.
- `src/stores/__tests__/projectStore.test.ts`: Added test suite for clean project initialization and Schema V2 loading.
- `src/stores/__tests__/flowJobStore.test.ts`: Updated mock signatures for `startGeneration` and `listFlowJobs`.
- `src/features/flow/__tests__/flowPromptUx.test.ts`: Updated mock signatures for `startGeneration`.

---

## 4. Verification Results

### 4.1 Automated Test Execution

| Test Suite | Tests Run | Passed | Failed | Execution Time |
| :--- | :--- | :--- | :--- | :--- |
| `ai::tests_phase_flow_p2` | 5 | 5 | 0 | 0.12s |
| `ai::tests_phase20c` | 13 | 13 | 0 | 0.00s |
| `ai::tests_phase20b` | 27 | 27 | 0 | 123.11s |
| `ai::tests_phase20a` | 78 | 78 | 0 | 58.51s |
| Frontend Vitest (`npm test`) | 59 | 59 | 0 | 0.53s |
| **Total Tests** | **182** | **182** | **0** | — |

### 4.2 Quality Checks

- `cargo check`: **PASS** (0 errors)
- `cargo fmt --check`: **PASS** (0 diffs)
- `npm run build` (`tsc && vite build`): **PASS** (0 errors, 1865 modules transformed)

---

## 5. Cost & Provider Audit

- **Live Flow Video Generations**: `0`
- **Live Pruna API Calls**: `0`
- **Live BRIA API Calls**: `0`
- **Live Gemini Calls**: `0`
- **Total Incurred Cost**: `\$0.00`
- **Zero-Paid Invariant**: **STRICTLY PRESERVED**

---

## 6. Conclusion & Next Steps

Phase FLOW-P2 is complete with full real-project media integration, derived asset provenance tracking, schema versioning, chained Flow editing, and rigorous test coverage.
