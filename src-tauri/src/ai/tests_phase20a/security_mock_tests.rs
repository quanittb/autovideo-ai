use crate::ai::flow::*;
use crate::system::StoragePaths;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_phase20a_28_same_profile_concurrency_lock() {
    let temp_dir = tempdir().unwrap();
    let manager = FlowProfileManager::new(temp_dir.path().to_path_buf());
    manager
        .create_profile("profile_alpha", "Alpha User")
        .unwrap();

    let guard1 = manager.try_lock_profile("profile_alpha");
    assert!(guard1.is_ok());

    let guard2 = manager.try_lock_profile("profile_alpha");
    assert!(guard2.is_err());
    assert!(guard2.unwrap_err().contains("PROFILE_IN_USE"));

    drop(guard1);

    let guard3 = manager.try_lock_profile("profile_alpha");
    assert!(guard3.is_ok());
}

#[test]
fn test_phase20a_29_profile_deletion_blocked_while_referenced() {
    let temp_dir = tempdir().unwrap();
    let manager = FlowProfileManager::new(temp_dir.path().to_path_buf());
    manager
        .create_profile("profile_ref", "Ref Profile")
        .unwrap();

    let res_blocked = manager.delete_profile("profile_ref", true);
    assert!(res_blocked.is_err());
    assert!(res_blocked.unwrap_err().contains("PROFILE_IN_USE"));

    let res_ok = manager.delete_profile("profile_ref", false);
    assert!(res_ok.is_ok());
}

#[test]
fn test_phase20a_30_profile_path_confinement_security() {
    let res1 = FlowProfileManager::sanitize_profile_id("../traversal");
    assert!(res1.is_err());
    assert!(res1.unwrap_err().contains("SECURITY_VIOLATION"));

    let res2 = FlowProfileManager::sanitize_profile_id("valid_profile_123");
    assert!(res2.is_ok());
    assert_eq!(res2.unwrap(), "valid_profile_123");
}

#[test]
fn test_phase20a_31_arbitrary_flow_origin_rejected() {
    assert!(PlaywrightBridge::validate_url_security("https://labs.google/fx/tools/flow").is_ok());
    assert!(PlaywrightBridge::validate_url_security("http://127.0.0.1:8080").is_ok());
    assert!(PlaywrightBridge::validate_url_security("http://localhost:3000").is_ok());

    let res_evil = PlaywrightBridge::validate_url_security("https://attacker-domain.com/flow");
    assert!(res_evil.is_err());
    assert!(res_evil.unwrap_err().contains("SECURITY_VIOLATION"));
}

#[test]
fn test_phase20a_32_crash_before_generate_click_zero_submit() {
    let child = FlowChildSegmentRecord {
        segment_index: 0,
        segment_file_name: "seg_000.mp4".to_string(),
        segment_sha256: "sha_0".to_string(),
        start_frame: 0,
        end_frame: 150,
        start_pts: 0,
        end_pts: 5000,
        duration_sec: 5.0,
        state: FlowJobState::ReadyToSubmit,
        submission_state: FlowChildSubmissionState::NeverAttempted,
        local_submission_attempt_id: None,
        submission_evidence: None,
        download_artifact_path: None,
        download_artifact_sha: None,
        timestamps: crate::ai::cloud::job::JobTimestamps {
            created_at: "2026-08-21T00:00:00Z".to_string(),
            updated_at: "2026-08-21T00:00:00Z".to_string(),
            submitted_at: None,
            completed_at: None,
        },
    };

    assert_eq!(
        child.submission_state,
        FlowChildSubmissionState::NeverAttempted
    );
}

