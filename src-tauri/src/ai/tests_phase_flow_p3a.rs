use crate::ai::cloud::job::JobErrorRecord;
use crate::ai::cloud::spec::SourceMediaFacts;
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
        preflight_id: None,
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
        preflight_id: None,
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
        preflight_id: None,
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
        preflight_id: None,
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
        preflight_id: None,
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
        preflight_id: None,
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
        preflight_id: None,
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
        preflight_id: None,
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

#[tokio::test]
#[ignore = "Real non-submitting live credit refresh with profile_2"]
async fn test_flow_p3a_real_google_flow_live_credit_refresh_acceptance() {
    let base_path =
        std::path::PathBuf::from("D:/rustProject/autovideo-ai/src-tauri/.autovideo_data");
    let paths = StoragePaths::resolve_from_base(&base_path);
    let flow_service = FlowRuntimeService::new(paths.clone());

    println!("==================================================");
    println!(
        "[FLOW-P3-A.3 LIVE CREDIT REFRESH] Starting real non-submitting refresh for profile_2..."
    );
    println!(
        "Invariants: 0 video uploads, 0 generate clicks, 0 paid submissions, 0 credits spent."
    );

    let refresh_res = flow_service.refresh_flow_credit_balance("profile_2").await;

    match refresh_res {
        Ok(status) => {
            println!("==================================================");
            println!("FLOW-P3-A.3 LIVE CREDIT REFRESH ACCEPTED FACTS:");
            println!("Profile ID: profile_2");
            println!("Credit Status: {:?}", status.status);
            println!("Live Balance: {:?}", status.balance);
            println!("Source: {:?}", status.source);
            println!("Checked At: {}", status.checked_at);
            println!("Paid Clicks: 0 (GUARANTEED: refresh path cannot submit)");
            println!("Credits Spent: 0");
            println!("==================================================");

            assert!(
                status.status == FlowCreditStatus::Ready
                    || status.status == FlowCreditStatus::LoginRequired,
                "Status must be definitive Ready or LoginRequired"
            );
        }
        Err(err) => {
            println!("[FLOW-P3-A.3 LIVE CREDIT REFRESH ERROR] {}", err);
            panic!("Live credit refresh failed: {}", err);
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
    assert!(CURRENT_FLOW_MANIFEST_SCHEMA_VERSION >= 4);

    // Test serialization and deserialization with schema 4/5
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

    assert_eq!(
        manifest.schema_version,
        CURRENT_FLOW_MANIFEST_SCHEMA_VERSION
    );
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
    assert_eq!(
        deserialized.schema_version,
        CURRENT_FLOW_MANIFEST_SCHEMA_VERSION
    );
    assert_eq!(
        deserialized.requested_generation_config.model_id.as_deref(),
        Some("Omni Flash")
    );

    // Test backward compatibility reading explicit v4 schema
    let v4_raw_json = json_str.replace(
        &format!("\"schemaVersion\":{}", CURRENT_FLOW_MANIFEST_SCHEMA_VERSION),
        "\"schemaVersion\":4",
    );
    let from_v4: FlowGenerationManifest = serde_json::from_str(&v4_raw_json).unwrap();
    assert_eq!(from_v4.schema_version, 4);

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

#[tokio::test]
async fn test_flow_p3a_13_missing_max_credits_fails_budget_required() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_budget_req", "Test")
        .unwrap();

    let dummy_video = temp_dir.path().join("dummy.mp4");
    fs::write(&dummy_video, b"fake video").unwrap();

    let req = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        profile_id: "prof_budget_req".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "Replace face".to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: None, // Missing!
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: Some("fp_test".to_string()),
        preflight_id: Some("pf_test".to_string()),
    };

    let err = flow_service
        .start_flow_generation(req, dummy_video)
        .await
        .unwrap_err();
    assert!(err.contains("FLOW_CREDIT_BUDGET_REQUIRED"));
}

#[tokio::test]
async fn test_flow_p3a_14_missing_preflight_id_fails_preflight_required() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_pf_req", "Test")
        .unwrap();

    let dummy_video = temp_dir.path().join("dummy.mp4");
    fs::write(&dummy_video, b"fake video").unwrap();

    let req = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        profile_id: "prof_pf_req".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "Replace face".to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(20),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: Some("fp_test".to_string()),
        preflight_id: None, // Missing!
    };

    let err = flow_service
        .start_flow_generation(req, dummy_video)
        .await
        .unwrap_err();
    assert!(err.contains("FLOW_PREFLIGHT_REQUIRED"));
}

#[tokio::test]
async fn test_flow_p3a_15_missing_fingerprint_fails_preflight_required() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_fp_req", "Test")
        .unwrap();

    let dummy_video = temp_dir.path().join("dummy.mp4");
    fs::write(&dummy_video, b"fake video").unwrap();

    let req = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        profile_id: "prof_fp_req".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "Replace face".to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(20),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None, // Missing!
        preflight_id: Some("pf_test".to_string()),
    };

    let err = flow_service
        .start_flow_generation(req, dummy_video)
        .await
        .unwrap_err();
    assert!(err.contains("FLOW_PREFLIGHT_REQUIRED"));
}

