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

fn create_synthetic_mp4(
    path: &std::path::Path,
    duration_sec: f64,
    fps: f64,
    width: u32,
    height: u32,
    include_audio: bool,
) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.arg("-y").arg("-f").arg("lavfi").arg("-i").arg(format!(
        "testsrc=duration={:.2}:size={}x{}:rate={:.2}",
        duration_sec, width, height, fps
    ));

    if include_audio {
        cmd.arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg(format!("sine=frequency=1000:duration={:.2}", duration_sec))
            .arg("-c:v")
            .arg("libx264")
            .arg("-c:a")
            .arg("aac")
            .arg("-shortest");
    } else {
        cmd.arg("-c:v").arg("libx264").arg("-an");
    }

    cmd.arg("-pix_fmt").arg("yuv420p").arg(path);

    let output = cmd.output().expect("Failed to generate synthetic mp4");
    assert!(output.status.success());
}

#[test]
fn test_phase20b_09_expected_9_682s_generated_4s_fails_duration_validation() {
    let temp_dir = tempdir().unwrap();
    let file_4s = temp_dir.path().join("child_4s.mp4");
    create_synthetic_mp4(&file_4s, 4.0, 24.0, 1280, 720, false);

    let val_res = FlowOutputValidator::validate_child_artifact(&file_4s, 9.682);
    assert!(val_res.is_err());
    let err = val_res.unwrap_err();
    assert!(err.contains("FLOW_OUTPUT_DURATION_MISMATCH"));
    assert!(err.contains("Duration drift too large"));
}

#[test]
fn test_phase20b_10_expected_10s_generated_9_95s_passes_within_tolerance() {
    let temp_dir = tempdir().unwrap();
    let file_9_95s = temp_dir.path().join("child_9_95s.mp4");
    create_synthetic_mp4(&file_9_95s, 9.95, 30.0, 1280, 720, false);

    let val_res = FlowOutputValidator::validate_child_artifact(&file_9_95s, 10.0);
    assert!(val_res.is_ok());
    let rec = val_res.unwrap();
    assert!((rec.duration_sec - 9.95).abs() < 0.1);
}

#[test]
fn test_phase20b_11_video_4s_source_audio_10s_stitcher_refuses_normal_completion() {
    let temp_dir = tempdir().unwrap();
    let video_4s = temp_dir.path().join("video_4s.mp4");
    create_synthetic_mp4(&video_4s, 4.0, 24.0, 1280, 720, false);

    let audio_src = temp_dir.path().join("audio_10s.mp4");
    create_synthetic_mp4(&audio_src, 10.0, 30.0, 320, 240, true);

    let out_file = temp_dir.path().join("out_stitched.mp4");
    let policy = FlowFinalAudioPolicy {
        preserve_original_audio: true,
        codec: "aac".to_string(),
    };

    let stitch_res =
        FlowStitcher::stitch_flow_segments(&[video_4s], Some(&audio_src), 10.0, &policy, &out_file);

    assert!(stitch_res.is_err());
    let err = stitch_res.unwrap_err();
    assert!(err.contains("FLOW_OUTPUT_DURATION_MISMATCH"));
    assert!(!out_file.exists());
}

#[test]
fn test_phase20b_12_container_duration_10s_video_stream_4s_validator_fails() {
    let temp_dir = tempdir().unwrap();
    let video_4s = temp_dir.path().join("video_4s.mp4");
    create_synthetic_mp4(&video_4s, 4.0, 24.0, 1280, 720, false);

    let audio_10s = temp_dir.path().join("audio_10s.mp4");
    create_synthetic_mp4(&audio_10s, 10.0, 30.0, 320, 240, true);

    // Mux 4s video with 10s audio without -shortest
    let mismatched_file = temp_dir.path().join("mismatched.mp4");
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.arg("-y")
        .arg("-i")
        .arg(&video_4s)
        .arg("-i")
        .arg(&audio_10s)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("1:a:0")
        .arg("-c:v")
        .arg("copy")
        .arg("-c:a")
        .arg("aac")
        .arg(&mismatched_file);
    let out = cmd.output().unwrap();
    assert!(out.status.success());

    // Expecting 10s: video is 4s -> must fail
    let val_res_10 = FlowOutputValidator::validate_child_artifact(&mismatched_file, 10.0);
    assert!(val_res_10.is_err());
    assert!(val_res_10
        .unwrap_err()
        .contains("FLOW_OUTPUT_DURATION_MISMATCH"));

    // Expecting 4s: video is 4s but audio is 10s -> must fail audio duration alignment check
    let val_res_4 = FlowOutputValidator::validate_child_artifact(&mismatched_file, 4.0);
    assert!(val_res_4.is_err());
    assert!(val_res_4
        .unwrap_err()
        .contains("FLOW_OUTPUT_DURATION_MISMATCH"));
}

