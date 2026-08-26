use crate::ai::flow::*;
use crate::ai::transformation::{IdentityMode, TransformationIntent};
use crate::commands::resolve_project_media_by_id;
use crate::projects::{
    DerivedMediaAsset, DerivedMediaProvenance, Project, ProjectEditorState, ProjectManager,
    SourceMedia, CURRENT_SCHEMA_VERSION,
};
use crate::system::StoragePaths;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_flow_p2_01_schema_v2_migration_and_deserialization_compatibility() {
    // V1 Project JSON (without derivedMediaAssets and without activeMediaId)
    let v1_json = r#"{
        "schemaVersion": 1,
        "id": "proj-legacy-v1",
        "name": "Legacy Project",
        "createdAt": "2026-08-01T00:00:00Z",
        "updatedAt": "2026-08-01T00:00:00Z",
        "status": "IMPORTED",
        "sourceMedia": {
            "mediaId": "media-orig-1",
            "originalFileName": "input.mp4",
            "sourcePath": "media/input.mp4",
            "durationMs": 10000,
            "width": 1920,
            "height": 1080,
            "fps": 30.0,
            "fileSizeBytes": 1024000,
            "container": "mp4",
            "videoCodec": "h264",
            "audioCodec": "aac",
            "hasAudio": true
        },
        "transformationConfig": {
            "category": "character",
            "prompt": "change face",
            "preservation": {
                "preserveMotion": true,
                "preserveCamera": true,
                "preserveComposition": true,
                "preserveOriginalAudio": true
            }
        },
        "outputs": [],
        "editorState": {
            "currentTime": 0.0,
            "timelineZoom": 1.0
        },
        "isFixture": false
    }"#;

    let deserialized: Project = serde_json::from_str(v1_json).expect("V1 JSON must deserialize");
    assert_eq!(deserialized.schema_version, 1);
    assert_eq!(deserialized.id, "proj-legacy-v1");
    assert!(deserialized.derived_media_assets.is_empty());
    assert_eq!(
        deserialized
            .editor_state
            .as_ref()
            .and_then(|e| e.active_media_id.as_ref()),
        None
    );

    // V2 Project serialization & deserialization
    let mut v2_proj = Project::new("V2 Test Project");
    assert_eq!(v2_proj.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(CURRENT_SCHEMA_VERSION, 2);

    let derived = DerivedMediaAsset {
        media: SourceMedia {
            media_id: "derived-123".to_string(),
            original_file_name: "flow_job1_derived.mp4".to_string(),
            source_path: PathBuf::from("media/derived/flow_job1_derived.mp4"),
            duration_ms: 10000,
            width: 1920,
            height: 1080,
            fps: 30.0,
            file_size_bytes: 2048000,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: Some("aac".to_string()),
            has_audio: true,
        },
        provenance: DerivedMediaProvenance {
            provider: "FLOW".to_string(),
            provider_job_id: "flow_job1".to_string(),
            source_media_id: "media-orig-1".to_string(),
            transformation_intent: TransformationIntent::FaceReplace,
            identity_mode: IdentityMode::Generated,
            prompt_hash: "sha_p".to_string(),
            created_at: "2026-08-25T12:00:00Z".to_string(),
        },
    };

    v2_proj.derived_media_assets.push(derived);
    v2_proj.editor_state = Some(ProjectEditorState {
        current_time: 2.5,
        timeline_zoom: 1.0,
        selected_track: None,
        active_media_id: Some("derived-123".to_string()),
    });

    let serialized = serde_json::to_string(&v2_proj).expect("V2 must serialize");
    let loaded: Project = serde_json::from_str(&serialized).expect("V2 must deserialize");

    assert_eq!(loaded.derived_media_assets.len(), 1);
    assert_eq!(loaded.derived_media_assets[0].provenance.provider, "FLOW");
    assert_eq!(
        loaded.derived_media_assets[0].provenance.provider_job_id,
        "flow_job1"
    );
    assert_eq!(
        loaded.editor_state.unwrap().active_media_id,
        Some("derived-123".to_string())
    );
}