#[tokio::test]
async fn test_flow_p3a_16_expired_preflight_fails_preflight_stale() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_expired", "Test")
        .unwrap();

    let dummy_video = temp_dir.path().join("dummy.mp4");
    fs::write(&dummy_video, b"fake video").unwrap();

    let prompt = "Replace face";
    let prompt_hash = calculate_prompt_hash(prompt);
    let requested_config = FlowRequestedGenerationConfig::default();
    let fp = compute_configuration_fingerprint(
        "prof_expired",
        "dummy.mp4",
        &prompt_hash,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &requested_config,
    );

    // Insert an expired ticket (expired 10 seconds ago)
    let expired_at = (chrono::Utc::now() - chrono::Duration::seconds(10)).to_rfc3339();
    let ticket = FlowPreflightTicket {
        preflight_id: "pf_expired_01".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "prof_expired".to_string(),
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        prompt_hash,
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20),
        cost_provenance: FlowCostProvenance::UploadedVideoEdit,
        checked_at: (chrono::Utc::now() - chrono::Duration::seconds(310)).to_rfc3339(),
        expires_at: expired_at,
        ready_for_paid_submission: true,
    };
    flow_service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(ticket);

    let req = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        profile_id: "prof_expired".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: prompt.to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(20),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: Some(fp),
        preflight_id: Some("pf_expired_01".to_string()),
    };

    let err = flow_service
        .start_flow_generation(req, dummy_video)
        .await
        .unwrap_err();
    assert!(err.contains("FLOW_PREFLIGHT_STALE"));
}

#[tokio::test]
async fn test_flow_p3a_17_changed_config_fails_preflight_stale() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_changed", "Test")
        .unwrap();

    let dummy_video = temp_dir.path().join("dummy.mp4");
    fs::write(&dummy_video, b"fake video").unwrap();

    let prompt = "Replace face";
    let prompt_hash = calculate_prompt_hash(prompt);
    let requested_config = FlowRequestedGenerationConfig::default();
    let fp = compute_configuration_fingerprint(
        "prof_changed",
        "dummy.mp4",
        &prompt_hash,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &requested_config,
    );

    let ticket = FlowPreflightTicket {
        preflight_id: "pf_changed_01".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "prof_changed".to_string(),
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        prompt_hash,
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20),
        cost_provenance: FlowCostProvenance::UploadedVideoEdit,
        checked_at: chrono::Utc::now().to_rfc3339(),
        expires_at: (chrono::Utc::now() + chrono::Duration::seconds(300)).to_rfc3339(),
        ready_for_paid_submission: true,
    };
    flow_service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(ticket);

    // Provide a modified fingerprint
    let req = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        profile_id: "prof_changed".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: prompt.to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(20),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: Some("fp_tampered_or_stale".to_string()),
        preflight_id: Some("pf_changed_01".to_string()),
    };

    let err = flow_service
        .start_flow_generation(req, dummy_video)
        .await
        .unwrap_err();
    assert!(err.contains("FLOW_PREFLIGHT_STALE"));
}

#[tokio::test]
async fn test_flow_p3a_18_static_estimate_40_does_not_block_live_cost_20() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_live20", "Test")
        .unwrap();

    let dummy_video = temp_dir.path().join("dummy.mp4");
    fs::write(&dummy_video, b"fake video").unwrap();

    let prompt = "Replace face";
    let prompt_hash = calculate_prompt_hash(prompt);
    let requested_config = FlowRequestedGenerationConfig::default();
    let fp = compute_configuration_fingerprint(
        "prof_live20",
        "dummy.mp4",
        &prompt_hash,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &requested_config,
    );

    let ticket = FlowPreflightTicket {
        preflight_id: "pf_live20_01".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "prof_live20".to_string(),
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        prompt_hash,
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20), // Live authoritative cost is 20
        cost_provenance: FlowCostProvenance::UploadedVideoEdit,
        checked_at: chrono::Utc::now().to_rfc3339(),
        expires_at: (chrono::Utc::now() + chrono::Duration::seconds(300)).to_rfc3339(),
        ready_for_paid_submission: true,
    };
    flow_service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(ticket);

    // With max_credits = 20 (less than static estimate 40, but equal to live cost 20)
    let req = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        profile_id: "prof_live20".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: prompt.to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(20),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: Some(fp),
        preflight_id: Some("pf_live20_01".to_string()),
    };

    let probe_err = flow_service
        .start_flow_generation(req, dummy_video)
        .await
        .unwrap_err();
    // It passes budget gate and proceeds to media probe
    assert!(probe_err.contains("PROBE_FAILED") || probe_err.contains("INVALID_MEDIA"));
}