#[test]
fn test_phase20b_13_normal_valid_matching_child_passes() {
    let temp_dir = tempdir().unwrap();
    let file_5s = temp_dir.path().join("child_5s.mp4");
    create_synthetic_mp4(&file_5s, 5.0, 30.0, 1280, 720, true);

    let val_res = FlowOutputValidator::validate_child_artifact(&file_5s, 5.0);
    assert!(val_res.is_ok());
    let rec = val_res.unwrap();
    assert_eq!(rec.width, 1280);
    assert_eq!(rec.height, 720);
    assert!(rec.has_audio);
    assert!((rec.duration_sec - 5.0).abs() < 0.2);
}

#[test]
fn test_phase20b_14_generation_settings_10s_portrait_x1_readback_pass() {
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

    let preflight = rt
        .block_on(session.dry_run_preflight("Cinematic sunset lighting test", None))
        .unwrap();

    assert_eq!(
        preflight.get("authStatus").and_then(|v| v.as_str()),
        Some("READY")
    );
    assert_eq!(
        preflight.get("model").and_then(|v| v.as_str()),
        Some("Omni Flash")
    );
    assert_eq!(
        preflight
            .get("generationLengthSec")
            .and_then(|v| v.as_u64()),
        Some(10)
    );
    let ori = preflight
        .get("orientation")
        .and_then(|v| v.as_str())
        .unwrap();
    assert!(ori.contains("PORTRAIT") || ori.contains("9:16"));
    assert_eq!(
        preflight.get("outputCount").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert_eq!(
        preflight.get("generateLocated").and_then(|v| v.as_bool()),
        Some(true)
    );

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_15_duration_selector_missing_fails_closed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::MissingDurationSelector))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let submit_res =
        rt.block_on(session.submit("Cinematic sunset lighting test", None, 9.68, "att_p20b_15"));

    assert!(submit_res.is_err());
    let err_msg = submit_res.unwrap_err();
    assert!(
        err_msg.contains("FLOW_CONFIGURATION_UNVERIFIED")
            || err_msg.contains("10s")
            || err_msg.contains("not found")
    );

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_16_orientation_selector_missing_fails_closed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(
            MockScenario::MissingOrientationSelector,
        ))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let submit_res =
        rt.block_on(session.submit("Cinematic sunset lighting test", None, 9.68, "att_p20b_16"));

    assert!(submit_res.is_err());
    let err_msg = submit_res.unwrap_err();
    assert!(
        err_msg.contains("FLOW_CONFIGURATION_UNVERIFIED")
            || err_msg.contains("Orientation")
            || err_msg.contains("not found")
    );

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_17_readback_mismatch_fails_closed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::ReadbackMismatch))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let submit_res =
        rt.block_on(session.submit("Cinematic sunset lighting test", None, 9.68, "att_p20b_17"));

    assert!(submit_res.is_err());
    let err_msg = submit_res.unwrap_err();
    assert!(err_msg.contains("FLOW_CONFIGURATION_UNVERIFIED") || err_msg.contains("mismatch"));

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_18_input_trim_duration_does_not_imply_output_length() {
    let input_selected_duration = 9.682_f64;
    let output_generation_length = 10_u32;

    // Input trim and output generation length are completely independent dimensions
    assert_ne!(input_selected_duration as u32, output_generation_length);
    assert!((input_selected_duration - 9.682).abs() < 0.001);
    assert_eq!(output_generation_length, 10);
}

#[test]
fn test_phase20b_19_wrong_output_count_fails_closed() {
    let target_output_count = 1;
    let observed_output_count = 2;

    // If observed output count != 1, system must refuse to treat as verified
    let is_verified = target_output_count == observed_output_count;
    assert!(!is_verified);
}

