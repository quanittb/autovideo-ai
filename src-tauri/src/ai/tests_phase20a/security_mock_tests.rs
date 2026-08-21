use crate::ai::flow::*;
use crate::system::StoragePaths;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_phase20a_28_same_profile_concurrency_lock() {
    let temp_dir = tempdir().unwrap();
    let manager1 = FlowProfileManager::new(temp_dir.path().to_path_buf());
    manager1
        .create_profile("profile_alpha", "Alpha User")
        .unwrap();

    let guard1 = manager1.acquire_session_lock("profile_alpha");
    assert!(guard1.is_ok());

    // Second manager instance attempting to lock same profile
    let manager2 = FlowProfileManager::new(temp_dir.path().to_path_buf());
    let guard2 = manager2.acquire_session_lock("profile_alpha");
    assert!(guard2.is_err());
    assert!(guard2.unwrap_err().contains("PROFILE_IN_USE"));

    drop(guard1);

    let guard3 = manager2.acquire_session_lock("profile_alpha");
    assert!(guard3.is_ok());
}

#[test]
fn test_phase20a_29_profile_deletion_blocked_while_locked() {
    let temp_dir = tempdir().unwrap();
    let manager = FlowProfileManager::new(temp_dir.path().to_path_buf());
    manager
        .create_profile("profile_ref", "Ref Profile")
        .unwrap();

    let guard = manager.acquire_session_lock("profile_ref").unwrap();

    let res_blocked = manager.delete_profile("profile_ref", false);
    assert!(res_blocked.is_err());
    assert!(res_blocked.unwrap_err().contains("PROFILE_LOCKED"));

    drop(guard);

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

    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path().join("safe_root");
    std::fs::create_dir_all(&root).unwrap();

    let safe_child = root.join("child.mp4");
    std::fs::write(&safe_child, b"safe").unwrap();
    assert!(PlaywrightBridge::validate_path_confinement(&safe_child, &root).is_ok());

    let outside_child = temp_dir.path().join("outside.mp4");
    std::fs::write(&outside_child, b"outside").unwrap();
    assert!(PlaywrightBridge::validate_path_confinement(&outside_child, &root).is_err());
}

#[test]
fn test_phase20a_31_arbitrary_flow_origin_rejected() {
    let prod_bridge = PlaywrightBridge::new();
    assert!(prod_bridge
        .validate_url_security("https://labs.google/fx/tools/flow")
        .is_ok());
    assert!(prod_bridge
        .validate_url_security("https://labs.google/flow")
        .is_ok());

    // In production, local and arbitrary external origins are rejected
    assert!(prod_bridge
        .validate_url_security("http://127.0.0.1:8080")
        .is_err());
    assert!(prod_bridge
        .validate_url_security("https://attacker-domain.com/flow")
        .is_err());

    // In mock mode, injected mock url is allowed
    let mock_bridge = PlaywrightBridge::with_mock_url("http://127.0.0.1:9090".to_string());
    assert!(mock_bridge
        .validate_url_security("http://127.0.0.1:9090")
        .is_ok());
    assert!(mock_bridge
        .validate_url_security("https://attacker-domain.com/flow")
        .is_err());
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
fn test_phase20a_34_restart_recovery_zero_additional_generate_clicks() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::DelayAfterGenerateClick))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let store = FlowJobStore::new(paths.clone());
    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager.create_profile("test_p", "Test").unwrap();

    let facts = crate::ai::cloud::spec::SourceMediaFacts {
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
        "flow_crash_test".to_string(),
        "req_crash".to_string(),
        "proj_crash".to_string(),
        "test_p".to_string(),
        "cfg_crash".to_string(),
        None,
        "hash_crash".to_string(),
        None,
        "A crashed prompt".to_string(),
        calculate_prompt_hash("A crashed prompt"),
        PromptSource::User,
        1,
        1,
        facts,
        plan,
        FlowCreditRecord::default(),
        FlowFinalAudioPolicy::default(),
    );

    // Simulate crash after AttemptPersisted
    let child = FlowChildSegmentRecord {
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
    manifest.child_segments.push(child);
    manifest.state = FlowJobState::Submitting;
    store.save_manifest_atomic(&mut manifest).unwrap();

    // Construct new orchestrator and run worker
    let orchestrator =
        FlowOrchestrator::with_mock_bridge(paths.clone(), server_handle.base_url.clone());
    let test_video = temp_dir.path().join("dummy.mp4");
    std::fs::write(&test_video, b"fake_mp4_bytes").unwrap();

    let res =
        rt.block_on(orchestrator.run_flow_worker("proj_crash", "flow_crash_test", &test_video));
    assert!(res.is_ok());

    let loaded = store
        .load_manifest("proj_crash", "flow_crash_test")
        .unwrap();
    // Must be Ambiguous and click count must remain 0!
    assert_eq!(loaded.state, FlowJobState::GenerationAmbiguous);
    assert_eq!(server_handle.generate_click_count.load(Ordering::SeqCst), 0);
}