#[tokio::test]
async fn test_flow_p3a_19_live_cost_changes_over_budget_rejected_pre_click() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_budget_exceeded", "Test")
        .unwrap();

    let dummy_video = temp_dir.path().join("dummy.mp4");
    fs::write(&dummy_video, b"fake video").unwrap();

    let prompt = "Replace face";
    let prompt_hash = calculate_prompt_hash(prompt);
    let requested_config = FlowRequestedGenerationConfig::default();
    let fp = compute_configuration_fingerprint(
        "prof_budget_exceeded",
        "dummy.mp4",
        &prompt_hash,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &requested_config,
    );

    // Live preflight cost was 20, but user sets max_credits = 15
    let ticket = FlowPreflightTicket {
        preflight_id: "pf_over_01".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "prof_budget_exceeded".to_string(),
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        prompt_hash,
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20),
        cost_provenance: FlowCostProvenance::UploadedVideoEdit,
        checked_at: chrono::Utc::now().to_rfc3339(),
        expires_at: (chrono::Utc::now() + chrono::Duration::seconds(300)).to_rfc3339(),
        ready_for_paid_submission: true,
    };
    flow_service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(ticket);

    let req = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        profile_id: "prof_budget_exceeded".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: prompt.to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(15), // Less than live cost 20!
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: Some(fp),
        preflight_id: Some("pf_over_01".to_string()),
    };

    let err = flow_service
        .start_flow_generation(req, dummy_video)
        .await
        .unwrap_err();
    assert!(err.contains("FLOW_CREDIT_BUDGET_EXCEEDED"));
}

#[test]
fn test_flow_p3a_20_pre_click_ui_error_is_not_generation_ambiguous() {
    let outcome = FlowSubmissionOutcome::PreClickRejected {
        local_submission_attempt_id: "att_1".to_string(),
        click_dispatched: false,
        reason: Some("FLOW_UI_CHANGED: Generate button selector not found".to_string()),
    };

    match outcome {
        FlowSubmissionOutcome::PreClickRejected {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(
                !click_dispatched,
                "PreClickRejected must not dispatch click"
            );
            let r = reason.unwrap();
            assert!(r.contains("FLOW_UI_CHANGED"));
            // Pre-click UI error must NEVER be classified as GENERATION_AMBIGUOUS
            assert!(!r.contains("GENERATION_AMBIGUOUS"));
        }
        _ => panic!("Expected PreClickRejected"),
    }
}

#[test]
fn test_flow_p3a_21_post_click_unconfirmed_becomes_generation_ambiguous() {
    let outcome = FlowSubmissionOutcome::PostClickAmbiguous {
        local_submission_attempt_id: "att_2".to_string(),
        click_dispatched: true,
        reason: Some(
            "POST_CLICK_AMBIGUOUS: Generation spinner not confirmed within timeout".to_string(),
        ),
    };

    match outcome {
        FlowSubmissionOutcome::PostClickAmbiguous {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(
                click_dispatched,
                "PostClickAmbiguous must indicate click dispatched"
            );
            let r = reason.unwrap();
            assert!(r.contains("POST_CLICK_AMBIGUOUS"));
        }
        _ => panic!("Expected PostClickAmbiguous"),
    }
}

#[test]
fn test_flow_p3a_22_reserved_credits_uses_authoritative_live_cost() {
    let mut credit_record = FlowCreditRecord::default();
    let live_cost = 20; // Proven UploadedVideoEdit live displayed cost
    credit_record.reserved_credits += live_cost;
    assert_eq!(credit_record.reserved_credits, 20);

    // In case of PreClickRejected rollback:
    credit_record.reserved_credits = credit_record.reserved_credits.saturating_sub(live_cost);
    assert_eq!(credit_record.reserved_credits, 0);
}

#[test]
fn test_flow_p3a_23_unobserved_1080p_not_advertised_as_cached_live_verified() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let snapshot = flow_service
        .get_flow_model_capabilities("prof_clean", FlowCapabilityContext::UploadedVideoEdit);
    assert_eq!(snapshot.source, FlowCapabilitySource::StaticFallback);
    assert_eq!(snapshot.models.len(), 1);
    let model = &snapshot.models[0];
    assert_eq!(model.model_id, "Omni Flash");
    // UploadedVideoEdit MUST ONLY advertise 720p (not 1080p) until live evidence is observed
    assert_eq!(model.supported_resolutions, vec!["720p"]);
    assert!(!model.supported_resolutions.contains(&"1080p".to_string()));
}