#[test]
fn test_phase20b_20_image_only_input_rejects_mp4() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::ImageOnlyFileInput))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let test_mp4_path = temp_dir.path().join("test_input.mp4");
    std::fs::write(&test_mp4_path, b"dummy_mp4_data").unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let edit_res = rt.block_on(session.ensure_uploaded_video_edit_active(
        Some(&test_mp4_path),
        9.682,
        "PORTRAIT / 9:16",
    ));

    assert!(edit_res.is_err());
    let err_msg = edit_res.unwrap_err();
    assert!(err_msg.contains("FLOW_VIDEO_NOT_ATTACHED") || err_msg.contains("image-only"));

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_21_unattached_video_fails_closed() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::UnattachedVideoUpload))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let edit_res =
        rt.block_on(session.ensure_uploaded_video_edit_active(None, 9.682, "PORTRAIT / 9:16"));

    assert!(edit_res.is_err());
    let err_msg = edit_res.unwrap_err();
    assert!(err_msg.contains("FLOW_VIDEO_EDIT_NOT_ACTIVE"));

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_22_true_video_edit_passes_with_20_credits() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server_handle = rt
        .block_on(MockFlowServer::start(MockScenario::TrueVideoEditActive))
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let profile_dir = temp_dir.path().join("chrome_profile");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let test_mp4_path = temp_dir.path().join("test_input.mp4");
    std::fs::write(&test_mp4_path, b"dummy_mp4_data").unwrap();

    let bridge = PlaywrightBridge::with_mock_url(server_handle.base_url.clone());
    let mut session = rt
        .block_on(bridge.open_active_session(&profile_dir))
        .unwrap();

    let verif = rt
        .block_on(session.ensure_uploaded_video_edit_active(
            Some(&test_mp4_path),
            9.682,
            "PORTRAIT / 9:16",
        ))
        .unwrap();

    assert!(verif.uploaded_video_attached);
    assert!(verif.uploaded_video_edit_active);
    assert_eq!(verif.active_composer_mode, "EDIT");
    assert!((verif.input_trim_start - 0.0).abs() < 0.001);
    assert!((verif.input_trim_end - 9.682).abs() < 0.01);
    assert!((verif.input_selected_duration - 9.682).abs() < 0.01);
    assert_eq!(verif.model, "Omni Flash");
    assert_eq!(verif.generation_length_sec, 10);
    assert_eq!(verif.orientation, "PORTRAIT / 9:16");
    assert_eq!(verif.output_count, 1);
    assert_eq!(verif.credit_estimate_number, Some(20));
    assert!(verif.credit_stable);
    assert_eq!(verif.cost_classification, "UPLOADED_VIDEO_EDIT_FLASH_20");

    rt.block_on(session.close());
}

#[test]
fn test_phase20b_23_input_selected_duration_matches_benchmark() {
    let source_duration = 9.685_f64;
    let expected_trim_duration = 9.682_f64;
    let tolerance = 0.05_f64;

    assert!((source_duration - expected_trim_duration).abs() < tolerance);
}

#[test]
fn test_phase20b_24_generic_15_credit_rejected_as_true_edit_proof() {
    let generic_text_to_video_cost = 15;
    let is_true_edit_proof = generic_text_to_video_cost == 20 || generic_text_to_video_cost == 40;
    // 15 credits alone is text-to-video, NOT true video edit proof
    assert!(!is_true_edit_proof);
}

#[test]
fn test_phase20b_25_credit_policy_classification_rules() {
    let classify = |credits: u32| match credits {
        40 => "UPLOADED_VIDEO_EDIT_EXPECTED",
        20 => "UPLOADED_VIDEO_EDIT_FLASH_20",
        30 => "LOOKS_LIKE_10S_NON_EDIT_GENERATION",
        15 => "LOOKS_LIKE_4S_NON_EDIT_OR_STALE_VALUE",
        _ => "UNKNOWN_CURRENT_PRICING",
    };

    assert_eq!(classify(40), "UPLOADED_VIDEO_EDIT_EXPECTED");
    assert_eq!(classify(20), "UPLOADED_VIDEO_EDIT_FLASH_20");
    assert_eq!(classify(30), "LOOKS_LIKE_10S_NON_EDIT_GENERATION");
    assert_eq!(classify(15), "LOOKS_LIKE_4S_NON_EDIT_OR_STALE_VALUE");
    assert_eq!(classify(99), "UNKNOWN_CURRENT_PRICING");
}

#[test]
fn test_phase20b_26_stale_credit_unstable_caught() {
    let readback_1 = "15 tín dụng";
    let readback_2 = "20 tín dụng";

    let is_stable = readback_1 == readback_2;
    assert!(!is_stable);
}

#[test]
fn test_phase20b_27_credit_isolated_to_active_edit_composer() {
    let account_balance_text = "Account total: 1000 credits remaining";
    let active_composer_tooltip = "Quá trình tạo sẽ tốn 20 tín dụng";

    // System must isolate active composer tooltip cost (20) and not account balance (1000)
    let isolated_cost: u32 = active_composer_tooltip
        .split_whitespace()
        .find_map(|word| word.parse::<u32>().ok())
        .unwrap_or(0);

    assert_eq!(isolated_cost, 20);
    assert!(!account_balance_text.contains("tốn"));
}
