# Phase 16 — Character Replacement Provider Integration & Safety Hardening Report

**Target Model Candidate**: `prunaai/p-video-replace` (Replicate official candidate)  
**Starting HEAD**: `22b9627867f276a3c9f95a56b35f9ffe3a3a1534`  
**Fix Implementation Commit**: `c2db733`  
**Official Facts Observed Date**: `2026-08-20`  
**Paid / Live Costs Incurred in Phase 16**: **$0.00** (Zero paid/live inference calls executed)  
**Live Uploads Executed**: **0**  
**Live Predictions Executed**: **0**  
**LIVE QUALITY VERIFIED**: **NO**  
**LIVE PAID INFERENCE EXECUTED**: **NO**  

---

## 1. Summary of Hardening Changes

During post-implementation audit, the following safety hardening enhancements were introduced to close production contract boundaries:

1. **Elimination of Fabricated Generic `create_prediction` Bridge**:
   - `CloudVideoProvider::create_prediction` default implementation returns `PREPARED_SUBMISSION_UNSUPPORTED` immediately with 0 uploads, 0 predictions, 0 network, and never fabricates a `CloudJobRequest` or fake task semantics.
   - Provider adapters that support prepared submissions implement `create_prediction` explicitly.

2. **Pruna Raw `submit_job` Fails Closed**:
   - `PrunaPVideoReplaceProvider::submit_job` rejects raw `CloudJobRequest` with `RAW_SUBMISSION_UNSUPPORTED`.
   - The production path only executes via `PreparedProviderSubmission` built from validated requests, project audio policies, and verified uploaded assets.

3. **Truthful Capabilities & Unknown Max Duration**:
   - `ProviderCapabilities.max_duration_sec` and `ProviderRecord.max_duration_sec` are strongly typed as `Option<f64>`.
   - For `prunaai/p-video-replace`, `max_duration_sec = None` because official documentation does not define a fixed duration limit. Capabilities and registry records are in 100% agreement.

4. **Target FPS Semantics & Source Framerate Preservation**:
   - `TargetFps::Original` maps all non-override framerates (e.g. 23.976, 25, 29.97, 30, 50, 59.94, 60 fps) to `Original` to preserve source video timing.
   - `TargetFps::Fps24` and `TargetFps::Fps48` handle explicit 24/48 target FPS options.
   - `ProviderRecord.supports_original_fps = true` allows routing arbitrary valid source framerates without claiming unsupported numbers as generated options.

5. **Explicit Resolution Preset Mapping**:
   - `ResolutionTier::from_dimensions` maps known AutoVideo AI presets (e.g., 720x1280, 1080x1920, 576x1024, 512x512, 1280x720, 1920x1080) to `P720` or `P1080`.
   - Arbitrary unmapped resolutions (e.g. 123x456, 9999x9999) fail closed with `UNSUPPORTED_RESOLUTION_PRESET` without guessing.

---

## 2. Phase 16 Test Coverage Matrix

