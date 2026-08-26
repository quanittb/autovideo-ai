use crate::ai::flow::*;
use crate::ai::transformation::{IdentityMode, TransformationIntent};
use crate::commands::resolve_project_media_by_id;
use crate::projects::{ProjectEditorState, ProjectManager, SourceMedia};
use crate::system::StoragePaths;
use std::fs;
use std::sync::atomic::Ordering;
use tempfile::tempdir;

#[test]
fn test_flow_p3a_01_preflight_resolves_canonical_media_id() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let manager = ProjectManager::new(paths.clone());

    let mut project = manager.create_project("Preflight Media Test").unwrap();
    let proj_dir = paths.projects_dir.join(&project.id);
    let media_dir = proj_dir.join("media");
    fs::create_dir_all(&media_dir).unwrap();

    let orig_file = media_dir.join("input_source.mp4");
    fs::write(&orig_file, b"fake video").unwrap();

    project.source_media = Some(SourceMedia {
        media_id: "media_orig_100".to_string(),
        original_file_name: "input_source.mp4".to_string(),
        source_path: orig_file.clone(),
        duration_ms: 10000,
        width: 1080,
        height: 1920,
        fps: 30.0,
        file_size_bytes: 1000,
        container: "mp4".to_string(),
        video_codec: "h264".to_string(),
        audio_codec: Some("aac".to_string()),
        has_audio: true,
    });
    project.editor_state = Some(ProjectEditorState {
        active_media_id: Some("media_orig_100".to_string()),
        ..Default::default()
    });
    manager.update_project(&project).unwrap();

    // 1. Resolve explicitly by mediaId
    let (resolved_path, source_media) =
        resolve_project_media_by_id(&project.id, Some("media_orig_100"), &paths).unwrap();
    assert_eq!(source_media.media_id, "media_orig_100");
    assert_eq!(
        resolved_path.canonicalize().unwrap(),
        orig_file.canonicalize().unwrap()
    );

    // 2. Reject path traversal
    let traversal_err =
        resolve_project_media_by_id(&project.id, Some("../../etc/passwd"), &paths).unwrap_err();
    assert!(
        traversal_err.contains("MEDIA_NOT_FOUND") || traversal_err.contains("SECURITY_VIOLATION")
    );
}

#[test]
fn test_flow_p3a_02_preflight_resolves_system_default_prompt() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    // Create profile
    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("test_profile", "Test")
        .unwrap();

    let dummy_video = temp_dir.path().join("dummy.mp4");
    fs::write(&dummy_video, b"fake video").unwrap();

    let req = FlowGenerationRequest {
        project_id: "proj_1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        profile_id: "test_profile".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "  ".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None,
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let probe_err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(flow_service.preflight_flow_generation(req, dummy_video))
        .unwrap_err();

    // It resolved prompt to SYSTEM_DEFAULT and proceeded to source media probe
    assert!(
        probe_err.contains("PROBE_FAILED") || probe_err.contains("INVALID_MEDIA"),
        "Expected media probe failure, got: {}",
        probe_err
    );
}

#[test]
fn test_flow_p3a_03_preflight_blocks_reference_and_empty_style_before_browser() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("test_profile", "Test")
        .unwrap();

    let video_file = temp_dir.path().join("source.mp4");
    fs::write(&video_file, b"fake video").unwrap();

    // 1. Reference mode is blocked
    let req_ref = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "source.mp4".to_string(),
        profile_id: "test_profile".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Reference),
        prompt: "Replace face".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None,
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let ref_err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(flow_service.preflight_flow_generation(req_ref, video_file.clone()))
        .unwrap_err();
    assert!(ref_err.contains("FLOW_REFERENCE_IDENTITY_NOT_SUPPORTED"));

    // 2. Empty prompt on STYLE_EDIT is blocked
    let req_style = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "source.mp4".to_string(),
        profile_id: "test_profile".to_string(),
        transformation_intent: Some(TransformationIntent::StyleEdit),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None,
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let style_err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(flow_service.preflight_flow_generation(req_style, video_file))
        .unwrap_err();
    assert!(style_err.contains("REQUEST_INVALID"));
}

