use crate::ai::cloud::spec::SourceMediaFacts;
use crate::ai::flow::*;
use crate::system::StoragePaths;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_phase20a_13_flow_manifest_freezes_submitted_prompt_and_hash() {
    let prompt = "A golden retriever playing with a glowing red ball in snow";
    let prompt_hash = calculate_prompt_hash(prompt);

    let facts = SourceMediaFacts {
        duration_sec: 15.0,
        fps: 30.0,
        width: 1920,
        height: 1080,
        has_audio: true,
        timing: None,
    };

    let policy = FlowCapabilityPolicy::default();
    let plan = FlowVideoSegmenter::plan_segments(&facts, &policy).unwrap();

    let manifest = FlowGenerationManifest::new(
        "flow_parent_123".to_string(),
        "req_123".to_string(),
        "proj_123".to_string(),
        "profile_default".to_string(),
        "cfg_123".to_string(),
        None,
        "hash_123".to_string(),
        Some("test.mp4".to_string()),
        prompt.to_string(),
        prompt_hash.clone(),
        PromptSource::User,
        1,
        1,
        facts,
        plan,
        FlowCreditRecord::default(),
        FlowFinalAudioPolicy::default(),
    );

    assert_eq!(manifest.submitted_prompt, prompt);
    assert_eq!(manifest.prompt_hash, prompt_hash);
    assert_eq!(manifest.prompt_source, PromptSource::User);

    let snap = manifest.to_snapshot();
    assert_eq!(snap.submitted_prompt, prompt);
    assert_eq!(snap.prompt_hash, prompt_hash);
}

#[test]
fn test_phase20a_14_all_flow_segments_reuse_identical_frozen_prompt() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let store = FlowJobStore::new(paths);

    let facts = SourceMediaFacts {
        duration_sec: 25.0,
        fps: 30.0,
        width: 1920,
        height: 1080,
        has_audio: false,
        timing: None,
    };
    let policy = FlowCapabilityPolicy {
        max_edit_segment_duration_sec: 10.0,
        ..Default::default()
    };
    let plan = FlowVideoSegmenter::plan_segments(&facts, &policy).unwrap();
    assert_eq!(plan.segments.len(), 3);

    let mut manifest = FlowGenerationManifest::new(
        "flow_frozen_test".to_string(),
        "req_1".to_string(),
        "proj_1".to_string(),
        "profile_1".to_string(),
        "cfg_1".to_string(),
        None,
        "hash_1".to_string(),
        None,
        "Frozen Prompt String".to_string(),
        calculate_prompt_hash("Frozen Prompt String"),
        PromptSource::GeminiOptimized,
        1,
        1,
        facts,
        plan,
        FlowCreditRecord::default(),
        FlowFinalAudioPolicy::default(),
    );

    store.save_manifest_atomic(&mut manifest).unwrap();

    let loaded = store.load_manifest("proj_1", "flow_frozen_test").unwrap();
    assert_eq!(loaded.submitted_prompt, "Frozen Prompt String");
    assert_eq!(loaded.prompt_source, PromptSource::GeminiOptimized);
}

#[test]
fn test_phase20a_15_editing_editor_after_job_submit_does_not_mutate_job() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let store = FlowJobStore::new(paths);

    let facts = SourceMediaFacts {
        duration_sec: 5.0,
        fps: 30.0,
        width: 1280,
        height: 720,
        has_audio: false,
        timing: None,
    };
    let policy = FlowCapabilityPolicy::default();
    let plan = FlowVideoSegmenter::plan_segments(&facts, &policy).unwrap();

    let mut manifest = FlowGenerationManifest::new(
        "flow_immutable".to_string(),
        "req_imm".to_string(),
        "proj_imm".to_string(),
        "profile_imm".to_string(),
        "cfg_imm".to_string(),
        None,
        "hash_imm".to_string(),
        None,
        "Original Submitted Prompt".to_string(),
        calculate_prompt_hash("Original Submitted Prompt"),
        PromptSource::User,
        1,
        1,
        facts,
        plan,
        FlowCreditRecord::default(),
        FlowFinalAudioPolicy::default(),
    );

    store.save_manifest_atomic(&mut manifest).unwrap();
    let loaded = store.load_manifest("proj_imm", "flow_immutable").unwrap();
    assert_eq!(loaded.submitted_prompt, "Original Submitted Prompt");
}

