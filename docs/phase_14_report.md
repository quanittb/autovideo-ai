# AutoVideo AI — Phase 14 Remediation Report
## Production Routing, Capability Truth & Budget Enforcement

---

## 1. Executive Summary

Phase 14 Remediation closes all acceptance gaps by enforcing strict **task-specific capability truth**, **authoritative backend budget limits (USD 3.00 default)**, and a **protected production submission path**. Under this model, paid cloud providers cannot be reached without passing full backend task classification, capability validation, resolution/FPS validation, cost calculation, and `CostGuard` verification.

### Key Remediation Results:
1. **Protected Production Submission Path**: `start_cloud_generation` in [`src-tauri/src/commands/mod.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/commands/mod.rs) now routes through `GenerationRouter::route_with_registry` and rejects local tasks (`TASK_ROUTES_TO_LOCAL_EXECUTION`), unavailable routes (`ROUTING_UNAVAILABLE`), budget violations (`CostLimitExceeded`), and unexecutable provider records (`PROVIDER_UNAVAILABLE`) prior to calling `provider.submit_job()`.
2. **Authoritative Standard Budget (USD 3.00)**: Replaced arbitrary `5.0` defaults with `DEFAULT_STANDARD_JOB_BUDGET_USD` ($3.00). User-supplied budgets are strictly validated in Rust (rejecting NaN, Infinity, negative values).
3. **Truthful Provider Capabilities**: [`ReplicateProvider`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/providers/replicate.rs) advertises only `supports_text_to_video: true` matching its actual `prompt` & `prompt_optimizer` request serialization. Unsupported claims (`supports_video_to_video`, `supports_reference_image`, `supports_character_reference`, `supports_audio`) have been removed.
4. **Guarded Character Replacement & Background Removal**: `CHARACTER_REPLACEMENT` and `BACKGROUND_REMOVAL` are classified into their desired execution classes (`SPECIALIZED_VIDEO_TRANSFORMATION` and `UTILITY_CLOUD`) but correctly set to `RoutingTarget::Unavailable` with `auto_submit_allowed: false` until real adapters are implemented in Phase 16 and Phase 17.
5. **Unified Cost & Routing Truth**: `get_cloud_cost_estimate`, `get_generation_route`, and `start_cloud_generation` share the same authoritative `ProviderRegistry` and `CostBreakdown` pipeline.

---

## 2. Preflight & Commit Metadata

- **Starting HEAD**: `040793a04d63f369b92f5b6401284e31c5a1325e`
- **Branch**: `main`
- **Working Tree**: Clean prior to remediation edits.

---

## 3. Files Changed

### Backend Rust Core
- [x] [`src-tauri/src/ai/cloud/providers/replicate.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/providers/replicate.rs): Set truthful capability declaration matching actual `submit_job()` serialization.
- [x] [`src-tauri/src/ai/cloud/registry.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/registry.rs): Updated provider records with truthful capabilities and added `has_executable_adapter()` verification.
- [x] [`src-tauri/src/ai/cloud/router.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/router.rs): Implemented `TaskClass::from_str_or_default()`, guarded unexecutable character-replacement / background-removal routes, and enforced local deterministic preference.
- [x] [`src-tauri/src/ai/cloud/cost.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/cloud/cost.rs): Added `CostGuard::validate_budget()` and enforced budget checking on `CostBreakdown`.
- [x] [`src-tauri/src/commands/mod.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/commands/mod.rs): Integrated router and budget checks into `start_cloud_generation` and unified cost estimation.
- [x] [`src-tauri/src/ai/tests_cloud_mvp.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/ai/tests_cloud_mvp.rs): Added dedicated production-path integration tests (Tests A–H) and historical project fixture deserialization.

### Frontend TypeScript Bridge
- [x] [`src/lib/ipc.ts`](file:///d:/rustProject/autovideo-ai/src/lib/ipc.ts): Defined `CostEstimate` interface and strongly typed `getCostEstimate: Promise<CostEstimate>`.

---

## 4. Phase 14 Requirement Status Mapping

| Requirement | Description | Status |
|---|---|---|
| **Req 1** | Single `ProviderRegistry` with complete pricing/source metadata | `PASS` |
| **Req 2** | Capabilities valid only if adapter serializes every input | `PASS` |
| **Req 3** | Structured `CostBreakdown` with billable metrics & confidence | `PASS` |
| **Req 4** | Unknown cost is not zero and strictly blocks submission | `PASS` |
| **Req 5** | Backend budget enforcement is authoritative on submission path | `PASS` |
| **Req 6** | Default budgets: Preview USD 0.25, Standard Full Job USD 3.00 | `PASS` |
| **Req 7** | User settings cannot bypass backend budget validation | `PASS` |
| **Req 8** | Backward compatibility for stored project/task data | `PASS` |
| **Req 9** | IPC/TypeScript types aligned without loose `any` | `PASS` |
| **Req 10** | Provider prices derived from metadata, not hardcoded UI text | `PASS` |
| **Req 11** | Price refresh supported dynamically without code changes | `PASS` |
| **Blocker A** | Paid submission (`start_cloud_generation`) cannot bypass routing | `PASS` |
| **Blocker B** | Standard default budget is USD 3.00 (rejecting > $3.00 and NaN/Inf) | `PASS` |
| **Blocker C** | Replicate capabilities truthful to `submit_job` JSON payload | `PASS` |
| **Task Route 1** | Local deterministic tasks (`StyleFilter`, `BackgroundComposite`, `AudioTransformation`) route to FFmpeg ($0.00) and reject cloud submission | `PASS` |
| **Task Route 2** | `CharacterReplacement` returns `SpecializedVideoTransformation` + `Unavailable` until real adapter | `BLOCKED_BY_LATER_PROVIDER_PHASE` (Phase 16) |
| **Task Route 3** | `BackgroundRemoval` returns `UtilityCloud` + `Unavailable` until real adapter | `BLOCKED_BY_LATER_PROVIDER_PHASE` (Phase 17) |

---

## 5. Provider Capability Matrix

| Provider ID | Model ID | Executable Adapter | Text Input | Source Video | Reference Image | Audio Policy | Executable Tasks | Execution Class | Price / Rate | Confidence |
|---|---|---|---|---|---|---|---|---|---|---|
| `local_ffmpeg` | `ffmpeg_native` | **Yes** | N/A | **Yes** | N/A | **Yes** | `StyleFilter`, `BackgroundComposite`, `AudioTransformation` | `LOCAL_DETERMINISTIC` | $0.00 (Free Local) | `EXACT` |
| `replicate` | `minimax/video-01` | **Yes** | **Yes** | No | No | No | Text-to-Video only | `SPECIALIZED_VIDEO_TRANSFORMATION` | $0.0400 / sec | `ESTIMATED` |
| `replicate_utility` | `lucataco/remove-bg` | **No** (Phase 17) | No | No | No | No | None (Deferred to Phase 17) | `UTILITY_CLOUD` | $0.0050 / pred | `UNKNOWN` |
| `local_diffusers` | `sd15-animatediff-v3` | **Yes** | **Yes** | **Yes** | **Yes** | No | Local Generative Fallback | `GENERATIVE_FALLBACK` | $0.00 (Free Local) | `EXACT` |

---

## 6. Submission-Path Proof

The exact call sequence for paid job submission in [`src-tauri/src/commands/mod.rs`](file:///d:/rustProject/autovideo-ai/src-tauri/src/commands/mod.rs) is:
```text
UI / IPC invoke('start_cloud_generation', { request, maxCost })
  │
  ├── 1. CostGuard::validate_budget(max_cost.unwrap_or(DEFAULT_STANDARD_JOB_BUDGET_USD))
  │      └── Rejects NaN, Infinity, negative values
  │
  ├── 2. TaskClass::from_str_or_default(&request.task_type)
  │
  ├── 3. GenerationRouter::route_with_registry(task, CostSaving, &request, &provider, None, &registry)
  │
  ├── 4. Rejection Check: decision.target == RoutingTarget::Local
  │      └── Returns Err("TASK_ROUTES_TO_LOCAL_EXECUTION...")
  │
  ├── 5. Rejection Check: decision.target == RoutingTarget::Unavailable || !decision.auto_submit_allowed
  │      └── Returns Err("ROUTING_UNAVAILABLE...")
  │
  ├── 6. CostGuard::check_breakdown(&decision.cost_breakdown)
  │      ├── Rejects CostConfidence::Unknown or total_usd == None
  │      └── Rejects cost > budget_limit ($3.00 default) with CostLimitExceeded
  │
  ├── 7. Adapter Check: registry.has_executable_adapter(&decision.provider_id)
  │      └── Rejects unimplemented provider adapters
  │
  └── 8. provider.submit_job(&request).await
```

---

## 7. Test Suite Execution & Real Results

```powershell
1. cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
   Exit Code: 0 (No diffs)

2. cargo check --all-targets --manifest-path src-tauri/Cargo.toml
   Exit Code: 0 (0 errors, 0 warnings)

3. cargo test --manifest-path src-tauri/Cargo.toml -- test_phase14 --test-threads=1
   Exit Code: 0 (11 passed; 0 failed)

4. cargo test --manifest-path src-tauri/Cargo.toml -- --test-threads=1
   Exit Code: 0 (616 passed; 0 failed; 0 ignored)

5. npm run build
   Exit Code: 0 (1859 modules transformed and bundled in 10.97s)
```

### Verified Test List:
- `test_phase14_remediation_test_a_local_tasks_cannot_submit_cloud_job`: **PASS**
- `test_phase14_remediation_test_b_character_replacement_blocked_until_real_adapter`: **PASS**
- `test_phase14_remediation_test_c_background_removal_blocked_until_real_adapter`: **PASS**
- `test_phase14_remediation_test_d_default_budget_is_3_usd`: **PASS**
- `test_phase14_remediation_test_e_unknown_price_blocks_submission`: **PASS**
- `test_phase14_remediation_test_f_invalid_budget_values_rejected`: **PASS**
- `test_phase14_remediation_test_g_replicate_adapter_truthful_capabilities`: **PASS**
- `test_phase14_remediation_test_h_provider_registry_adapter_verification`: **PASS**
- `test_phase14_remediation_historical_project_fixture_deserialization`: **PASS**
- `test_phase14_remediation_task_class_string_aliases`: **PASS**
- `test_phase14_remediation_dynamic_price_refresh`: **PASS**

---

## 8. Incurred Cost

- **Live Paid Cloud Calls**: `$0.00` (Zero paid cloud calls made).

---

## 9. Remaining Limitations

- Real cloud character-replacement provider (`prunaai/p-video-replace`) is deferred to Phase 16.
- Real cloud background-removal provider (Bria / BiRefNet / fal) is deferred to Phase 17.
- Live cloud acceptance runner (`cloud_live_acceptance.py`) remains blocked until `REPLICATE_API_TOKEN` is supplied by the user.

---

## 10. Final Status

**STATUS: `PHASE_COMPLETED`**