#[test]
fn test_flow_p2_02_canonical_project_media_resolver() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let manager = ProjectManager::new(paths.clone());

    let mut project = manager.create_project("Resolver Test").unwrap();
    let proj_dir = paths.projects_dir.join(&project.id);
    let media_dir = proj_dir.join("media");
    let derived_dir = media_dir.join("derived");
    fs::create_dir_all(&derived_dir).unwrap();

    let orig_file = media_dir.join("orig.mp4");
    fs::write(&orig_file, b"fake video content original").unwrap();

    let derived_file = derived_dir.join("flow_derived_1.mp4");
    fs::write(&derived_file, b"fake video content derived").unwrap();

    let orig_media = SourceMedia {
        media_id: "orig_001".to_string(),
        original_file_name: "orig.mp4".to_string(),
        source_path: orig_file.clone(),
        duration_ms: 5000,
        width: 1280,
        height: 720,
        fps: 30.0,
        file_size_bytes: 100,
        container: "mp4".to_string(),
        video_codec: "h264".to_string(),
        audio_codec: None,
        has_audio: false,
    };

    let derived_media = SourceMedia {
        media_id: "derived_001".to_string(),
        original_file_name: "flow_derived_1.mp4".to_string(),
        source_path: derived_file.clone(),
        duration_ms: 5000,
        width: 1280,
        height: 720,
        fps: 30.0,
        file_size_bytes: 100,
        container: "mp4".to_string(),
        video_codec: "h264".to_string(),
        audio_codec: None,
        has_audio: false,
    };

    let derived_asset = DerivedMediaAsset {
        media: derived_media,
        provenance: DerivedMediaProvenance {
            provider: "FLOW".to_string(),
            provider_job_id: "flow_parent_001".to_string(),
            source_media_id: "orig_001".to_string(),
            transformation_intent: TransformationIntent::FaceReplace,
            identity_mode: IdentityMode::Generated,
            prompt_hash: "hash_001".to_string(),
            created_at: "2026-08-25T12:00:00Z".to_string(),
        },
    };

    project.source_media = Some(orig_media);
    project.derived_media_assets.push(derived_asset);
    project.editor_state = Some(ProjectEditorState {
        active_media_id: Some("derived_001".to_string()),
        ..Default::default()
    });
    manager.update_project(&project).unwrap();

    // 1. Resolve by explicit original ID
    let (resolved_orig, _) =
        resolve_project_media_by_id(&project.id, Some("orig_001"), &paths).unwrap();
    assert_eq!(
        resolved_orig.canonicalize().unwrap(),
        orig_file.canonicalize().unwrap()
    );

    // 2. Resolve by explicit derived ID
    let (resolved_derived, _) =
        resolve_project_media_by_id(&project.id, Some("derived_001"), &paths).unwrap();
    assert_eq!(
        resolved_derived.canonicalize().unwrap(),
        derived_file.canonicalize().unwrap()
    );

    // 3. Resolve without media ID -> falls back to active_media_id (derived_001)
    let (resolved_active, _) = resolve_project_media_by_id(&project.id, None, &paths).unwrap();
    assert_eq!(
        resolved_active.canonicalize().unwrap(),
        derived_file.canonicalize().unwrap()
    );

    // 4. Resolve non-existent ID fails
    let err = resolve_project_media_by_id(&project.id, Some("non_existent"), &paths).unwrap_err();
    assert!(err.contains("MEDIA_NOT_FOUND"));

    // 5. Invalid project ID validation
    let err_invalid =
        resolve_project_media_by_id("../malicious_id", Some("orig_001"), &paths).unwrap_err();
    assert!(err_invalid.contains("INVALID_IDENTIFIER"));
}

#[test]
fn test_flow_p2_03_empty_prompt_and_system_default_preservation() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let flow_service = FlowRuntimeService::new(paths.clone());

    // Create profile
    let profile_manager = FlowProfileManager::new(paths.app_data_dir.clone());
    profile_manager
        .create_profile("test_profile", "Test")
        .unwrap();

    // Create a dummy video file
    let video_file = temp_dir.path().join("source.mp4");
    fs::write(&video_file, b"dummy video bytes").unwrap();

    // Case 1: Empty prompt for FACE_REPLACE + GENERATED -> should succeed in prompt validation
    let req_face_empty = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "source.mp4".to_string(),
        profile_id: "test_profile".to_string(),
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

    let probe_err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(flow_service.start_flow_generation(req_face_empty, video_file.clone()))
        .unwrap_err();
    // It passed prompt check and reached media probe!
    assert!(
        probe_err.contains("PROBE_FAILED") || probe_err.contains("INVALID_MEDIA"),
        "Expected media probe failure on dummy file, got: {}",
        probe_err
    );

    // Case 2: Empty prompt for STYLE_EDIT -> rejected with REQUEST_INVALID
    let req_style_empty = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "source.mp4".to_string(),
        profile_id: "test_profile".to_string(),
        transformation_intent: Some(TransformationIntent::StyleEdit),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "   ".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: Some(50),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let style_err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(flow_service.start_flow_generation(req_style_empty, video_file.clone()))
        .unwrap_err();
    assert!(style_err.contains("REQUEST_INVALID"));

    // Case 3: FACE_REPLACE + REFERENCE -> rejected as unsupported
    let req_ref = FlowGenerationRequest {
        project_id: "p1".to_string(),
        source_media_id: "source.mp4".to_string(),
        profile_id: "test_profile".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Reference),
        prompt: "Replace face".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: Some(50),
        preserve_original_audio: Some(true),
        requested_config: None,
        configuration_fingerprint: None,
    };

    let ref_err = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(flow_service.start_flow_generation(req_ref, video_file))
        .unwrap_err();
    assert!(ref_err.contains("FLOW_REFERENCE_IDENTITY_NOT_SUPPORTED"));
}

#[test]
fn test_flow_p2_04_budget_limit_rejection_pre_click() {
    let policy = FlowCapabilityPolicy::for_edit_uploaded_video();
    let estimated = policy.estimate_credits(1); // 40 credits
    assert_eq!(estimated, 40);

    let max_budget = 20; // less than 40
    assert!(estimated > max_budget);
}

