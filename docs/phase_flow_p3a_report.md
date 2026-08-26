# Phase FLOW-P3-A Report: Real Google Flow Production Preflight & Live Credit Readback

## 1. Executive Summary & Zero-Spend Confirmation

Phase **FLOW-P3-A** establishes and validates the **dedicated production preflight inspection** for Google Flow generative video editing.

- **Total Accounting for Phase FLOW-P3-A**:
  - `FLOW_REAL_PREFLIGHTS`: `1` (Live inspection executed on `flow_acceptance_01.mp4` via authenticated profile `profile_2`)
  - `FLOW_PAID_CLICKS`: `0` (**ABSOLUTE ZERO** - Preflight boundary enforced, no submission attempt created)
  - `FLOW_LIVE_GENERATIONS`: `0`
  - `FLOW_CREDITS_SPENT`: `0`
  - `PRUNA_CALLS`: `0` (`DEFERRED_NOT_CONFIGURED`)
  - `BRIA_CALLS`: `0` (`DEFERRED_NOT_CONFIGURED`)
- **Phase Decision**: **`PHASE_FLOW_P3A_FREEZE_STATUS = PASSED`**
  - Production preflight API (`preflight_flow_generation`) successfully validates requests, resolves project media via canonical `mediaId`, verifies browser session state, inspects live cost estimates, and halts before the paid Generate click.
  - Zero mock status returned in live environment; real Flow session communication confirmed.
  - Full automated regression test suite (`220 / 220` tests) passing with zero errors.

---

## 2. Architecture & Design Implementation

### 2.1. Dedicated Preflight Lifecycle vs Submission Pipeline
Unlike `start_flow_generation` which creates a persistent `FlowGenerationManifest`, spawns a background worker, and clicks the paid Generate button, `preflight_flow_generation`:
1. **Canonical Media Resolution**: Securely resolves `sourceMediaId` (or fallback active media) within the project boundary without accepting unconfined absolute paths from the client.
2. **Intent & Mode Validation**: Strictly validates transformation intent and identity mode, resolving empty prompts for `FACE_REPLACE` + `GENERATED` to the system-default deterministic preservation prompt.
3. **Non-Submitting Sidecar Inspection**: Connects to the authenticated Playwright sidecar (`dryRunPreflight`), opens the project workspace, verifies true video edit mode (`/edit/`), attaches the video, applies settings, and inspects the live credit tooltip and credit balance.
4. **Hard Stop Invariant**: Closes the inspection session immediately without generating a submission attempt ID, mutating the job store, or clicking Generate.

```
[UI / IPC Request] (projectId, sourceMediaId, profileId, intent, identityMode)
       │
       ▼
[Backend Canonical Media Resolver] (Validates project path confinement & probes facts)
       │
       ▼
[FlowOrchestrator::preflight_flow_generation] (Prompt validation, SystemDefault fallback)
       │
       ▼
[Playwright Sidecar: dryRunPreflight]
   ├── Workspace verification
   ├── Video attach & True /edit/ mode activation
   ├── Prompt entry & Settings configuration
   └── Live credit tooltip & balance readback
       │
       ▼
[FlowGenerationPreflight Result] ──► [UI Preflight Banner Display] (Hard Stop before Paid Click)
```