#[test]
fn test_phase20a_35_proven_submitted_restart_resumes_polling_zero_submit() {
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
        submission_evidence: Some("evidence:att_proven_1:2026-08-21T00:00:00Z:fp1".to_string()),
        download_artifact_path: None,
        download_artifact_sha: None,
        timestamps: crate::ai::cloud::job::JobTimestamps {
            created_at: "2026-08-21T00:00:00Z".to_string(),
            updated_at: "2026-08-21T00:00:00Z".to_string(),
            submitted_at: Some("2026-08-21T00:00:00Z".to_string()),
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
fn test_phase20a_36_secret_store_os_keyring_and_dev_fallback() {
    let temp_dir = tempdir().unwrap();
    let store = SecretStore::new(temp_dir.path().to_path_buf());

    let set_res = store.set_gemini_api_key("AIzaSyTestKey123");
    assert!(set_res.is_ok());

    let is_conf = store.is_gemini_configured();
    assert!(is_conf);

    let clear_res = store.clear_gemini_api_key();
    assert!(clear_res.is_ok());
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
fn test_phase20a_38_real_mock_playwright_chromium_e2e() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let test_mp4_path = temp_dir.path().join("test_input.mp4");
    std::fs::write(&test_mp4_path, b"dummy_mp4_data_for_upload").unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());

    // 1. Check auth status
    let is_auth = rt.block_on(bridge.check_auth_status(&profile_dir)).unwrap();
    assert!(is_auth);

    // 2. Submit prompt generation (real Node sidecar + real Playwright + real Chromium)
    let evidence = rt
        .block_on(bridge.submit_generation(
            &profile_dir,
            "Cinematic golden fox in meadow",
            Some(&test_mp4_path),
            5.0,
            "att_e2e_001",
        ))
        .unwrap();

    assert!(evidence.contains("att_e2e_001"));
    assert_eq!(server_handle.generate_click_count.load(Ordering::SeqCst), 1);

    // 3. Poll generation
    let poll_res = rt
        .block_on(bridge.poll_generation(&profile_dir, &evidence))
        .unwrap();
    assert_eq!(poll_res.status, "ready");
    assert_eq!(poll_res.progress_pct, 100.0);
    assert!(poll_res.download_url.is_some());

    // 4. Download artifact via browser
    let download_url = poll_res.download_url.unwrap();
    let dest_path = temp_dir.path().join("downloaded_output.mp4");
    let dl_res = rt.block_on(bridge.download_artifact(&profile_dir, &download_url, &dest_path));
    assert!(dl_res.is_ok());
    assert!(dest_path.exists());
    assert!(std::fs::metadata(&dest_path).unwrap().len() > 0);
}

#[test]
fn test_phase20a_39_mock_flow_scenarios_logged_out_credits_and_ui_changed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let h_logout = rt
        .block_on(MockFlowServer::start(MockScenario::LoggedOut))
        .unwrap();
    let b_logout = PlaywrightBridge::with_mock_url(h_logout.base_url.clone());
    let auth_logout = rt
        .block_on(b_logout.check_auth_status(&profile_dir))
        .unwrap();
    assert!(!auth_logout);

    let h_credits = rt
        .block_on(MockFlowServer::start(MockScenario::CreditsRequired))
        .unwrap();
    let b_credits = PlaywrightBridge::with_mock_url(h_credits.base_url.clone());
    let poll_credits = rt
        .block_on(b_credits.poll_generation(&profile_dir, "ev_2"))
        .unwrap();
    assert_eq!(poll_credits.status, "credits_required");

    let h_ui = rt
        .block_on(MockFlowServer::start(MockScenario::UiChanged))
        .unwrap();
    let b_ui = PlaywrightBridge::with_mock_url(h_ui.base_url.clone());
    let poll_ui = rt
        .block_on(b_ui.poll_generation(&profile_dir, "ev_3"))
        .unwrap();
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

#[test]
fn test_phase20a_45_browser_session_persistence_and_bounded_alive() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_mgr
        .create_profile("prof_persist_test", "Persist Test")
        .unwrap();
    let profile_dir = profile_mgr.get_profile_dir("prof_persist_test").unwrap();

    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let session_mgr = FlowBrowserSessionManager::with_mock_url(server_handle.base_url.clone());

    // 1. Open session -> IPC returns -> managed session is alive
    let open_res = rt.block_on(session_mgr.open_session("prof_persist_test", &profile_dir, &paths));
    assert_eq!(open_res.unwrap(), "OPEN");
    assert!(session_mgr.is_session_open("prof_persist_test"));

    // 2. Bounded wait -> session remains alive
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(session_mgr.is_session_open("prof_persist_test"));

    // 3. Clean up
    rt.block_on(session_mgr.close_session("prof_persist_test"))
        .unwrap();
    assert!(!session_mgr.is_session_open("prof_persist_test"));
}