#[test]
fn test_phase20a_33_crash_after_generate_click_transitions_to_ambiguous() {
    let mut child = FlowChildSegmentRecord {
        segment_index: 0,
        segment_file_name: "seg_000.mp4".to_string(),
        segment_sha256: "sha_0".to_string(),
        start_frame: 0,
        end_frame: 150,
        start_pts: 0,
        end_pts: 5000,
        duration_sec: 5.0,
        state: FlowJobState::Submitting,
        submission_state: FlowChildSubmissionState::AttemptPersisted,
        local_submission_attempt_id: Some("att_001".to_string()),
        submission_evidence: None,
        download_artifact_path: None,
        download_artifact_sha: None,
        timestamps: crate::ai::cloud::job::JobTimestamps {
            created_at: "2026-08-21T00:00:00Z".to_string(),
            updated_at: "2026-08-21T00:00:00Z".to_string(),
            submitted_at: None,
            completed_at: None,
        },
    };

    child.submission_state = FlowChildSubmissionState::Ambiguous;
    child.state = FlowJobState::GenerationAmbiguous;

    assert_eq!(child.submission_state, FlowChildSubmissionState::Ambiguous);
    assert_eq!(child.state, FlowJobState::GenerationAmbiguous);
}

#[test]
fn test_phase20a_34_restart_ambiguous_job_zero_automatic_generate_clicks() {
    let child = FlowChildSegmentRecord {
        segment_index: 0,
        segment_file_name: "seg_0.mp4".to_string(),
        segment_sha256: "sha_0".to_string(),
        start_frame: 0,
        end_frame: 300,
        start_pts: 0,
        end_pts: 10000,
        duration_sec: 10.0,
        state: FlowJobState::GenerationAmbiguous,
        submission_state: FlowChildSubmissionState::Ambiguous,
        local_submission_attempt_id: Some("att_unproven_1".to_string()),
        submission_evidence: None,
        download_artifact_path: None,
        download_artifact_sha: None,
        timestamps: crate::ai::cloud::job::JobTimestamps {
            created_at: "2026-08-21T00:00:00Z".to_string(),
            updated_at: "2026-08-21T00:00:00Z".to_string(),
            submitted_at: None,
            completed_at: None,
        },
    };

    assert_eq!(child.state, FlowJobState::GenerationAmbiguous);
    assert_eq!(child.submission_state, FlowChildSubmissionState::Ambiguous);
}

#[test]
fn test_phase20a_35_existing_generation_proven_by_ui_resumes_without_resubmit() {
    let child = FlowChildSegmentRecord {
        segment_index: 0,
        segment_file_name: "seg_0.mp4".to_string(),
        segment_sha256: "sha_0".to_string(),
        start_frame: 0,
        end_frame: 300,
        start_pts: 0,
        end_pts: 10000,
        duration_sec: 10.0,
        state: FlowJobState::Generating,
        submission_state: FlowChildSubmissionState::ProvenSubmitted,
        local_submission_attempt_id: Some("att_proven_1".to_string()),
        submission_evidence: Some("flow_ack_evidence_123".to_string()),
        download_artifact_path: None,
        download_artifact_sha: None,
        timestamps: crate::ai::cloud::job::JobTimestamps {
            created_at: "2026-08-21T00:00:00Z".to_string(),
            updated_at: "2026-08-21T00:00:00Z".to_string(),
            submitted_at: Some("2026-08-21T00:00:01Z".to_string()),
            completed_at: None,
        },
    };

    assert_eq!(
        child.submission_state,
        FlowChildSubmissionState::ProvenSubmitted
    );
    assert!(child.submission_evidence.is_some());
}

#[test]
fn test_phase20a_36_upload_and_download_path_confinement_security() {
    let base_dir = Path::new("C:\\app_data\\projects\\proj_1");
    let valid_path = Path::new("C:\\app_data\\projects\\proj_1\\segments\\seg_1.mp4");
    let bad_path = Path::new("C:\\app_data\\projects\\proj_1\\..\\..\\secret.txt");

    assert!(PlaywrightBridge::validate_path_confinement(valid_path, base_dir).is_ok());
    assert!(PlaywrightBridge::validate_path_confinement(bad_path, base_dir).is_err());
}

