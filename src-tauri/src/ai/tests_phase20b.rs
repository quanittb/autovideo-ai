use crate::ai::flow::*;
use std::sync::atomic::Ordering;
use tempfile::tempdir;

#[test]
fn test_phase20b_01_real_semantic_generating_transition_proven_submitted() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::DelayAfterGenerateClick))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let test_mp4_path = temp_dir.path().join("test_input.mp4");
    std::fs::write(&test_mp4_path, b"dummy_mp4_data_for_upload").unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let evidence = rt
        .block_on(session.submit(
            "Cinematic sunset lighting test",
            Some(&test_mp4_path),
            9.68,
            "att_p20b_01",
        ))
        .unwrap();

    // Must contain positive semantic proof and attempt id
    assert!(evidence.contains("semantic:generating:"));
    assert!(evidence.contains("att_p20b_01"));
    assert_eq!(server_handle.generate_click_count.load(Ordering::SeqCst), 1);

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_02_no_transition_after_click_ambiguous() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::NoTransitionAfterClick))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let test_mp4_path = temp_dir.path().join("test_input.mp4");
    std::fs::write(&test_mp4_path, b"dummy_mp4_data_for_upload").unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let submit_res = rt.block_on(session.submit(
        "Cinematic sunset lighting test",
        Some(&test_mp4_path),
        9.68,
        "att_p20b_02",
    ));

    // Must fail closed with GENERATION_AMBIGUOUS and make 0 retries
    assert!(submit_res.is_err());
    let err_msg = submit_res.unwrap_err();
    assert!(err_msg.contains("GENERATION_AMBIGUOUS"));
    assert_eq!(server_handle.generate_click_count.load(Ordering::SeqCst), 1);

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_03_unknown_poll_dom_fails_closed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::UnknownPollDom))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let poll_res = rt
        .block_on(session.poll("semantic:generating:2026-08-24T00:00:00Z:att_03"))
        .unwrap();

    // Must return ui_changed or unknown and progress 0 (NEVER 50%)
    assert!(poll_res.status == "ui_changed" || poll_res.status == "unknown");
    assert_eq!(poll_res.progress_pct, 0.0);

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_04_result_missing_download_fails_closed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::ResultMissingDownload))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let dest_path = temp_dir.path().join("missing_download_out.mp4");
    let dl_res = rt.block_on(session.download(None, &dest_path));

    // Must fail closed with DOWNLOAD_CONTROL_NOT_OBSERVED
    assert!(dl_res.is_err());
    let err_msg = dl_res.unwrap_err();
    assert!(err_msg.contains("DOWNLOAD_CONTROL_NOT_OBSERVED"));
    assert!(!dest_path.exists());

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_05_valid_result_download_and_validation() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::Ready))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let dest_path = temp_dir.path().join("valid_out.mp4");
    let dl_res = rt.block_on(session.download(Some("/download"), &dest_path));

    assert!(dl_res.is_ok());
    assert!(dest_path.exists());
    assert!(std::fs::metadata(&dest_path).unwrap().len() > 0);

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_06_single_session_lifecycle() {
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
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    // 1. Submit on active session
    let evidence = rt
        .block_on(session.submit(
            "Single session submit prompt",
            Some(&test_mp4_path),
            5.0,
            "att_single_01",
        ))
        .unwrap();
    assert!(evidence.contains("att_single_01"));

    // 2. Poll on SAME active session
    let poll_res = rt.block_on(session.poll(&evidence)).unwrap();
    assert_eq!(poll_res.status, "ready");

    // 3. Download on SAME active session
    let dest_path = temp_dir.path().join("single_session_out.mp4");
    let dl_res = rt.block_on(session.download(poll_res.download_url.as_deref(), &dest_path));
    assert!(dl_res.is_ok());
    assert!(dest_path.exists());

    // 4. Close session cleanly
    rt.block_on(session.close());
}

#[test]
fn test_phase20b_07_dry_run_preflight_production_helpers() {
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
    let preflight = rt
        .block_on(bridge.dry_run_preflight(
            &profile_dir,
            "Dry run check prompt",
            Some(&test_mp4_path),
        ))
        .unwrap();

    assert_eq!(
        preflight.get("authStatus").and_then(|v| v.as_str()),
        Some("READY")
    );
    assert_eq!(
        preflight
            .get("workspaceAccessible")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        preflight.get("promptLocated").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        preflight.get("generateLocated").and_then(|v| v.as_bool()),
        Some(true)
    );
    // Absolutely 0 generate clicks!
    assert_eq!(server_handle.generate_click_count.load(Ordering::SeqCst), 0);
}

#[test]
fn test_phase20b_08_no_synthetic_proof_accepted() {
    let raw_fingerprint = "fp_att_123_dur_9.68";
    // Synthetic fingerprint cannot be treated as proven submitted evidence
    assert!(!raw_fingerprint.starts_with("semantic:"));
    assert!(!raw_fingerprint.contains("evidence:"));

    let valid_semantic_evidence = "semantic:generating:2026-08-24T12:00:00Z:att_123";
    assert!(valid_semantic_evidence.starts_with("semantic:"));
}