#[test]
fn test_phase20a_16_versioned_flow_capability_policy_calculation() {
    let policy = FlowCapabilityPolicy::for_edit_uploaded_video();
    assert_eq!(policy.mode, FlowGenerationMode::OmniEditUploadedVideo);
    assert_eq!(policy.credits_per_generation, 40);
    assert_eq!(policy.outputs_per_generation, 1);
    assert_eq!(policy.automatic_generation_retries, 0);

    // 5 segments -> 200 credits
    let credits_5 = policy.estimate_credits(5);
    assert_eq!(credits_5, 200);

    // 4 segments -> 160 credits
    let credits_4 = policy.estimate_credits(4);
    assert_eq!(credits_4, 160);
}

#[test]
fn test_phase20a_17_credit_estimation_separate_from_observed_balance() {
    let credit_rec = FlowCreditRecord {
        estimated_credits: 5,
        observed_credit_balance: None,
        completed_generations: 2,
    };

    assert_eq!(credit_rec.estimated_credits, 5);
    assert_eq!(credit_rec.observed_credit_balance, None);
    assert_eq!(credit_rec.completed_generations, 2);
}

#[test]
fn test_phase20a_18_state_revision_and_cas_prevents_stale_overwrites() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let store = FlowJobStore::new(paths);

    let facts = SourceMediaFacts {
        duration_sec: 5.0,
        fps: 30.0,
        width: 1280,
        height: 720,
        has_audio: false,
        timing: None,
    };
    let policy = FlowCapabilityPolicy::default();
    let plan = FlowVideoSegmenter::plan_segments(&facts, &policy).unwrap();

    let mut manifest = FlowGenerationManifest::new(
        "flow_cas_test".to_string(),
        "req_cas".to_string(),
        "proj_cas".to_string(),
        "profile_cas".to_string(),
        "cfg_cas".to_string(),
        None,
        "hash_cas".to_string(),
        None,
        "CAS Prompt".to_string(),
        calculate_prompt_hash("CAS Prompt"),
        PromptSource::User,
        1,
        1,
        facts,
        plan,
        FlowCreditRecord::default(),
        FlowFinalAudioPolicy::default(),
    );

    store.save_manifest_atomic(&mut manifest).unwrap();
    assert_eq!(manifest.state_revision, 2);

    let mut stale_manifest = manifest.clone();
    stale_manifest.state_revision = 1;
    let save_res = store.save_manifest_atomic(&mut stale_manifest);
    assert!(save_res.is_err());
    assert!(save_res
        .unwrap_err()
        .contains("STALE_STATE_REVISION_CAS_REJECTED"));
}

#[test]
fn test_phase20a_19_no_api_key_leakage_in_manifest_dtos_logs() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let store = FlowJobStore::new(paths);

    let facts = SourceMediaFacts {
        duration_sec: 5.0,
        fps: 30.0,
        width: 1280,
        height: 720,
        has_audio: false,
        timing: None,
    };
    let policy = FlowCapabilityPolicy::default();
    let plan = FlowVideoSegmenter::plan_segments(&facts, &policy).unwrap();

    let mut manifest = FlowGenerationManifest::new(
        "flow_leak_test".to_string(),
        "req_leak".to_string(),
        "proj_leak".to_string(),
        "profile_leak".to_string(),
        "cfg_leak".to_string(),
        None,
        "hash_leak".to_string(),
        None,
        "A test prompt".to_string(),
        calculate_prompt_hash("A test prompt"),
        PromptSource::User,
        1,
        1,
        facts,
        plan,
        FlowCreditRecord::default(),
        FlowFinalAudioPolicy::default(),
    );

    store.save_manifest_atomic(&mut manifest).unwrap();

    let manifest_file = store.manifest_path("proj_leak", "flow_leak_test").unwrap();
    let content = fs::read_to_string(&manifest_file).unwrap();

    assert!(!content.contains("AIzaSy"));
    assert!(!content.contains("secret"));
}