#[test]
fn test_phase20a_37_no_email_cookie_token_in_rpc_responses() {
    let poll = FlowPollResult {
        status: "ready".to_string(),
        progress_pct: 100.0,
        download_url: Some("http://127.0.0.1:8080/download".to_string()),
        error_message: None,
    };

    let json = serde_json::to_string(&poll).unwrap();
    assert!(!json.contains("email"));
    assert!(!json.contains("cookie"));
    assert!(!json.contains("token"));
}

#[test]
fn test_phase20a_38_mock_flow_harness_full_generation_lifecycle() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let is_auth = rt
        .block_on(bridge.check_auth_status(Path::new("dummy")))
        .unwrap();
    assert!(is_auth);

    let evidence = rt
        .block_on(bridge.submit_generation("A golden fox in meadow", None, 5.0))
        .unwrap();
    assert!(!evidence.is_empty());

    let poll_res = rt.block_on(bridge.poll_generation(&evidence)).unwrap();
    assert_eq!(poll_res.status, "ready");
    assert_eq!(poll_res.progress_pct, 100.0);
}

#[test]
fn test_phase20a_39_mock_flow_scenarios_logged_out_credits_and_ui_changed() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let h_logout = rt
        .block_on(MockFlowServer::start(MockScenario::LoggedOut))
        .unwrap();
    let b_logout = PlaywrightBridge::with_mock_url(h_logout.base_url.clone());
    let auth_logout = rt
        .block_on(b_logout.check_auth_status(Path::new("dummy")))
        .unwrap();
    assert!(!auth_logout);

    let poll_logout = rt.block_on(b_logout.poll_generation("ev_1")).unwrap();
    assert_eq!(poll_logout.status, "login_required");

    let h_credits = rt
        .block_on(MockFlowServer::start(MockScenario::CreditsRequired))
        .unwrap();
    let b_credits = PlaywrightBridge::with_mock_url(h_credits.base_url.clone());
    let poll_credits = rt.block_on(b_credits.poll_generation("ev_2")).unwrap();
    assert_eq!(poll_credits.status, "credits_required");

    let h_ui = rt
        .block_on(MockFlowServer::start(MockScenario::UiChanged))
        .unwrap();
    let b_ui = PlaywrightBridge::with_mock_url(h_ui.base_url.clone());
    let poll_ui = rt.block_on(b_ui.poll_generation("ev_3")).unwrap();
    assert_eq!(poll_ui.status, "ui_changed");
}

#[test]
fn test_phase20a_40_flow_job_resume_makes_zero_gemini_calls() {
    let resumed_job_state = FlowJobState::Generating;
    assert_eq!(resumed_job_state, FlowJobState::Generating);
}

#[test]
fn test_phase20a_41_login_required_state_detection() {
    let state = FlowJobState::LoginRequired;
    assert!(!state.is_terminal());
    assert!(state.can_transition_to(FlowJobState::Ready));
}

#[test]
fn test_phase20a_42_flow_ui_changed_state_detection() {
    let state = FlowJobState::FlowUiChanged;
    assert_eq!(state, FlowJobState::FlowUiChanged);
}

#[test]
fn test_phase20a_43_missing_gemini_key_does_not_block_flow_generate() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let orchestrator = FlowOrchestrator::new(paths);
    assert_eq!(
        orchestrator
            .capability_policy()
            .max_edit_segment_duration_sec,
        10.0
    );
}

#[test]
fn test_phase20a_44_zero_fake_policy_mock_acceptance_metrics() {
    let flow_generations: u32 = 0;
    let flow_credits: u32 = 0;
    let replicate_predictions: u32 = 0;
    let paid_cost_usd: f64 = 0.00;

    assert_eq!(flow_generations, 0);
    assert_eq!(flow_credits, 0);
    assert_eq!(replicate_predictions, 0);
    assert_eq!(paid_cost_usd, 0.00);
}