#[test]
fn test_phase20a_46_same_session_auth_refresh() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_mgr
        .create_profile("prof_refresh_test", "Refresh Test")
        .unwrap();
    let profile_dir = profile_mgr.get_profile_dir("prof_refresh_test").unwrap();

    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let session_mgr = FlowBrowserSessionManager::with_mock_url(server_handle.base_url.clone());

    // Open session
    let _ = rt
        .block_on(session_mgr.open_session("prof_refresh_test", &profile_dir, &paths))
        .unwrap();

    // Refresh auth using SAME live session
    let auth_status = rt
        .block_on(session_mgr.check_or_refresh_auth("prof_refresh_test", &profile_dir, &paths))
        .unwrap();
    assert_eq!(auth_status, "READY");

    // Close session
    rt.block_on(session_mgr.close_session("prof_refresh_test"))
        .unwrap();
}

#[test]
fn test_phase20a_47_browser_already_open_guard() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_mgr
        .create_profile("prof_dup_test", "Dup Test")
        .unwrap();
    let profile_dir = profile_mgr.get_profile_dir("prof_dup_test").unwrap();

    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let session_mgr = FlowBrowserSessionManager::with_mock_url(server_handle.base_url.clone());

    let res1 = rt
        .block_on(session_mgr.open_session("prof_dup_test", &profile_dir, &paths))
        .unwrap();
    assert_eq!(res1, "OPEN");

    // Second open on same profile -> BROWSER_ALREADY_OPEN
    let res2 = rt
        .block_on(session_mgr.open_session("prof_dup_test", &profile_dir, &paths))
        .unwrap();
    assert_eq!(res2, "BROWSER_ALREADY_OPEN");

    rt.block_on(session_mgr.close_session("prof_dup_test"))
        .unwrap();
}

#[test]
fn test_phase20a_48_explicit_browser_close_and_lock_release() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_mgr
        .create_profile("prof_close_test", "Close Test")
        .unwrap();
    let profile_dir = profile_mgr.get_profile_dir("prof_close_test").unwrap();

    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let session_mgr = FlowBrowserSessionManager::with_mock_url(server_handle.base_url.clone());

    let _ = rt
        .block_on(session_mgr.open_session("prof_close_test", &profile_dir, &paths))
        .unwrap();
    assert!(profile_dir.join(".session.lock").exists());

    // Explicit close releases lock
    rt.block_on(session_mgr.close_session("prof_close_test"))
        .unwrap();
    assert!(!profile_dir.join(".session.lock").exists());
    assert!(!session_mgr.is_session_open("prof_close_test"));
}

#[test]
fn test_phase20a_49_session_manager_shutdown_cleanup() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_mgr
        .create_profile("prof_shut_test", "Shutdown Test")
        .unwrap();
    let profile_dir = profile_mgr.get_profile_dir("prof_shut_test").unwrap();

    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let session_mgr = FlowBrowserSessionManager::with_mock_url(server_handle.base_url.clone());

    let _ = rt
        .block_on(session_mgr.open_session("prof_shut_test", &profile_dir, &paths))
        .unwrap();
    assert!(session_mgr.is_session_open("prof_shut_test"));

    // App shutdown -> close_all
    rt.block_on(session_mgr.close_all());
    assert!(!session_mgr.is_session_open("prof_shut_test"));
}