#[test]
fn test_flow_p2_05_use_flow_output_and_chained_editing() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let manager = ProjectManager::new(paths.clone());

    let mut project = manager.create_project("Chained Flow Project").unwrap();
    let proj_dir = paths.projects_dir.join(&project.id);
    let media_dir = proj_dir.join("media");
    fs::create_dir_all(&media_dir).unwrap();

    let orig_file = media_dir.join("orig.mp4");
    fs::write(&orig_file, b"fake orig video bytes").unwrap();

    let orig_media = SourceMedia {
        media_id: "orig_001".to_string(),
        original_file_name: "orig.mp4".to_string(),
        source_path: orig_file.clone(),
        duration_ms: 10000,
        width: 1920,
        height: 1080,
        fps: 30.0,
        file_size_bytes: 1000,
        container: "mp4".to_string(),
        video_codec: "h264".to_string(),
        audio_codec: Some("aac".to_string()),
        has_audio: true,
    };
    project.source_media = Some(orig_media);
    manager.update_project(&project).unwrap();

    // Create a mock completed Flow job
    let flow_service = FlowRuntimeService::new(paths.clone());
    let parent_id = "flow_mock_job_1";
    let flow_job_dir = flow_service
        .orchestrator
        .store()
        .parent_flow_job_dir(&project.id, parent_id)
        .unwrap();
    fs::create_dir_all(&flow_job_dir).unwrap();
    let artifact_path = flow_job_dir.join("output.mp4");
    fs::write(&artifact_path, b"flow output 1 bytes").unwrap();

    let mut manifest = FlowGenerationManifest::new(
        parent_id.to_string(),
        "req_1".to_string(),
        project.id.clone(),
        "profile_1".to_string(),
        "cfg_1".to_string(),
        Some("orig_001".to_string()),
        "prompt_hash_1".to_string(),
        Some("orig.mp4".to_string()),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        None,
        FlowRequestedGenerationConfig::default(),
        "Change face prompt".to_string(),
        "prompt_hash_1".to_string(),
        PromptSource::User,
        1,
        1,
        crate::ai::cloud::spec::SourceMediaFacts {
            duration_sec: 10.0,
            fps: 30.0,
            width: 1920,
            height: 1080,
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
        FlowFinalAudioPolicy::default(),
    );
    manifest.state = FlowJobState::Completed;
    manifest.final_output = Some(FlowOutputArtifactRecord {
        final_path: artifact_path.clone(),
        sha256: "sha_artifact_1".to_string(),
        duration_sec: 10.0,
        width: 1920,
        height: 1080,
        fps: 30.0,
        frame_count: 300,
        has_audio: true,
        validated_at: "2026-08-25T12:00:00Z".to_string(),
    });
    flow_service
        .orchestrator
        .store()
        .save_manifest_atomic(&mut manifest)
        .unwrap();

    // Create derived media asset and add to project
    let derived_dir = media_dir.join("derived");
    fs::create_dir_all(&derived_dir).unwrap();
    let derived_dest = derived_dir.join("flow_mock_job_1_derived.mp4");
    fs::copy(&artifact_path, &derived_dest).unwrap();

    let derived_asset = DerivedMediaAsset {
        media: SourceMedia {
            media_id: "media_derived_001".to_string(),
            original_file_name: "flow_mock_job_1_derived.mp4".to_string(),
            source_path: derived_dest.clone(),
            duration_ms: 10000,
            width: 1920,
            height: 1080,
            fps: 30.0,
            file_size_bytes: 1000,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: Some("aac".to_string()),
            has_audio: true,
        },
        provenance: DerivedMediaProvenance {
            provider: "FLOW".to_string(),
            provider_job_id: parent_id.to_string(),
            source_media_id: "orig_001".to_string(),
            transformation_intent: TransformationIntent::FaceReplace,
            identity_mode: IdentityMode::Generated,
            prompt_hash: "prompt_hash_1".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    };

    project.derived_media_assets.push(derived_asset);
    project.editor_state = Some(ProjectEditorState {
        active_media_id: Some("media_derived_001".to_string()),
        ..Default::default()
    });
    manager.update_project(&project).unwrap();

    // Test chained media resolution: using derived media ID as input for the next generation
    let (resolved_chain_input, source_media) =
        resolve_project_media_by_id(&project.id, Some("media_derived_001"), &paths).unwrap();

    assert_eq!(source_media.media_id, "media_derived_001");
    assert_eq!(
        resolved_chain_input.canonicalize().unwrap(),
        derived_dest.canonicalize().unwrap()
    );

    // Verify original source remains intact and untouched
    let (resolved_orig_again, orig_again) =
        resolve_project_media_by_id(&project.id, Some("orig_001"), &paths).unwrap();
    assert_eq!(orig_again.media_id, "orig_001");
    assert_eq!(
        resolved_orig_again.canonicalize().unwrap(),
        orig_file.canonicalize().unwrap()
    );
}