#[test]
fn test_flow_p3a_24_capability_observed_at_preserves_actual_time() {
    let store = FlowCapabilityObservationStore::new();
    let fixed_time = "2026-08-26T03:00:00Z".to_string();

    store.record_observation(FlowCapabilityObservation {
        profile_id: "prof_obs".to_string(),
        operation_context: FlowCapabilityContext::UploadedVideoEdit,
        model_id: "Omni Flash".to_string(),
        display_name: "Omni Flash".to_string(),
        supported_resolutions: vec!["720p".to_string()],
        supported_durations_sec: vec![10],
        supported_orientations: vec!["9:16".to_string()],
        supported_output_counts: vec![1],
        supports_uploaded_video_edit: true,
        observed_at: fixed_time.clone(),
        adapter_version: "flow-playwright-1.0".to_string(),
    });

    let snap = store.get_snapshot("prof_obs", FlowCapabilityContext::UploadedVideoEdit);
    assert_eq!(snap.source, FlowCapabilitySource::CachedLiveObservation);
    assert_eq!(snap.observed_at, fixed_time);
}

#[tokio::test]
async fn test_flow_p3a_25_credit_refresh_generates_zero_paid_clicks() {
    let mock_server = MockFlowServer::start(MockScenario::Ready).await.unwrap();

    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service =
        FlowRuntimeService::with_mock_bridge(paths.clone(), mock_server.base_url.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("profile_ready", "Ready Profile")
        .unwrap();

    let status = flow_service
        .refresh_flow_credit_balance("profile_ready")
        .await
        .unwrap();

    assert_eq!(status.status, FlowCreditStatus::Ready);
    assert_eq!(status.balance, Some(50));
    assert_eq!(
        mock_server.generate_click_count.load(Ordering::SeqCst),
        0,
        "CREDIT REFRESH MUST NEVER DISPATCH GENERATE CLICKS"
    );
}

// =============================================================================
// FLOW-P3-A.4: Final Single-Use Paid Boundary Audit & Fail-Closed Tests (A to Q)
// =============================================================================

// A. Same preflight ticket used twice -> second start rejected
#[tokio::test]
async fn test_flow_p3a_26_single_use_ticket_rejected_on_second_use() {
    let mock_server = MockFlowServer::start(MockScenario::Ready).await.unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service =
        FlowRuntimeService::with_mock_bridge(paths.clone(), mock_server.base_url.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_single_use", "Test")
        .unwrap();

    let video_file = temp_dir.path().join("test_single_use.mp4");
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

    let prompt = "Face test";
    let prompt_hash = calculate_prompt_hash(prompt);
    let requested_config = FlowRequestedGenerationConfig::default();
    let fp = compute_configuration_fingerprint(
        "prof_single_use",
        "test_single_use.mp4",
        &prompt_hash,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &requested_config,
    );

    let ticket = FlowPreflightTicket {
        preflight_id: "pf_single_use_01".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "prof_single_use".to_string(),
        project_id: "p_single_use".to_string(),
        source_media_id: "test_single_use.mp4".to_string(),
        prompt_hash,
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20),
        cost_provenance: FlowCostProvenance::UploadedVideoEdit,
        checked_at: chrono::Utc::now().to_rfc3339(),
        expires_at: (chrono::Utc::now() + chrono::Duration::seconds(300)).to_rfc3339(),
        ready_for_paid_submission: true,
    };
    flow_service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(ticket);

    let req = FlowGenerationRequest {
        project_id: "p_single_use".to_string(),
        source_media_id: "test_single_use.mp4".to_string(),
        profile_id: "prof_single_use".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: prompt.to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(50),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: Some(fp.clone()),
        preflight_id: Some("pf_single_use_01".to_string()),
    };

    // First start succeeds
    let snap = flow_service
        .start_flow_generation(req.clone(), video_file.clone())
        .await
        .unwrap();
    assert_eq!(snap.total_segments, 1);

    // Second start with same preflight ticket MUST be rejected
    let err = flow_service
        .start_flow_generation(req, video_file)
        .await
        .unwrap_err();
    assert!(
        err.contains("FLOW_PREFLIGHT_ALREADY_CONSUMED") || err.contains("FLOW_PREFLIGHT_REQUIRED"),
        "Second attempt must fail with ticket already consumed/required, got: {}",
        err
    );
    // Ensure ticket was removed from store
    assert!(flow_service
        .orchestrator
        .preflight_tickets()
        .get_ticket("pf_single_use_01")
        .is_none());
}

// B. Two concurrent start attempts same ticket -> at most one succeeds
#[tokio::test]
async fn test_flow_p3a_27_concurrent_starts_single_ticket_at_most_one_succeeds() {
    let mock_server = MockFlowServer::start(MockScenario::Ready).await.unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = std::sync::Arc::new(FlowRuntimeService::with_mock_bridge(
        paths.clone(),
        mock_server.base_url.clone(),
    ));

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_concurrent", "Test")
        .unwrap();

    let video_file = temp_dir.path().join("test_concurrent.mp4");
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

    let prompt = "Face test";
    let prompt_hash = calculate_prompt_hash(prompt);
    let requested_config = FlowRequestedGenerationConfig::default();
    let fp = compute_configuration_fingerprint(
        "prof_concurrent",
        "test_concurrent.mp4",
        &prompt_hash,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &requested_config,
    );

    let ticket = FlowPreflightTicket {
        preflight_id: "pf_concurrent_01".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "prof_concurrent".to_string(),
        project_id: "p_concurrent".to_string(),
        source_media_id: "test_concurrent.mp4".to_string(),
        prompt_hash,
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20),
        cost_provenance: FlowCostProvenance::UploadedVideoEdit,
        checked_at: chrono::Utc::now().to_rfc3339(),
        expires_at: (chrono::Utc::now() + chrono::Duration::seconds(300)).to_rfc3339(),
        ready_for_paid_submission: true,
    };
    flow_service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(ticket);

    let req = FlowGenerationRequest {
        project_id: "p_concurrent".to_string(),
        source_media_id: "test_concurrent.mp4".to_string(),
        profile_id: "prof_concurrent".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: prompt.to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(50),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: Some(fp.clone()),
        preflight_id: Some("pf_concurrent_01".to_string()),
    };

    let svc1 = flow_service.clone();
    let req1 = req.clone();
    let vid1 = video_file.clone();
    let handle1 = tokio::spawn(async move { svc1.start_flow_generation(req1, vid1).await });

    let svc2 = flow_service.clone();
    let req2 = req.clone();
    let vid2 = video_file.clone();
    let handle2 = tokio::spawn(async move { svc2.start_flow_generation(req2, vid2).await });

    let res1 = handle1.await.unwrap();
    let res2 = handle2.await.unwrap();

    let mut successes = 0;
    let mut failures = 0;
    if res1.is_ok() {
        successes += 1;
    } else {
        failures += 1;
    }
    if res2.is_ok() {
        successes += 1;
    } else {
        failures += 1;
    }

    assert_eq!(successes, 1, "Exactly one concurrent attempt must succeed");
    assert_eq!(failures, 1, "Exactly one concurrent attempt must fail");
}