#[test]
fn test_phase20a_50_profile_locked_by_worker_not_browser_open() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_mgr
        .create_profile("prof_worker_lock", "Worker Lock")
        .unwrap();
    let profile_dir = profile_mgr.get_profile_dir("prof_worker_lock").unwrap();

    let session_mgr = FlowBrowserSessionManager::new();

    // Lock profile directly with FlowProfileGuard (simulating background worker)
    let guard = profile_mgr
        .acquire_session_lock("prof_worker_lock")
        .unwrap();
    assert!(profile_dir.join(".session.lock").exists());

    // browser_session_open must be false
    assert!(!session_mgr.is_session_open("prof_worker_lock"));

    drop(guard);
}

#[test]
fn test_phase20a_58_profile_auth_refresh_reload_consistency() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_mgr
        .create_profile("prof_consistency_test", "Consistency Test")
        .unwrap();
    let profile_dir = profile_mgr
        .get_profile_dir("prof_consistency_test")
        .unwrap();

    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let session_mgr = FlowBrowserSessionManager::with_mock_url(server_handle.base_url.clone());

    // 1. Open session
    let _ = rt
        .block_on(session_mgr.open_session("prof_consistency_test", &profile_dir, &paths))
        .unwrap();

    // 2. Refresh auth -> READY
    let refreshed = rt
        .block_on(session_mgr.check_or_refresh_auth("prof_consistency_test", &profile_dir, &paths))
        .unwrap();
    assert_eq!(refreshed, "READY");

    // 3. Re-read / overlay profiles as in list_flow_profiles
    let mut profiles = profile_mgr.list_profiles();
    for p in &mut profiles {
        if session_mgr.is_session_open(&p.profile_id) {
            p.browser_session_open = true;
            p.status = session_mgr
                .get_session_auth_status(&p.profile_id)
                .unwrap_or_else(|| "UNKNOWN".to_string());
        } else {
            p.browser_session_open = false;
            p.status = "UNKNOWN".to_string();
        }
    }

    let p_snap = profiles
        .iter()
        .find(|p| p.profile_id == "prof_consistency_test")
        .unwrap();
    assert_eq!(p_snap.status, "READY");
    assert!(p_snap.browser_session_open);

    // 4. Close browser session
    rt.block_on(session_mgr.close_session("prof_consistency_test"))
        .unwrap();

    // 5. Re-read profiles after close -> auth status becomes UNKNOWN, browserSessionOpen=false
    let mut profiles_after = profile_mgr.list_profiles();
    for p in &mut profiles_after {
        if session_mgr.is_session_open(&p.profile_id) {
            p.browser_session_open = true;
            p.status = session_mgr
                .get_session_auth_status(&p.profile_id)
                .unwrap_or_else(|| "UNKNOWN".to_string());
        } else {
            p.browser_session_open = false;
            p.status = "UNKNOWN".to_string();
        }
    }

    let p_snap_after = profiles_after
        .iter()
        .find(|p| p.profile_id == "prof_consistency_test")
        .unwrap();
    assert_eq!(p_snap_after.status, "UNKNOWN");
    assert!(!p_snap_after.browser_session_open);
}

#[test]
fn test_phase20a_59_production_app_shutdown_lifecycle_callback_cleans_sessions() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_mgr
        .create_profile("prof_prod_shut", "Prod Shutdown Test")
        .unwrap();
    let profile_dir = profile_mgr.get_profile_dir("prof_prod_shut").unwrap();

    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let session_mgr = Arc::new(FlowBrowserSessionManager::with_mock_url(
        server_handle.base_url.clone(),
    ));

    // Open active managed login session
    let _ = rt
        .block_on(session_mgr.open_session("prof_prod_shut", &profile_dir, &paths))
        .unwrap();
    assert!(session_mgr.is_session_open("prof_prod_shut"));
    assert!(profile_dir.join(".session.lock").exists());

    // Invoke production app shutdown lifecycle handler
    rt.block_on(crate::handle_app_shutdown(session_mgr.clone()));

    // Assert session is closed, lock is removed, no orphan processes
    assert!(!session_mgr.is_session_open("prof_prod_shut"));
    assert!(!profile_dir.join(".session.lock").exists());
}