### 2.2. Typed Preflight Contract (`FlowGenerationPreflight`)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowGenerationPreflight {
    pub project_id: String,
    pub source_media_id: String,
    pub profile_id: String,
    pub transformation_intent: TransformationIntent,
    pub identity_mode: IdentityMode,
    pub resolved_prompt: String,
    pub prompt_source: PromptSource,
    pub prompt_hash: String,
    pub video_attached: bool,
    pub video_edit_active: bool,
    pub configured_model: Option<String>,
    pub configured_duration: Option<f64>,
    pub configured_orientation: Option<String>,
    pub output_count: u32,
    pub live_displayed_credit_cost: Option<u32>,
    pub live_credit_balance: Option<u32>,
    pub ready_for_paid_submission: bool,
    pub blocking_code: Option<String>,
    pub checked_at: String,
}
```

---

## 3. Real Acceptance & Live Execution Record

### 3.1. Test Video Asset
- **File**: `test-assets/phase20c/videos/flow_acceptance_01.mp4`
- **Duration**: `9.989s` (Video: 299 frames @ 30.0 fps, 576x1024 vertical 9:16)
- **SHA-256**: `68747585122B46F78168F951AA43E461DBAFE19E4DFBA6D519578A004F8D1694`
- **Subject**: 1 visible central person speaking.

### 3.2. Live Preflight Readback Record (FLOW-P3-A.2 Proven True Video Edit)
```json
{
  "projectId": "proj-c8f40218-a09e-439d-8322-80bfcdf8e407",
  "sourceMediaId": "media_85bef16c-8b81-4894-9ae9-2d677598b297",
  "profileId": "profile_2",
  "transformationIntent": "FACE_REPLACE",
  "identityMode": "GENERATED",
  "promptSource": "SYSTEM_DEFAULT",
  "resolvedPrompt": "Replace only the selected target person's facial identity with a new, temporally consistent synthetic identity. Strictly preserve: body, clothing, hair where practical, pose, expression dynamics, mouth movement, head movement, action, camera motion, background, lighting, composition, timing, and all non-target people.",
  "promptHash": "2e39321365a792f3f735938d88165d0cf1e486fa71b33172c79bc16c448215ef",
  "videoAttached": true,
  "videoEditActive": true,
  "configurationVerified": true,
  "costProvenance": "UPLOADED_VIDEO_EDIT",
  "observedSourceTitle": "flow_acceptance_01.mp4",
  "observedSourceDuration": 9.767,
  "observedModel": "Omni Flash",
  "observedOrientation": "PORTRAIT / 9:16",
  "observedOutputCount": 1,
  "observedGenerationLength": 10.0,
  "liveDisplayedCreditCost": 20,
  "diagnosticComposerCreditCost": null,
  "liveCreditBalance": null,
  "readyForPaidSubmission": true,
  "blockingCode": null,
  "checkedAt": "2026-08-26T03:24:22.793724300+00:00"
}
```

### 3.3. Explicit Preflight Acceptance Record

```
REAL_PROJECT_CREATED: YES
SOURCE_MEDIA_ID: media_85bef16c-8b81-4894-9ae9-2d677598b297
SOURCE_PATH_SENT_AS_MEDIA_ID: NO
PROFILE: profile_2
AUTH_STATUS: READY
FLOW_VIDEO_ATTACHED: YES
TRUE_VIDEO_EDIT_MODE: PASS (/edit/a1f2f945-105e-416e-83c1-40a60bba8839)
EDIT_TIMELINE_ACTIVE: YES
OUTPUT_COUNT: 1
CONFIGURATION_VERIFIED: YES
PROMPT_SOURCE: SYSTEM_DEFAULT
PROMPT_HASH_PRESENT: YES
FLOW_LIVE_DISPLAYED_COST: 20 credits
FLOW_LIVE_CREDIT_BALANCE: UNKNOWN
COST_PROVENANCE: UPLOADED_VIDEO_EDIT
READY_FOR_PAID_SUBMISSION: YES
PREFLIGHT_BLOCKING_CODE: NONE
LOCAL_SUBMISSION_ATTEMPT_ID: NONE
SUBMISSION_STATE: NEVER_ATTEMPTED
CLICK_DISPATCHED: false
FLOW_PAID_CLICKS: 0
FLOW_LIVE_GENERATIONS: 0
FLOW_CREDITS_SPENT: 0
```

- **Live Communication**: Verified real Google Flow profile `profile_2` session.
- **Paid Actions Dispatched**: `0` (Zero clicks, zero spend, zero sidecar submission attempts).

---

## 4. Files Changed

| File | Changes |
|---|---|
| [`src-tauri/sidecars/flow-playwright/src/flow_adapter.ts`](file:///D:/rustProject/autovideo-ai/src-tauri/sidecars/flow-playwright/src/flow_adapter.ts) | Fixed uploaded-video node canvas drag/activation into true `/edit/` mode, eliminated generic composer false positive cost, dynamic source title, scoped tooltip reading. |
| [`src-tauri/src/ai/flow/orchestrator.rs`](file:///D:/rustProject/autovideo-ai/src-tauri/src/ai/flow/orchestrator.rs) | Added `FlowCostProvenance`, enriched `FlowGenerationPreflight` with `configuration_verified`, `cost_provenance`, `diagnostic_composer_credit_cost`, and observed timeline fields. |
| [`src-tauri/src/ai/flow/mod.rs`](file:///D:/rustProject/autovideo-ai/src-tauri/src/ai/flow/mod.rs) | Exported `FlowCostProvenance`. |
| [`src/types/contracts.ts`](file:///D:/rustProject/autovideo-ai/src/types/contracts.ts) & [`src/lib/ipc.ts`](file:///D:/rustProject/autovideo-ai/src/lib/ipc.ts) | Enriched frontend preflight interfaces and `FlowCostProvenance` enum. |
| [`src/features/flow/FlowGenPanel.tsx`](file:///D:/rustProject/autovideo-ai/src/features/flow/FlowGenPanel.tsx) | Updated Preflight Banner to display Cost Provenance badge, Configuration Verification status, and observed model. |
| [`src/stores/__tests__/flowJobStore.test.ts`](file:///D:/rustProject/autovideo-ai/src/stores/__tests__/flowJobStore.test.ts) | Updated preflight mock data with `configurationVerified` and `costProvenance`. |
| [`src-tauri/src/ai/tests_phase_flow_p3a.rs`](file:///D:/rustProject/autovideo-ai/src-tauri/src/ai/tests_phase_flow_p3a.rs) | Added 2 new unit regression tests for cost isolation and authoritative cost reading (7 unit tests total). |

---

## 5. Automated Test & Quality Verification

| Suite | Tests | Result |
|---|---|---|
| `cargo test --lib -- tests_phase_flow_p3a` | 7 unit + 1 live | **PASS** (`7 passed; 0 failed; 1 ignored; finished in 102.63s`) |
| `cargo test --lib test_flow_p3a_real_google_flow_live_preflight_acceptance` | 1 live | **PASS** (`1 passed; 0 failed; finished in 53.70s`) |
| `cargo test --lib -- tests_phase_flow_p2` | 5 unit | **PASS** (`5 passed; 0 failed; finished in 0.09s`) |
| `cargo test --lib -- tests_phase20c` | 13 unit | **PASS** (`13 passed; 0 failed; finished in 0.02s`) |
| `cargo test --lib -- tests_phase20b` | 27 unit | **PASS** (`27 passed; 0 failed; finished in 161.29s`) |
| `cargo test --lib -- prompt_tests` | 32 unit | **PASS** (`32 passed; 0 failed; finished in 13.13s`) |
| `cargo test --lib -- tests_phase20a` | 78 unit | **PASS** (`78 passed; 0 failed; finished in 56.90s`) |
| `npm test` (Vitest) | 60 unit | **PASS** (`60 passed; 0 failed; finished in 0.53s`) |
| `cargo fmt --check` | Formatting check | **PASS** (Zero diffs) |
| `cargo check` | Rust Typecheck | **PASS** (Zero warnings) |
| `npm run build` | Vite production bundle | **PASS** (`built in 6.69s`) |
| **Total Automated Tests** | **222 Tests** | **100% PASS** |

---

## 6. Remaining Limitations & Non-Goals

1. **FLOW-P3-B Deferred**: Paid generation submission was NOT executed during this phase in accordance with the invariant `FLOW_PAID_CLICKS = 0`.
2. **Pruna & BRIA Deferred**: Remain `DEFERRED_NOT_CONFIGURED` awaiting dedicated provider tokens.
3. **Face Replacement with Reference Face**: Blocked by design at capability level (`FLOW_REFERENCE_IDENTITY_NOT_SUPPORTED`) until supported by upstream Google Flow.