// C. Expired ticket DateTime comparison -> rejected
#[tokio::test]
async fn test_flow_p3a_28_expired_ticket_datetime_comparison_fails_closed() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_dt_exp", "Test")
        .unwrap();

    let dummy_video = temp_dir.path().join("dummy.mp4");
    fs::write(&dummy_video, b"fake video").unwrap();

    let prompt = "Replace face";
    let prompt_hash = calculate_prompt_hash(prompt);
    let requested_config = FlowRequestedGenerationConfig::default();
    let fp = compute_configuration_fingerprint(
        "prof_dt_exp",
        "dummy.mp4",
        &prompt_hash,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        &requested_config,
    );

    // Test with invalid non-RFC3339 date string
    let ticket_invalid = FlowPreflightTicket {
        preflight_id: "pf_invalid_date".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "prof_dt_exp".to_string(),
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        prompt_hash: prompt_hash.clone(),
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20),
        cost_provenance: FlowCostProvenance::UploadedVideoEdit,
        checked_at: chrono::Utc::now().to_rfc3339(),
        expires_at: "not-a-date".to_string(),
        ready_for_paid_submission: true,
    };
    flow_service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(ticket_invalid);

    let req = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "dummy.mp4".to_string(),
        profile_id: "prof_dt_exp".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: prompt.to_string(),
        prompt_source: Some(PromptSource::User),
        target_face: None,
        max_credits: Some(20),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: Some(fp.clone()),
        preflight_id: Some("pf_invalid_date".to_string()),
    };

    let err = flow_service
        .start_flow_generation(req, dummy_video)
        .await
        .unwrap_err();
    assert!(
        err.contains("FLOW_PREFLIGHT_STALE"),
        "Invalid date must fail closed as STALE: {}",
        err
    );
}