| Scenario / Contract Requirement | Concrete Test(s) | Result |
|---|---|---|
| Live guard disabled by default | `test_phase16_01_paid_live_disabled_by_default` | PASSED |
| Block live upload/submit when guard disabled | `test_phase16_02_new_request_when_live_disabled_fails_safe_without_upload_or_prediction` | PASSED |
| Offline recovery, polling, cancellation without live guard | `test_phase16_03_existing_job_recovery_poll_cancel_download_works_when_live_guard_disabled` | PASSED |
| ProviderKey hashing and equality | `test_phase16_04_provider_key_equality_and_hashing` | PASSED |
| Multi-model registry compound indexing | `test_phase16_05_registry_records_unique_by_compound_key` | PASSED |
| Deterministic model selection order-independent | `test_phase16_06_registry_deterministic_selection_order_independent` | PASSED |
| Pruna cost estimate model-aware | `test_phase16_07_pruna_estimate_cost_model_aware_never_uses_minimax` | PASSED |
| Router independent of provider instances | `test_phase16_08_route_with_registry_without_provider_instance` | PASSED |
| Strict TaskClass parsing | `test_phase16_09_strict_task_class_parsing_rejects_unknown` | PASSED |
| Resolution tier and FPS routing | `test_phase16_10_character_replacement_resolution_and_fps_routing` | PASSED |
| Action regeneration unsupported isolation | `test_phase16_11_action_regeneration_unsupported_isolated` | PASSED |
| Reference images normalization and conflict check | `test_phase16_12_reference_images_normalization_and_conflict_rejection` | PASSED |
| Audio preservation policy derivation | `test_phase16_13_save_audio_derived_from_project_policy` | PASSED |
| Prepared submission uses uploaded remote URIs | `test_phase16_14_prepared_provider_submission_uses_uploaded_uris_never_local_paths` | PASSED |
| Pruna serializer consumes spec | `test_phase16_15_replicate_pruna_serializer_consumes_spec_not_raw_request` | PASSED |
| Uploader authentication requirement | `test_phase16_16_replicate_uploader_requires_token_and_valid_file` | PASSED |
| Mock uploader tracking | `test_phase16_17_mock_uploader_tracks_calls_and_returns_valid_delivery_uris` | PASSED |
| Upload failure never becomes ambiguous | `test_phase16_18_upload_failure_never_becomes_ambiguous_submission` | PASSED |
| Prediction failure transitions to ambiguous and blocked | `test_phase16_20_prediction_create_failure_transitions_to_ambiguous_and_blocked` | PASSED |
| SSRF allows replicate.delivery and subdomains | `test_phase16_21_ssrf_allows_replicate_delivery_and_subdomains` | PASSED |
| SSRF rejects replicate.com and third parties | `test_phase16_22_ssrf_rejects_replicate_com_and_third_parties` | PASSED |
| SSRF rejects private IPs / localhost / redirects | `test_phase16_23_ssrf_rejects_localhost_private_ips_and_redirects` | PASSED |
| ValidatingOutput promotes local artifact | `test_phase16_24_validating_output_recovery_promotes_existing_final_artifact_with_zero_submits_and_downloads` | PASSED |
| Uploading state restart resets to Created | `test_phase16_25_uploading_state_on_restart_resets_safely_never_ambiguous` | PASSED |
| Backward compatible manifest deserialization | `test_phase16_26_legacy_singular_input_assets_manifest_deserialization` | PASSED |
| Actual cost remains None without monetary data | `test_phase16_27_actual_cost_remains_none_unless_monetary_amount_present` | PASSED |
| Full mocked lifecycle with Phase 16 fixture | `test_phase16_28_full_mocked_character_replacement_lifecycle_with_phase16_fixture` | PASSED |
| IPC contract camelCase serialization | `test_phase16_30_ipc_contract_reference_images_camel_case` | PASSED |
| Multiple reference images (1-3) supported | `test_phase16_31_multiple_reference_images_up_to_3_supported` | PASSED |
| > 3 reference images rejected | `test_phase16_32_more_than_3_reference_images_rejected` | PASSED |
| 0 reference images rejected for CharacterReplacement | `test_phase16_33_zero_reference_images_rejected_for_character_replacement` | PASSED |
| `disable_safety_checker` always false | `test_phase16_34_disable_safety_checker_always_false_in_spec` | PASSED |
| Pruna prioritized over generic models | `test_phase16_35_router_model_selection_pruna_chosen_over_minimax_for_character_replacement` | PASSED |
| Generic `create_prediction` fails closed | `test_phase16_36_generic_create_prediction_fails_closed_zero_fabrication` | PASSED |
| Pruna raw `submit_job` fails closed | `test_phase16_37_pruna_raw_submit_job_fails_closed_zero_network` | PASSED |
| Unknown max duration truthful `None` | `test_phase16_38_truthful_capabilities_unknown_max_duration` | PASSED |
| Original FPS preserves source framerate | `test_phase16_39_original_fps_preserves_source_framerate` | PASSED |
| Unsupported target FPS not advertised | `test_phase16_40_unsupported_target_fps_not_claimed_as_generated_option` | PASSED |
| Explicit resolution mapping (no arbitrary guessing) | `test_phase16_41_explicit_resolution_tier_mapping_and_no_arbitrary_thresholds` | PASSED |

---

## 3. Test Verification Results

- **Phase 16 Suite**: `cargo test -- test_phase16 --test-threads=1` → **39 passed; 0 failed**
- **Phase 15 Suite**: `cargo test -- test_phase15 --test-threads=1` → **38 passed; 0 failed**
- **Phase 14 Suite**: `cargo test -- test_phase14 --test-threads=1` → **10 passed; 0 failed**
- **Cloud MVP Suite**: `cargo test -- test_cloud --test-threads=1` → **6 passed; 0 failed**
- **Full Rust Test Suite**: `cargo test -- --test-threads=1` → **692 passed; 0 failed**
- **Cargo Format Check**: `cargo fmt -- --check` → **Passed (0 violations)**
- **Cargo Type Check**: `cargo check --all-targets` → **Passed (0 warnings, 0 errors)**
- **Frontend Build**: `npm run build` → **Passed (0 errors, 4.52s)**
- **Fixture Verification**: `ffprobe src-tauri/target/phase16_test_artifact.mp4` → **Valid H.264 720x1280 @ 24fps + AAC**

---

## 4. Cost Incurred in Phase 16

| Item | Calls | Cost USD |
|---|---|---|
| Replicate Files Uploads (Live) | 0 | $0.00 |
| Replicate Predictions (Live) | 0 | $0.00 |
| **Total Incurred in Phase 16** | **0** | **$0.00** |