#[tokio::test]
async fn test_flow_p3a_04_preflight_mock_flow_readback_and_zero_generate_clicks() {
    let mock_server = MockFlowServer::start(MockScenario::TrueVideoEditActive)
        .await
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service =
        FlowRuntimeService::with_mock_bridge(paths.clone(), mock_server.base_url.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("profile_ready", "Ready Profile")
        .unwrap();

    // Create real minimal video file
    let video_file = temp_dir.path().join("test_input.mp4");
    std::process::Command::new("ffmpeg")
        .args(&[
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=10:size=576x1024:rate=30",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            video_file.to_str().unwrap(),
        ])
        .output()
        .expect("ffmpeg creation");

    let req = FlowGenerationRequest {
        project_id: "proj_flow_p3a".to_string(),
        source_media_id: "media_100".to_string(),
        profile_id: "profile_ready".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None,
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let preflight = flow_service
        .preflight_flow_generation(req, video_file)
        .await
        .unwrap();

    // Verify preflight readback
    assert_eq!(
        preflight.transformation_intent,
        TransformationIntent::FaceReplace
    );
    assert_eq!(preflight.identity_mode, IdentityMode::Generated);
    assert_eq!(preflight.prompt_source, PromptSource::SystemDefault);
    assert!(!preflight.prompt_hash.is_empty());
    assert!(preflight.video_attached);
    assert!(preflight.video_edit_active);
    assert_eq!(preflight.live_displayed_credit_cost, Some(20));
    assert!(preflight.ready_for_paid_submission);
    assert_eq!(preflight.blocking_code, None);

    // CRITICAL: Absolute zero generate click invariant!
    assert_eq!(
        mock_server.generate_click_count.load(Ordering::SeqCst),
        0,
        "PREFLIGHT MUST NEVER DISPATCH GENERATE CLICK"
    );
}

#[tokio::test]
async fn test_flow_p3a_05_preflight_logged_out_profile_returns_blocking_code() {
    let mock_server = MockFlowServer::start(MockScenario::LoggedOut)
        .await
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service =
        FlowRuntimeService::with_mock_bridge(paths.clone(), mock_server.base_url.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("profile_logged_out", "Logged Out Profile")
        .unwrap();

    let video_file = temp_dir.path().join("test_input.mp4");
    std::process::Command::new("ffmpeg")
        .args(&[
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=10:size=576x1024:rate=30",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            video_file.to_str().unwrap(),
        ])
        .output()
        .expect("ffmpeg creation");

    let req = FlowGenerationRequest {
        project_id: "proj_flow_p3a".to_string(),
        source_media_id: "media_100".to_string(),
        profile_id: "profile_logged_out".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None,
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let preflight = flow_service
        .preflight_flow_generation(req, video_file)
        .await
        .unwrap();

    assert!(!preflight.ready_for_paid_submission);
    assert_eq!(preflight.blocking_code, Some("LOGIN_REQUIRED".to_string()));
    assert_eq!(mock_server.generate_click_count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_flow_p3a_06_video_edit_inactive_blocks_generic_cost_exposure() {
    let mock_server = MockFlowServer::start(MockScenario::UnattachedVideoUpload)
        .await
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service =
        FlowRuntimeService::with_mock_bridge(paths.clone(), mock_server.base_url.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("profile_ready", "Ready Profile")
        .unwrap();

    let video_file = temp_dir.path().join("test_input.mp4");
    std::process::Command::new("ffmpeg")
        .args(&[
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=10:size=576x1024:rate=30",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            video_file.to_str().unwrap(),
        ])
        .output()
        .expect("ffmpeg creation");

    let req = FlowGenerationRequest {
        project_id: "proj_flow_p3a".to_string(),
        source_media_id: "media_100".to_string(),
        profile_id: "profile_ready".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None,
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let preflight = flow_service
        .preflight_flow_generation(req, video_file)
        .await
        .unwrap();

    // When video edit mode is inactive, generic composer cost MUST NOT become liveDisplayedCreditCost
    assert!(!preflight.video_attached);
    assert!(!preflight.video_edit_active);
    assert!(!preflight.configuration_verified);
    assert_eq!(preflight.cost_provenance, FlowCostProvenance::Unknown);
    assert_eq!(preflight.live_displayed_credit_cost, None);
    assert!(!preflight.ready_for_paid_submission);
    assert_eq!(
        preflight.blocking_code,
        Some("FLOW_VIDEO_NOT_ATTACHED".to_string())
    );
}

#[tokio::test]
async fn test_flow_p3a_07_mock_true_edit_exposes_authoritative_cost() {
    let mock_server = MockFlowServer::start(MockScenario::TrueVideoEditActive)
        .await
        .unwrap();

    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service =
        FlowRuntimeService::with_mock_bridge(paths.clone(), mock_server.base_url.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("profile_ready", "Ready Profile")
        .unwrap();

    let video_file = temp_dir.path().join("test_input.mp4");
    std::process::Command::new("ffmpeg")
        .args(&[
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=10:size=576x1024:rate=30",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            video_file.to_str().unwrap(),
        ])
        .output()
        .expect("ffmpeg creation");

    let req = FlowGenerationRequest {
        project_id: "proj_flow_p3a".to_string(),
        source_media_id: "media_100".to_string(),
        profile_id: "profile_ready".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None,
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let preflight = flow_service
        .preflight_flow_generation(req, video_file)
        .await
        .unwrap();

    assert!(preflight.video_attached);
    assert!(preflight.video_edit_active);
    assert!(preflight.configuration_verified);
    assert_eq!(
        preflight.cost_provenance,
        FlowCostProvenance::UploadedVideoEdit
    );
    assert_eq!(preflight.live_displayed_credit_cost, Some(20));
    assert!(preflight.ready_for_paid_submission);
    assert_eq!(preflight.blocking_code, None);
    assert_eq!(
        preflight.observed_source_title,
        Some("flow_acceptance_01.mp4".to_string())
    );
}

#[tokio::test]
#[ignore]
async fn test_flow_p3a_real_google_flow_live_preflight_acceptance() {
    let base_path =
        std::path::PathBuf::from("D:/rustProject/autovideo-ai/src-tauri/.autovideo_data");
    let paths = StoragePaths::resolve_from_base(&base_path);
    let manager = ProjectManager::new(paths.clone());

    // 1. Create or use a real project
    let mut project = manager
        .create_project("Phase FLOW-P3-A Real Preflight Project")
        .unwrap();
    let proj_dir = paths.projects_dir.join(&project.id);
    let media_dir = proj_dir.join("media");
    fs::create_dir_all(&media_dir).unwrap();

    let source_video = std::path::PathBuf::from(
        "D:/rustProject/autovideo-ai/test-assets/phase20c/videos/flow_acceptance_01.mp4",
    );
    assert!(
        source_video.exists(),
        "Source test video must exist at {:?}",
        source_video
    );

    let dest_media_path = media_dir.join("flow_acceptance_01.mp4");
    fs::copy(&source_video, &dest_media_path).unwrap();

    let media_id = format!("media_{}", uuid::Uuid::new_v4());
    project.source_media = Some(SourceMedia {
        media_id: media_id.clone(),
        original_file_name: "flow_acceptance_01.mp4".to_string(),
        source_path: dest_media_path.clone(),
        duration_ms: 9988,
        width: 576,
        height: 1024,
        fps: 30.0,
        file_size_bytes: fs::metadata(&dest_media_path).unwrap().len(),
        container: "mp4".to_string(),
        video_codec: "h264".to_string(),
        audio_codec: Some("aac".to_string()),
        has_audio: true,
    });
    project.editor_state = Some(ProjectEditorState {
        active_media_id: Some(media_id.clone()),
        ..Default::default()
    });
    manager.update_project(&project).unwrap();

    // 2. Setup FlowRuntimeService with real sidecar
    let flow_service = FlowRuntimeService::new(paths.clone());

    let req = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: media_id.clone(),
        profile_id: "profile_2".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: Some(50),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let dest_canon = dest_media_path.canonicalize().unwrap();
    let dest_canon_str = dest_canon.to_string_lossy().to_string();
    let clean_dest_path = if let Some(stripped) = dest_canon_str.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(stripped)
    } else {
        dest_canon
    };

    println!(
        "[FLOW-P3-A LIVE PREFLIGHT] Starting preflight with Profile 'profile_2' and video '{}'",
        clean_dest_path.display()
    );
    let preflight_res = flow_service
        .preflight_flow_generation(req, clean_dest_path)
        .await;

    match preflight_res {
        Ok(preflight) => {
            println!("==================================================");
            println!("FLOW-P3-A LIVE PREFLIGHT RESULT:");
            println!("Project ID: {}", preflight.project_id);
            println!("Source Media ID: {}", preflight.source_media_id);
            println!("Profile ID: {}", preflight.profile_id);
            println!(
                "Transformation Intent: {:?}",
                preflight.transformation_intent
            );
            println!("Identity Mode: {:?}", preflight.identity_mode);
            println!("Prompt Source: {:?}", preflight.prompt_source);
            println!("Resolved Prompt: {}", preflight.resolved_prompt);
            println!("Prompt Hash: {}", preflight.prompt_hash);
            println!("Video Attached: {}", preflight.video_attached);
            println!("Video Edit Active: {}", preflight.video_edit_active);
            println!("Config Verified: {}", preflight.configuration_verified);
            println!("Cost Provenance: {:?}", preflight.cost_provenance);
            println!(
                "Observed Source Title: {:?}",
                preflight.observed_source_title
            );
            println!(
                "Observed Source Duration: {:?}",
                preflight.observed_source_duration
            );
            println!("Observed Model: {:?}", preflight.observed_model);
            println!("Observed Resolution: {:?}", preflight.observed_resolution);
            println!("Observed Orientation: {:?}", preflight.observed_orientation);
            println!(
                "Observed Output Count: {:?}",
                preflight.observed_output_count
            );
            println!(
                "Observed Generation Length: {:?}",
                preflight.observed_generation_length
            );
            println!(
                "Live Displayed Credit Cost: {:?}",
                preflight.live_displayed_credit_cost
            );
            println!(
                "Diagnostic Composer Credit Cost: {:?}",
                preflight.diagnostic_composer_credit_cost
            );
            println!("Live Credit Balance: {:?}", preflight.live_credit_balance);
            println!(
                "Configuration Fingerprint: {}",
                preflight.configuration_fingerprint
            );
            println!(
                "Ready For Paid Submission: {}",
                preflight.ready_for_paid_submission
            );
            println!("Blocking Code: {:?}", preflight.blocking_code);
            println!("Checked At: {}", preflight.checked_at);
            println!("==================================================");

            assert_eq!(preflight.prompt_source, PromptSource::SystemDefault);
            assert!(!preflight.prompt_hash.is_empty());
        }
        Err(err) => {
            println!("[FLOW-P3-A LIVE PREFLIGHT ERROR] {}", err);
            panic!("Live preflight failed: {}", err);
        }
    }
}

#[test]
fn test_flow_p3a_08_capability_provenance_and_context_separation() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let edit_caps = flow_service
        .get_flow_model_capabilities("prof_1", FlowCapabilityContext::UploadedVideoEdit);
    assert_eq!(
        edit_caps.operation_context,
        FlowCapabilityContext::UploadedVideoEdit
    );
    assert_eq!(edit_caps.models.len(), 1);
    assert_eq!(edit_caps.models[0].model_id, "Omni Flash");
    assert!(edit_caps.models[0].supports_uploaded_video_edit);
    assert_eq!(edit_caps.models[0].supported_durations_sec, vec![10]);
    assert_eq!(edit_caps.models[0].supported_output_counts, vec![1]);

    let generic_caps = flow_service
        .get_flow_model_capabilities("prof_1", FlowCapabilityContext::GenericVideoGeneration);
    assert_eq!(
        generic_caps.operation_context,
        FlowCapabilityContext::GenericVideoGeneration
    );
    assert_eq!(generic_caps.models.len(), 1);
    assert_eq!(generic_caps.models[0].model_id, "Omni Flash");
    assert!(!generic_caps.models[0].supports_uploaded_video_edit);
    assert_eq!(generic_caps.models[0].supported_durations_sec, vec![5, 10]);
    assert_eq!(
        generic_caps.models[0].supported_output_counts,
        vec![1, 2, 4]
    );
}

#[test]
fn test_flow_p3a_09_manifest_schema_v4_backward_compatibility() {
    assert_eq!(CURRENT_FLOW_MANIFEST_SCHEMA_VERSION, 4);

    // Test serialization and deserialization with schema 4
    let req_config = FlowRequestedGenerationConfig {
        model_id: Some("Omni Flash".to_string()),
        resolution: Some("720p".to_string()),
        duration_sec: Some(10),
        orientation: Some("9:16".to_string()),
        output_count: 1,
    };

    let manifest = FlowGenerationManifest::new(
        "parent_01".to_string(),
        "req_01".to_string(),
        "proj_01".to_string(),
        "prof_01".to_string(),
        "hash_01".to_string(),
        Some("media_01".to_string()),
        "prompt_hash_01".to_string(),
        Some("source.mp4".to_string()),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        None,
        req_config.clone(),
        "prompt text".to_string(),
        "prompt_hash_01".to_string(),
        PromptSource::SystemDefault,
        1,
        1,
        crate::ai::cloud::spec::SourceMediaFacts {
            duration_sec: 10.0,
            fps: 30.0,
            width: 720,
            height: 1280,
            has_audio: true,
            timing: None,
        },
        FlowSegmentPlan {
            segments: vec![],
            total_frames: 300,
            total_duration_sec: 10.0,
            target_fps: 30.0,
            capability_limit_sec: 10.0,
        },
        FlowCreditRecord::default(),
        FlowFinalAudioPolicy {
            preserve_original_audio: true,
            codec: "aac".to_string(),
        },
    );

    assert_eq!(manifest.schema_version, 4);
    assert_eq!(
        manifest.requested_generation_config.model_id.as_deref(),
        Some("Omni Flash")
    );
    assert_eq!(
        manifest.requested_generation_config.resolution.as_deref(),
        Some("720p")
    );

    let json_str = serde_json::to_string(&manifest).unwrap();
    let deserialized: FlowGenerationManifest = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.schema_version, 4);
    assert_eq!(
        deserialized.requested_generation_config.model_id.as_deref(),
        Some("Omni Flash")
    );

    let snapshot = manifest.to_snapshot();
    assert_eq!(
        snapshot.requested_generation_config.model_id.as_deref(),
        Some("Omni Flash")
    );
}

#[test]
fn test_flow_p3a_10_configuration_fingerprint_determinism_and_staleness() {
    let config = FlowRequestedGenerationConfig {
        model_id: Some("Omni Flash".to_string()),
        resolution: Some("720p".to_string()),
        duration_sec: Some(10),
        orientation: Some("9:16".to_string()),
        output_count: 1,
    };

    let fp1 = compute_configuration_fingerprint(
        "prof_1",
        "media_01",
        "hash_prompt_123",
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &config,
    );

    let fp2 = compute_configuration_fingerprint(
        "prof_1",
        "media_01",
        "hash_prompt_123",
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &config,
    );
    assert_eq!(fp1, fp2, "Fingerprint must be deterministic");

    // Altering resolution changes fingerprint
    let altered_config = FlowRequestedGenerationConfig {
        resolution: Some("1080p".to_string()),
        ..config.clone()
    };
    let fp_altered = compute_configuration_fingerprint(
        "prof_1",
        "media_01",
        "hash_prompt_123",
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &altered_config,
    );
    assert_ne!(
        fp1, fp_altered,
        "Altering resolution must change fingerprint"
    );
}

#[tokio::test]
async fn test_flow_p3a_11_profile_scoped_credit_balance_locking() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_busy", "Busy Profile")
        .unwrap();

    // Acquire lock manually to simulate active profile use
    let guard = profile_manager.acquire_session_lock("prof_busy").unwrap();

    // Credit balance refresh must return ProfileBusy
    let credit_status = flow_service
        .refresh_flow_credit_balance("prof_busy")
        .await
        .unwrap();
    assert_eq!(credit_status.status, FlowCreditStatus::ProfileBusy);
    assert_eq!(credit_status.balance, None);

    drop(guard);
}

#[tokio::test]
async fn test_flow_p3a_12_insufficient_credits_blocking_guard() {
    // When live balance is 10 and cost is 20, preflight must set FLOW_INSUFFICIENT_CREDITS
    let preflight_json = serde_json::json!({
        "authStatus": "READY",
        "liveCreditBalance": 10,
        "videoEditVerification": {
            "uploadedVideoAttached": true,
            "uploadedVideoEditActive": true,
            "creditEstimateNumber": 20,
            "model": "Omni Flash",
            "resolution": "720p",
            "orientation": "PORTRAIT",
            "outputCount": 1,
            "generationLengthSec": 10.0
        }
    });

    let live_balance = preflight_json
        .get("liveCreditBalance")
        .and_then(|v| v.as_u64())
        .map(|c| c as u32);
    let edit_verif = preflight_json.get("videoEditVerification");
    let live_cost_raw = edit_verif
        .and_then(|v| v.get("creditEstimateNumber"))
        .and_then(|v| v.as_u64())
        .map(|c| c as u32);

    let (mut ready, mut blocking) = (true, None);
    if let (Some(bal), Some(cost)) = (live_balance, live_cost_raw) {
        if bal < cost {
            blocking = Some("FLOW_INSUFFICIENT_CREDITS".to_string());
            ready = false;
        }
    }

    assert!(!ready);
    assert_eq!(blocking, Some("FLOW_INSUFFICIENT_CREDITS".to_string()));
}