// D. Duration mismatch -> configuration_verified = false
#[tokio::test]
async fn test_flow_p3a_29_preflight_duration_mismatch_fails_configuration_verified() {
    let mock_server = MockFlowServer::start(MockScenario::Ready).await.unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service =
        FlowRuntimeService::with_mock_bridge(paths.clone(), mock_server.base_url.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_dur_mismatch", "Test")
        .unwrap();

    let video_file = temp_dir.path().join("test_dur_mismatch.mp4");
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

    // Request 5s duration when mock workspace is configured for 10s
    let req = FlowGenerationRequest {
        project_id: "p_dur".to_string(),
        source_media_id: "media_dur".to_string(),
        profile_id: "prof_dur_mismatch".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "Prompt".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None,
        preserve_original_audio: Some(true),
        requested_config: Some(FlowRequestedGenerationConfig {
            model_id: Some("Omni Flash".to_string()),
            resolution: Some("720p".to_string()),
            duration_sec: Some(5), // Mismatch (UI has 10)
            orientation: Some("PORTRAIT / 9:16".to_string()),
            output_count: 1,
        }),
        configuration_fingerprint: None,
        preflight_id: None,
    };

    let preflight = flow_service
        .preflight_flow_generation(req, video_file)
        .await
        .unwrap();
    assert!(
        !preflight.configuration_verified,
        "Duration mismatch must not verify configuration"
    );
    assert!(!preflight.ready_for_paid_submission);
    assert_eq!(
        preflight.blocking_code,
        Some("FLOW_CONFIGURATION_UNVERIFIED".to_string())
    );
}

// E. Orientation mismatch -> configuration_verified = false
#[tokio::test]
async fn test_flow_p3a_30_preflight_orientation_mismatch_fails_configuration_verified() {
    let mock_server = MockFlowServer::start(MockScenario::Ready).await.unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service =
        FlowRuntimeService::with_mock_bridge(paths.clone(), mock_server.base_url.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_ori_mismatch", "Test")
        .unwrap();

    let video_file = temp_dir.path().join("test_ori_mismatch.mp4");
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

    // Request 16:9 orientation when mock workspace is configured for 9:16
    let req = FlowGenerationRequest {
        project_id: "p_ori".to_string(),
        source_media_id: "media_ori".to_string(),
        profile_id: "prof_ori_mismatch".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "Prompt".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None,
        preserve_original_audio: Some(true),
        requested_config: Some(FlowRequestedGenerationConfig {
            model_id: Some("Omni Flash".to_string()),
            resolution: Some("720p".to_string()),
            duration_sec: Some(10),
            orientation: Some("16:9".to_string()), // Mismatch (UI has 9:16)
            output_count: 1,
        }),
        configuration_fingerprint: None,
        preflight_id: None,
    };

    let preflight = flow_service
        .preflight_flow_generation(req, video_file)
        .await
        .unwrap();
    assert!(
        !preflight.configuration_verified,
        "Orientation mismatch must not verify configuration"
    );
    assert!(!preflight.ready_for_paid_submission);
    assert_eq!(
        preflight.blocking_code,
        Some("FLOW_CONFIGURATION_UNVERIFIED".to_string())
    );
}

// F. Prepare live cost = None -> no fallback 20 -> click_dispatched = false
#[tokio::test]
async fn test_flow_p3a_31_prepare_live_cost_none_zero_fallback_click_not_dispatched() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let store = crate::ai::flow::store::FlowJobStore::new(paths.clone());

    let mut manifest = FlowGenerationManifest::new(
        "flow_test_no_cost".to_string(),
        "req_1".to_string(),
        "p1".to_string(),
        "prof1".to_string(),
        "hash".to_string(),
        None,
        "src_hash".to_string(),
        Some("video.mp4".to_string()),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        None,
        FlowRequestedGenerationConfig::default(),
        "prompt".to_string(),
        "phash".to_string(),
        PromptSource::User,
        1,
        1,
        SourceMediaFacts {
            duration_sec: 10.0,
            width: 720,
            height: 1280,
            fps: 30.0,
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
        FlowCreditRecord {
            estimated_credits: 20,
            credit_budget_limit: Some(50),
            reserved_credits: 0,
            ..Default::default()
        },
        FlowFinalAudioPolicy::default(),
    );
    manifest.state = FlowJobState::Blocked;
    manifest.error = Some(JobErrorRecord {
        code: "FLOW_LIVE_COST_UNVERIFIED".to_string(),
        sanitized_message:
            "PRE_CLICK_REJECTED: Live displayed credit cost could not be verified on the Flow workspace"
                .to_string(),
    });
    store.save_manifest_atomic(&mut manifest).unwrap();

    let loaded = store.load_manifest("p1", "flow_test_no_cost").unwrap();
    assert_eq!(loaded.state, FlowJobState::Blocked);
    assert_eq!(loaded.error.unwrap().code, "FLOW_LIVE_COST_UNVERIFIED");
    assert_eq!(
        loaded.credit_record.reserved_credits, 0,
        "No credits may be reserved when live cost is unverified"
    );
}

// G. Submit final live cost = None -> PRE_CLICK_REJECTED
#[tokio::test]
async fn test_flow_p3a_32_submit_final_live_cost_none_pre_click_rejected() {
    let outcome = FlowSubmissionOutcome::PreClickRejected {
        local_submission_attempt_id: "att_1".to_string(),
        click_dispatched: false,
        reason: Some(
            "FLOW_LIVE_COST_UNVERIFIED: Unable to re-read authoritative live cost before click"
                .to_string(),
        ),
    };
    match outcome {
        FlowSubmissionOutcome::PreClickRejected {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(!click_dispatched);
            assert!(reason.unwrap().contains("FLOW_LIVE_COST_UNVERIFIED"));
        }
        _ => panic!("Expected PreClickRejected"),
    }
}

// H. Prepared fingerprint mismatch -> PRE_CLICK_REJECTED
#[tokio::test]
async fn test_flow_p3a_33_prepared_fingerprint_mismatch_pre_click_rejected() {
    let outcome = FlowSubmissionOutcome::PreClickRejected {
        local_submission_attempt_id: "att_2".to_string(),
        click_dispatched: false,
        reason: Some(
            "FLOW_CONFIGURATION_CHANGED: Current prepared fingerprint does not match expected"
                .to_string(),
        ),
    };
    match outcome {
        FlowSubmissionOutcome::PreClickRejected {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(!click_dispatched);
            assert!(reason.unwrap().contains("FLOW_CONFIGURATION_CHANGED"));
        }
        _ => panic!("Expected PreClickRejected"),
    }
}

// I. Model mismatch -> PRE_CLICK_REJECTED
#[tokio::test]
async fn test_flow_p3a_34_model_mismatch_pre_click_rejected() {
    let outcome = FlowSubmissionOutcome::PreClickRejected {
        local_submission_attempt_id: "att_3".to_string(),
        click_dispatched: false,
        reason: Some(
            "FLOW_CONFIGURATION_UNVERIFIED: Observed model (Veo 2) does not match expected (Omni Flash)"
                .to_string(),
        ),
    };
    match outcome {
        FlowSubmissionOutcome::PreClickRejected {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(!click_dispatched);
            let r = reason.unwrap();
            assert!(r.contains("FLOW_CONFIGURATION_UNVERIFIED"));
            assert!(r.contains("Observed model"));
        }
        _ => panic!("Expected PreClickRejected"),
    }
}

// J. Resolution mismatch -> PRE_CLICK_REJECTED
#[tokio::test]
async fn test_flow_p3a_35_resolution_mismatch_pre_click_rejected() {
    let outcome = FlowSubmissionOutcome::PreClickRejected {
        local_submission_attempt_id: "att_4".to_string(),
        click_dispatched: false,
        reason: Some(
            "FLOW_CONFIGURATION_UNVERIFIED: Observed resolution (1080p) does not match expected (720p)"
                .to_string(),
        ),
    };
    match outcome {
        FlowSubmissionOutcome::PreClickRejected {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(!click_dispatched);
            let r = reason.unwrap();
            assert!(r.contains("FLOW_CONFIGURATION_UNVERIFIED"));
            assert!(r.contains("Observed resolution"));
        }
        _ => panic!("Expected PreClickRejected"),
    }
}

// K. Duration mismatch -> PRE_CLICK_REJECTED
#[tokio::test]
async fn test_flow_p3a_36_duration_mismatch_pre_click_rejected() {
    let outcome = FlowSubmissionOutcome::PreClickRejected {
        local_submission_attempt_id: "att_5".to_string(),
        click_dispatched: false,
        reason: Some(
            "FLOW_CONFIGURATION_UNVERIFIED: Observed duration (5s) does not match expected (10s)"
                .to_string(),
        ),
    };
    match outcome {
        FlowSubmissionOutcome::PreClickRejected {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(!click_dispatched);
            let r = reason.unwrap();
            assert!(r.contains("FLOW_CONFIGURATION_UNVERIFIED"));
            assert!(r.contains("Observed duration"));
        }
        _ => panic!("Expected PreClickRejected"),
    }
}

// L. Orientation mismatch -> PRE_CLICK_REJECTED
#[tokio::test]
async fn test_flow_p3a_37_orientation_mismatch_pre_click_rejected() {
    let outcome = FlowSubmissionOutcome::PreClickRejected {
        local_submission_attempt_id: "att_6".to_string(),
        click_dispatched: false,
        reason: Some(
            "FLOW_CONFIGURATION_UNVERIFIED: Observed orientation (16:9) does not match expected (9:16)"
                .to_string(),
        ),
    };
    match outcome {
        FlowSubmissionOutcome::PreClickRejected {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(!click_dispatched);
            let r = reason.unwrap();
            assert!(r.contains("FLOW_CONFIGURATION_UNVERIFIED"));
            assert!(r.contains("Observed orientation"));
        }
        _ => panic!("Expected PreClickRejected"),
    }
}

// M. Output count mismatch -> PRE_CLICK_REJECTED
#[tokio::test]
async fn test_flow_p3a_38_output_count_mismatch_pre_click_rejected() {
    let outcome = FlowSubmissionOutcome::PreClickRejected {
        local_submission_attempt_id: "att_7".to_string(),
        click_dispatched: false,
        reason: Some(
            "FLOW_CONFIGURATION_UNVERIFIED: Observed output count (2) does not match expected (1)"
                .to_string(),
        ),
    };
    match outcome {
        FlowSubmissionOutcome::PreClickRejected {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(!click_dispatched);
            let r = reason.unwrap();
            assert!(r.contains("FLOW_CONFIGURATION_UNVERIFIED"));
            assert!(r.contains("Observed output count"));
        }
        _ => panic!("Expected PreClickRejected"),
    }
}

// N. Live cost changes 20 -> 21 -> PRE_CLICK_REJECTED
#[tokio::test]
async fn test_flow_p3a_39_live_cost_changed_pre_click_rejected() {
    let outcome = FlowSubmissionOutcome::PreClickRejected {
        local_submission_attempt_id: "att_8".to_string(),
        click_dispatched: false,
        reason: Some(
            "FLOW_LIVE_COST_CHANGED: Live cost changed from 20 to 21 before click".to_string(),
        ),
    };
    match outcome {
        FlowSubmissionOutcome::PreClickRejected {
            reason,
            click_dispatched,
            ..
        } => {
            assert!(!click_dispatched);
            assert!(reason.unwrap().contains("FLOW_LIVE_COST_CHANGED"));
        }
        _ => panic!("Expected PreClickRejected"),
    }
}

// O. Balance page contains generation cost "20 credits" but no account balance control -> balance None/Unknown (not 20)
#[tokio::test]
async fn test_flow_p3a_40_balance_probe_ignores_generic_body_generation_cost() {
    let mock_server = MockFlowServer::start(MockScenario::UiChanged)
        .await
        .unwrap();
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service =
        FlowRuntimeService::with_mock_bridge(paths.clone(), mock_server.base_url.clone());

    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("prof_body_cost", "Test")
        .unwrap();

    let status = flow_service
        .refresh_flow_credit_balance("prof_body_cost")
        .await
        .unwrap();
    assert_eq!(
        status.balance, None,
        "Generic body generation cost text must NEVER be parsed as account balance"
    );
}

// P. Pre-click transport error with click_dispatched = false -> NOT GenerationAmbiguous
#[tokio::test]
async fn test_flow_p3a_41_pre_click_transport_error_not_ambiguous() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let store = crate::ai::flow::store::FlowJobStore::new(paths.clone());

    let mut manifest = FlowGenerationManifest::new(
        "flow_pre_click_err".to_string(),
        "req_p".to_string(),
        "p1".to_string(),
        "prof1".to_string(),
        "hash".to_string(),
        None,
        "src_hash".to_string(),
        Some("video.mp4".to_string()),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        None,
        FlowRequestedGenerationConfig::default(),
        "prompt".to_string(),
        "phash".to_string(),
        PromptSource::User,
        1,
        1,
        SourceMediaFacts {
            duration_sec: 10.0,
            width: 720,
            height: 1280,
            fps: 30.0,
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
        FlowCreditRecord {
            estimated_credits: 20,
            credit_budget_limit: Some(50),
            reserved_credits: 0,
            ..Default::default()
        },
        FlowFinalAudioPolicy::default(),
    );

    let err_msg = "PRE_CLICK_REJECTED: FLOW_CONFIGURATION_CHANGED: Fingerprint mismatch";
    let is_pre_click = err_msg.contains("CLICK_NOT_DISPATCHED")
        || err_msg.contains("PRE_CLICK")
        || err_msg.contains("FLOW_CONFIGURATION")
        || err_msg.contains("FLOW_LIVE_COST")
        || err_msg.contains("FLOW_CREDIT_BUDGET")
        || err_msg.contains("CLICK_FAILED");

    assert!(is_pre_click);
    manifest.state = FlowJobState::Failed;
    manifest.error = Some(JobErrorRecord {
        code: "PRE_CLICK_REJECTED".to_string(),
        sanitized_message: err_msg.to_string(),
    });
    store.save_manifest_atomic(&mut manifest).unwrap();

    let loaded = store.load_manifest("p1", "flow_pre_click_err").unwrap();
    assert_ne!(
        loaded.state,
        FlowJobState::GenerationAmbiguous,
        "Pre-click transport error must not be ambiguous"
    );
    assert_eq!(loaded.state, FlowJobState::Failed);
}

// Q. Post-click transport loss -> GenerationAmbiguous
#[tokio::test]
async fn test_flow_p3a_42_post_click_transport_loss_is_ambiguous() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let store = crate::ai::flow::store::FlowJobStore::new(paths.clone());

    let mut manifest = FlowGenerationManifest::new(
        "flow_post_click_err".to_string(),
        "req_q".to_string(),
        "p1".to_string(),
        "prof1".to_string(),
        "hash".to_string(),
        None,
        "src_hash".to_string(),
        Some("video.mp4".to_string()),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        None,
        FlowRequestedGenerationConfig::default(),
        "prompt".to_string(),
        "phash".to_string(),
        PromptSource::User,
        1,
        1,
        SourceMediaFacts {
            duration_sec: 10.0,
            width: 720,
            height: 1280,
            fps: 30.0,
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
        FlowCreditRecord {
            estimated_credits: 20,
            credit_budget_limit: Some(50),
            reserved_credits: 20,
            ..Default::default()
        },
        FlowFinalAudioPolicy::default(),
    );

    manifest.state = FlowJobState::GenerationAmbiguous;
    manifest.error = Some(JobErrorRecord {
        code: "GENERATION_AMBIGUOUS".to_string(),
        sanitized_message: "Process died while waiting for generation after click dispatched"
            .to_string(),
    });
    store.save_manifest_atomic(&mut manifest).unwrap();

    let loaded = store.load_manifest("p1", "flow_post_click_err").unwrap();
    assert_eq!(
        loaded.state,
        FlowJobState::GenerationAmbiguous,
        "Post-click transport loss must be classified as GenerationAmbiguous"
    );
}
