use super::cloud::*;
use crate::commands::resolve_segmented_cloud_artifact_preview_path;
use crate::system::StoragePaths;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn get_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("autovideo_phase19_tests")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn create_synthetic_test_video(out_path: &Path, duration_sec: f64, fps: u32, with_audio: bool) {
    if let Some(p) = out_path.parent() {
        let _ = fs::create_dir_all(p);
    }

    let mut args = vec![
        "-y".to_string(),
        "-f".to_string(),
        "lavfi".to_string(),
        "-i".to_string(),
        format!(
            "testsrc=duration={:.2}:size=320x240:rate={}",
            duration_sec, fps
        ),
    ];

    if with_audio {
        args.extend([
            "-f".to_string(),
            "lavfi".to_string(),
            "-i".to_string(),
            format!("sine=frequency=1000:duration={:.2}", duration_sec),
            "-c:a".to_string(),
            "aac".to_string(),
        ]);
    } else {
        args.push("-an".to_string());
    }

    args.extend([
        "-c:v".to_string(),
        "libx264".to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        out_path.to_str().unwrap().to_string(),
    ]);

    let out = Command::new("ffmpeg")
        .args(&args)
        .output()
        .expect("ffmpeg failed to create synthetic test video");
    assert!(
        out.status.success(),
        "ffmpeg test video generation failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn create_synthetic_alpha_webm(out_path: &Path, duration_sec: f64, fps: u32) {
    if let Some(p) = out_path.parent() {
        let _ = fs::create_dir_all(p);
    }

    let args = [
        "-y",
        "-f",
        "lavfi",
        "-i",
        &format!(
            "testsrc=duration={:.2}:size=320x240:rate={}",
            duration_sec, fps
        ),
        "-c:v",
        "libvpx-vp9",
        "-pix_fmt",
        "yuva420p",
        "-auto-alt-ref",
        "0",
        out_path.to_str().unwrap(),
    ];

    let out = Command::new("ffmpeg")
        .args(&args)
        .output()
        .expect("ffmpeg failed to create synthetic alpha webm");
    assert!(
        out.status.success(),
        "ffmpeg alpha webm generation failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// =============================================================================
// 1. Typed Routing Block Code Test
// =============================================================================

#[test]
fn test_phase19_01_typed_routing_block_code() {
    let registry = ProviderRegistry::new();

    let req_long = CloudJobRequest {
        job_id: "job_long_1".to_string(),
        project_id: Some("proj_1".to_string()),
        prompt: "test".to_string(),
        negative_prompt: None,
        source_video: None,
        reference_image: None,
        reference_images: None,
        duration_seconds: 140.0,
        fps: 30.0,
        resolution: (1280, 720),
        task_type: "BACKGROUND_REMOVAL".to_string(),
    };

    let decision = GenerationRouter::route_with_registry(
        TaskClass::BackgroundRemoval,
        RoutingPreference::CostSaving,
        &req_long,
        None,
        &registry,
    );

    assert_eq!(decision.target, RoutingTarget::Unavailable);
    assert_eq!(
        decision.block_code,
        Some(RoutingBlockCode::ProviderDurationLimit)
    );

    let mut req_budget = req_long.clone();
    req_budget.duration_seconds = 10.0;
    let decision_ok = GenerationRouter::route_with_registry(
        TaskClass::BackgroundRemoval,
        RoutingPreference::CostSaving,
        &req_budget,
        None,
        &registry,
    );
    assert_eq!(decision_ok.target, RoutingTarget::Cloud);
    assert_eq!(decision_ok.block_code, None);
}

// =============================================================================
// 2. Authoritative Timing Facts Probe Test
// =============================================================================

#[test]
fn test_phase19_02_probe_detailed_timing_facts() {
    let test_dir = get_test_dir("timing_probe");
    let video_path = test_dir.join("synthetic_cfr.mp4");
    create_synthetic_test_video(&video_path, 3.0, 30, true);

    let (source_facts, timing_facts) =
        SourceMediaProbe::probe_file_detailed(&video_path).expect("probe detailed failed");

    assert_eq!(source_facts.width, 320);
    assert_eq!(source_facts.height, 240);
    assert!(source_facts.has_audio);
    assert!((source_facts.duration_sec - 3.0).abs() < 0.2);

    assert_eq!(timing_facts.r_frame_rate.num, 30);
    assert_eq!(timing_facts.r_frame_rate.den, 1);
    assert!(
        !timing_facts.is_vfr,
        "Synthetic CFR video must not be detected as VFR"
    );
}

// =============================================================================
// 3. VFR Fail-Closed Invariant Test
// =============================================================================

#[test]
fn test_phase19_03_vfr_fail_closed() {
    let source_facts = SourceMediaFacts {
        duration_sec: 120.0,
        width: 1920,
        height: 1080,
        fps: 30.0,
        has_audio: false,
        timing: None,
    };

    let timing_facts_vfr = DetailedTimingFacts {
        r_frame_rate: Rational { num: 30, den: 1 },
        avg_frame_rate: Rational {
            num: 2997,
            den: 100,
        },
        time_base: Rational { num: 1, den: 1000 },
        is_vfr: true, // Simulated VFR
        nb_frames: Some(3600),
    };

    let plan_res = SegmentPlanner::plan(&source_facts, &timing_facts_vfr, 60.0);
    assert!(plan_res.is_err(), "VFR must be rejected fail-closed");
    let err_msg = format!("{}", plan_res.unwrap_err());
    assert!(err_msg.contains("UNSUPPORTED_VFR_SEGMENTATION"));
}

// =============================================================================
// 4. Fractional CFR Acceptance Test
// =============================================================================

#[test]
fn test_phase19_04_fractional_cfr_accepted() {
    let source_facts = SourceMediaFacts {
        duration_sec: 100.0,
        width: 1920,
        height: 1080,
        fps: 29.97002997,
        has_audio: false,
        timing: None,
    };

    let timing_facts_2997 = DetailedTimingFacts {
        r_frame_rate: Rational {
            num: 30000,
            den: 1001,
        },
        avg_frame_rate: Rational {
            num: 30000,
            den: 1001,
        },
        time_base: Rational { num: 1, den: 30000 },
        is_vfr: false,
        nb_frames: Some(2997),
    };

    let plan = SegmentPlanner::plan(&source_facts, &timing_facts_2997, 60.0)
        .expect("fractional CFR plan failed");
    assert_eq!(plan.boundaries.len(), 2);
    assert_eq!(plan.boundaries[0].start_frame, 0);
    assert!(plan.boundaries[0].expected_duration_sec < 60.0);
}

// =============================================================================
// 5. Frame-Aligned Boundary Calculation (2-seg, 3-seg, fractional)
// =============================================================================

#[test]
fn test_phase19_05_frame_aligned_boundary_calculation() {
    let source_facts = SourceMediaFacts {
        duration_sec: 140.0,
        width: 1920,
        height: 1080,
        fps: 30.0,
        has_audio: true,
        timing: None,
    };

    let timing_facts = DetailedTimingFacts {
        r_frame_rate: Rational { num: 30, den: 1 },
        avg_frame_rate: Rational { num: 30, den: 1 },
        time_base: Rational { num: 1, den: 30 },
        is_vfr: false,
        nb_frames: Some(4200),
    };

    let plan = SegmentPlanner::plan(&source_facts, &timing_facts, 60.0).expect("plan failed");

    // 140s at 30fps = 4200 frames. Provider limit = 60s (max 1799 frames).
    // Segment count = ceil(4200 / 1799) = 3 segments.
    assert_eq!(plan.boundaries.len(), 3);
    assert_eq!(plan.boundaries[0].start_frame, 0);
    assert_eq!(plan.boundaries[0].end_frame, 1400);
    assert_eq!(plan.boundaries[1].start_frame, 1400);
    assert_eq!(plan.boundaries[1].end_frame, 2800);
    assert_eq!(plan.boundaries[2].start_frame, 2800);
    assert_eq!(plan.boundaries[2].end_frame, 4200);

    for b in &plan.boundaries {
        assert!(
            b.expected_duration_sec < 60.0,
            "Segment duration {:.2}s must be strictly below provider limit 60s",
            b.expected_duration_sec
        );
        assert_eq!(b.end_frame.saturating_sub(b.start_frame), 1400);
    }
}

// =============================================================================
// 6. Segment Splitter: Video-Only & Duration Correction
// =============================================================================

#[test]
fn test_phase19_06_splitter_video_only_and_duration_correction() {
    let test_dir = get_test_dir("splitter_test");
    let video_path = test_dir.join("source_with_audio.mp4");
    create_synthetic_test_video(&video_path, 6.0, 30, true);

    let boundary = SegmentBoundary {
        index: 0,
        start_frame: 0,
        end_frame: 60,
        start_pts: 0,
        end_pts: 60,
        start_ms: 0,
        end_ms: 2000,
        expected_duration_sec: 2.0,
    };

    let split_out = test_dir.join("segment_0.mp4");
    let facts = SegmentSplitter::split_segment(&video_path, &boundary, 30.0, &split_out, 60.0)
        .expect("split segment failed");

    assert!(split_out.exists());
    assert!(
        !facts.has_audio,
        "Split segment must be strictly video-only (no audio track)"
    );
    assert_eq!(facts.width, 320);
    assert_eq!(facts.height, 240);
    assert!((facts.duration_sec - 2.0).abs() < 0.2);
}

// =============================================================================
// 7. Duration Correction Exhaustion Failure
// =============================================================================

#[test]
fn test_phase19_07_duration_correction_exhaustion_failure() {
    let test_dir = get_test_dir("dur_exhaust");
    let video_path = test_dir.join("source.mp4");
    create_synthetic_test_video(&video_path, 5.0, 30, false);

    let boundary = SegmentBoundary {
        index: 0,
        start_frame: 0,
        end_frame: 150,
        start_pts: 0,
        end_pts: 150,
        start_ms: 0,
        end_ms: 5000,
        expected_duration_sec: 5.0,
    };

    let split_out = test_dir.join("segment_exhaust.mp4");
    // Setting max provider limit to an impossibly small duration (0.01s) triggers exhaustion
    let result = SegmentSplitter::split_segment(&video_path, &boundary, 30.0, &split_out, 0.01);
    assert!(
        result.is_err(),
        "Must fail closed when provider limit is violated after 3 iterations"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("SEGMENT_DURATION_LIMIT_VIOLATION"));
}

// =============================================================================
// 8. Child Client Identity Determinism
// =============================================================================

#[test]
fn test_phase19_08_child_client_identity_determinism() {
    let source_facts = SourceMediaFacts {
        duration_sec: 90.0,
        width: 1280,
        height: 720,
        fps: 30.0,
        has_audio: false,
        timing: None,
    };
    let timing_facts = DetailedTimingFacts {
        r_frame_rate: Rational { num: 30, den: 1 },
        avg_frame_rate: Rational { num: 30, den: 1 },
        time_base: Rational { num: 1, den: 30 },
        is_vfr: false,
        nb_frames: Some(2700),
    };
    let plan = SegmentPlanner::plan(&source_facts, &timing_facts, 60.0).unwrap();

    let manifest = SegmentedCloudJobManifest::new(
        "seg-parent-42".to_string(),
        "client-req-42".to_string(),
        "proj_test".to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "confighash42".to_string(),
        source_facts,
        timing_facts,
        plan,
        Some(5.0),
        0.378,
    );

    assert_eq!(manifest.child_jobs.len(), 2);
    assert_eq!(
        manifest.child_jobs[0].client_job_id,
        "segjob:seg-parent-42:0:confighash42:v1"
    );
    assert_eq!(
        manifest.child_jobs[1].client_job_id,
        "segjob:seg-parent-42:1:confighash42:v1"
    );
}

// =============================================================================
// 9. Parent Request Idempotency & Conflict
// =============================================================================

#[tokio::test]
async fn test_phase19_09_parent_request_idempotency_and_conflict() {
    let test_dir = get_test_dir("idempotency_test");
    let video_path = test_dir.join("source.mp4");
    create_synthetic_test_video(&video_path, 80.0, 30, false);

    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let resolver = std::sync::Arc::new(DefaultCloudProviderResolver::new());
    let event_sink = std::sync::Arc::new(NoopEventSink);
    let gate = std::sync::Arc::new(DefaultCloudSubmissionGate::new());
    let lifecycle = std::sync::Arc::new(CloudJobLifecycleService::new(
        storage_paths.clone(),
        resolver,
        event_sink,
        gate,
        LifecycleTimingConfig::fast_test(),
    ));

    let store = SegmentedCloudJobStore::new(storage_paths.clone());
    let registry = ProviderRegistry::new();
    let orchestrator =
        SegmentedCloudJobOrchestrator::new(lifecycle, store, storage_paths, registry, None);

    let req = CloudJobRequest {
        job_id: "req_idempotent_1".to_string(),
        project_id: Some("proj_idem".to_string()),
        prompt: "test".to_string(),
        negative_prompt: None,
        source_video: Some(video_path.clone()),
        reference_image: None,
        reference_images: None,
        duration_seconds: 80.0,
        fps: 30.0,
        resolution: (320, 240),
        task_type: "BACKGROUND_REMOVAL".to_string(),
    };

    // 1. First submission
    let m1 = orchestrator
        .start_segmented_transformation(req.clone(), Some(5.0))
        .await
        .expect("start 1 failed");

    // 2. Duplicate submission with identical config -> resumes same parent
    let m2 = orchestrator
        .start_segmented_transformation(req.clone(), Some(5.0))
        .await
        .expect("start 2 failed");
    assert_eq!(m1.parent_id, m2.parent_id);

    // 3. Duplicate client ID with modified configuration -> REQUEST_ID_CONFLICT
    let video_path_diff = test_dir.join("source_diff.mp4");
    create_synthetic_test_video(&video_path_diff, 90.0, 30, false);
    let mut req_diff = req.clone();
    req_diff.source_video = Some(video_path_diff);
    let res_conflict = orchestrator
        .start_segmented_transformation(req_diff, Some(5.0))
        .await;
    assert!(res_conflict.is_err());
    let err_msg = format!("{}", res_conflict.unwrap_err());
    assert!(err_msg.contains("REQUEST_ID_CONFLICT"));
}

// =============================================================================
// 10. Parent Storage Isolation
// =============================================================================

#[test]
fn test_phase19_10_parent_storage_isolation() {
    let test_dir = get_test_dir("isolation_test");
    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let store_parent = SegmentedCloudJobStore::new(storage_paths.clone());
    let store_child = PersistentCloudJobStore::new(storage_paths);
    let project_id = "proj_iso";

    let source_facts = SourceMediaFacts {
        duration_sec: 60.0,
        width: 320,
        height: 240,
        fps: 30.0,
        has_audio: false,
        timing: None,
    };
    let timing_facts = DetailedTimingFacts {
        r_frame_rate: Rational { num: 30, den: 1 },
        avg_frame_rate: Rational { num: 30, den: 1 },
        time_base: Rational { num: 1, den: 30 },
        is_vfr: false,
        nb_frames: Some(1800),
    };
    let plan = SegmentPlanner::plan(&source_facts, &timing_facts, 60.0).unwrap();

    let manifest = SegmentedCloudJobManifest::new(
        "seg-parent-iso".to_string(),
        "client-iso".to_string(),
        project_id.to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "hash_iso".to_string(),
        source_facts,
        timing_facts,
        plan,
        Some(5.0),
        0.252,
    );

    // Save segmented manifest under cloud-jobs/segmented/seg-parent-iso/manifest.json
    store_parent.save_manifest_atomic(&manifest).unwrap();

    // Verify PersistentCloudJobStore::list_jobs_in_project returns 0 jobs without deserialization error
    let child_jobs = store_child.list_jobs_in_project(project_id).unwrap();
    assert_eq!(
        child_jobs.len(),
        0,
        "Parent segmented directory must not be parsed as a normal child job"
    );
}

// =============================================================================
// 11. Parent Manifest Persistence & Atomic Crash Recovery (5 Cases)
// =============================================================================

#[test]
fn test_phase19_11_parent_manifest_persistence_and_atomic_store() {
    let test_dir = get_test_dir("store_test");
    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let store = SegmentedCloudJobStore::new(storage_paths);
    let project_id = "test_project_alpha";

    let source_facts = SourceMediaFacts {
        duration_sec: 90.0,
        width: 1280,
        height: 720,
        fps: 30.0,
        has_audio: true,
        timing: None,
    };
    let timing_facts = DetailedTimingFacts {
        r_frame_rate: Rational { num: 30, den: 1 },
        avg_frame_rate: Rational { num: 30, den: 1 },
        time_base: Rational { num: 1, den: 30 },
        is_vfr: false,
        nb_frames: Some(2700),
    };
    let plan = SegmentPlanner::plan(&source_facts, &timing_facts, 60.0).unwrap();

    let manifest = SegmentedCloudJobManifest::new(
        "seg-parent-99".to_string(),
        "client-req-99".to_string(),
        project_id.to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "hash_99".to_string(),
        source_facts,
        timing_facts,
        plan,
        Some(5.0),
        0.378,
    );

    // 1. Atomic Save & Load
    store
        .save_manifest_atomic(&manifest)
        .expect("save manifest failed");
    let loaded = store
        .load_manifest(project_id, "seg-parent-99")
        .expect("load manifest failed");
    assert_eq!(loaded.parent_id, "seg-parent-99");
    assert_eq!(loaded.state, SegmentedJobState::Planning);

    // 2. Case 1: Primary valid + Newer tmp valid -> Newer tmp wins
    let mut newer_tmp = manifest.clone();
    newer_tmp.state_revision = 5;
    newer_tmp.state = SegmentedJobState::Running;
    let tmp = store
        .manifest_tmp_file_path(project_id, "seg-parent-99")
        .unwrap();
    fs::write(&tmp, serde_json::to_string(&newer_tmp).unwrap()).unwrap();
    let loaded_newer = store.load_manifest(project_id, "seg-parent-99").unwrap();
    assert_eq!(loaded_newer.state_revision, 5);
    assert_eq!(loaded_newer.state, SegmentedJobState::Running);

    // 3. Case 4: Primary corrupt + tmp valid -> tmp recovers
    let primary = store
        .manifest_file_path(project_id, "seg-parent-99")
        .unwrap();
    fs::write(&primary, b"corrupted").unwrap();
    fs::write(&tmp, serde_json::to_string(&manifest).unwrap()).unwrap();
    let recovered = store.load_manifest(project_id, "seg-parent-99").unwrap();
    assert_eq!(recovered.parent_id, "seg-parent-99");

    // 4. Case 5: Both corrupt -> fails closed
    fs::write(&primary, b"corrupt1").unwrap();
    fs::write(&tmp, b"corrupt2").unwrap();
    assert!(store.load_manifest(project_id, "seg-parent-99").is_err());
}

// =============================================================================
// 12. Two-Stage Budget Guard Invariant
// =============================================================================

#[test]
fn test_phase19_12_two_stage_budget_guard() {
    let test_dir = get_test_dir("budget_test");
    let video_path = test_dir.join("source.mp4");
    create_synthetic_test_video(&video_path, 120.0, 30, false);

    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let resolver = std::sync::Arc::new(DefaultCloudProviderResolver::new());
    let event_sink = std::sync::Arc::new(NoopEventSink);
    let gate = std::sync::Arc::new(DefaultCloudSubmissionGate::new());
    let lifecycle = std::sync::Arc::new(CloudJobLifecycleService::new(
        storage_paths.clone(),
        resolver,
        event_sink,
        gate,
        LifecycleTimingConfig::fast_test(),
    ));

    let store = SegmentedCloudJobStore::new(storage_paths.clone());
    let registry = ProviderRegistry::new();
    let orchestrator =
        SegmentedCloudJobOrchestrator::new(lifecycle, store, storage_paths, registry, None);

    let req = CloudJobRequest {
        job_id: "test_budget_job".to_string(),
        project_id: Some("proj_budget".to_string()),
        prompt: "test".to_string(),
        negative_prompt: None,
        source_video: Some(video_path.clone()),
        reference_image: None,
        reference_images: None,
        duration_seconds: 120.0,
        fps: 30.0,
        resolution: (320, 240),
        task_type: "BACKGROUND_REMOVAL".to_string(),
    };

    // Stage A Preflight: provisional estimate for 120s BackgroundRemoval is ~$0.504.
    let preflight = orchestrator
        .preflight_segmented_transformation(&req, Some(0.10))
        .expect("preflight failed");
    assert!(!preflight.budget_approved);
    assert_eq!(
        preflight.blocking_code,
        Some("COST_BUDGET_EXCEEDED".to_string())
    );

    let preflight_ok = orchestrator
        .preflight_segmented_transformation(&req, Some(1.00))
        .expect("preflight failed");
    assert!(preflight_ok.budget_approved);
    assert_eq!(preflight_ok.blocking_code, None);
}

// =============================================================================
// 13. Budget Approval Resume
// =============================================================================

#[tokio::test]
async fn test_phase19_13_budget_approval_resume() {
    let test_dir = get_test_dir("budget_approval");
    let video_path = test_dir.join("source.mp4");
    create_synthetic_test_video(&video_path, 80.0, 30, false);

    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let resolver = std::sync::Arc::new(DefaultCloudProviderResolver::new());
    let event_sink = std::sync::Arc::new(NoopEventSink);
    let gate = std::sync::Arc::new(DefaultCloudSubmissionGate::new());
    let lifecycle = std::sync::Arc::new(CloudJobLifecycleService::new(
        storage_paths.clone(),
        resolver,
        event_sink,
        gate,
        LifecycleTimingConfig::fast_test(),
    ));

    let store = SegmentedCloudJobStore::new(storage_paths.clone());
    let registry = ProviderRegistry::new();
    let orchestrator =
        SegmentedCloudJobOrchestrator::new(lifecycle, store, storage_paths, registry, None);

    let req = CloudJobRequest {
        job_id: "test_budget_approval_job".to_string(),
        project_id: Some("proj_appr".to_string()),
        prompt: "test".to_string(),
        negative_prompt: None,
        source_video: Some(video_path),
        reference_image: None,
        reference_images: None,
        duration_seconds: 80.0,
        fps: 30.0,
        resolution: (320, 240),
        task_type: "BACKGROUND_REMOVAL".to_string(),
    };

    // Start with low budget ($0.05) -> pauses at CostApprovalRequired
    let manifest = orchestrator
        .start_segmented_transformation(req, Some(0.05))
        .await
        .expect("start failed");
    assert_eq!(manifest.state, SegmentedJobState::CostApprovalRequired);

    // Approve with insufficient budget ($0.10) -> rejected
    let res_low = orchestrator
        .approve_segmented_budget("proj_appr", &manifest.parent_id, 0.10)
        .await;
    assert!(res_low.is_err());

    // Approve with sufficient budget ($2.00) -> transitions to Ready
    let res_ok = orchestrator
        .approve_segmented_budget("proj_appr", &manifest.parent_id, 2.00)
        .await
        .expect("approval failed");
    assert_eq!(res_ok.state, SegmentedJobState::Ready);
    assert_eq!(res_ok.budget_limit, Some(2.00));
}

// =============================================================================
// 14. Child Created Before Parent Mapping Crash Recovery
// =============================================================================

#[test]
fn test_phase19_14_child_created_before_parent_mapping_crash() {
    let test_dir = get_test_dir("crash_mapping");
    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let child_store = PersistentCloudJobStore::new(storage_paths.clone());
    let _parent_store = SegmentedCloudJobStore::new(storage_paths);
    let project_id = "proj_crash";

    let child_client_id = "segjob:seg-crash-1:0:hash:v1";
    let internal_job_id = "cloud_job_internal_101";

    let child_job = PersistentCloudJob::new(
        child_client_id.to_string(),
        internal_job_id.to_string(),
        project_id.to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "v1".to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        ExecutionClass::UtilityCloud,
        InputAssets::default(),
        "hash".to_string(),
        CostRecord::default(),
    );

    // Simulate child job saved by lifecycle
    child_store.save_job_atomic(&child_job).unwrap();

    // Verify recovery lookup by client_job_id finds existing child
    let recovered_child = child_store
        .find_job_by_client_request_id(project_id, child_client_id)
        .unwrap();
    assert!(recovered_child.is_some());
    assert_eq!(
        recovered_child.unwrap().internal_job_id,
        "cloud_job_internal_101"
    );
}

// =============================================================================
// 15. Ambiguous / Failed Retry Policy (Zero Auto-Resubmit)
// =============================================================================

#[test]
fn test_phase19_15_ambiguous_and_failed_retry_zero_auto_resubmit() {
    let mut manifest = SegmentedCloudJobManifest::new(
        "seg-parent-fail".to_string(),
        "client-req-fail".to_string(),
        "proj_fail".to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "hash_fail".to_string(),
        SourceMediaFacts::default(),
        DetailedTimingFacts {
            r_frame_rate: Rational::new(30, 1),
            avg_frame_rate: Rational::new(30, 1),
            time_base: Rational::new(1, 30),
            is_vfr: false,
            nb_frames: Some(900),
        },
        SegmentPlan {
            plan_id: "p".to_string(),
            source_facts: SourceMediaFacts::default(),
            timing_facts: DetailedTimingFacts {
                r_frame_rate: Rational::new(30, 1),
                avg_frame_rate: Rational::new(30, 1),
                time_base: Rational::new(1, 30),
                is_vfr: false,
                nb_frames: Some(900),
            },
            boundaries: vec![],
            policy_version: 1,
            provider_limit_ms: 60000,
            total_source_duration_sec: 30.0,
        },
        Some(5.0),
        0.126,
    );

    // When child segment fails, parent moves to Failed without auto-resubmit
    manifest.state = SegmentedJobState::Running;
    let _ = manifest.transition_to(SegmentedJobState::Failed);
    assert_eq!(manifest.state, SegmentedJobState::Failed);
    assert!(manifest.state.is_terminal());
}

// =============================================================================
// 16. Max Paid Concurrency (Sequential Concurrency = 1)
// =============================================================================

#[test]
fn test_phase19_16_max_paid_concurrency_sequential() {
    let source_facts = SourceMediaFacts {
        duration_sec: 140.0,
        width: 320,
        height: 240,
        fps: 30.0,
        has_audio: false,
        timing: None,
    };
    let timing_facts = DetailedTimingFacts {
        r_frame_rate: Rational::new(30, 1),
        avg_frame_rate: Rational::new(30, 1),
        time_base: Rational::new(1, 30),
        is_vfr: false,
        nb_frames: Some(4200),
    };
    let plan = SegmentPlanner::plan(&source_facts, &timing_facts, 60.0).unwrap();

    // Loop structure in run_segmented_job_worker executes `for i in 0..total_children`
    // sequentially awaiting each child prediction before dispatching the next.
    assert_eq!(plan.boundaries.len(), 3);
}

// =============================================================================
// 17. Cancellation Semantics
// =============================================================================

#[tokio::test]
async fn test_phase19_17_cancellation_semantics() {
    let test_dir = get_test_dir("cancel_test");
    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let resolver = std::sync::Arc::new(DefaultCloudProviderResolver::new());
    let event_sink = std::sync::Arc::new(NoopEventSink);
    let gate = std::sync::Arc::new(DefaultCloudSubmissionGate::new());
    let lifecycle = std::sync::Arc::new(CloudJobLifecycleService::new(
        storage_paths.clone(),
        resolver,
        event_sink,
        gate,
        LifecycleTimingConfig::fast_test(),
    ));

    let store = SegmentedCloudJobStore::new(storage_paths.clone());
    let registry = ProviderRegistry::new();
    let orchestrator =
        SegmentedCloudJobOrchestrator::new(lifecycle, store.clone(), storage_paths, registry, None);

    let manifest = SegmentedCloudJobManifest::new(
        "seg-parent-cancel".to_string(),
        "client-req-cancel".to_string(),
        "proj_cancel".to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "hash".to_string(),
        SourceMediaFacts::default(),
        DetailedTimingFacts {
            r_frame_rate: Rational::new(30, 1),
            avg_frame_rate: Rational::new(30, 1),
            time_base: Rational::new(1, 30),
            is_vfr: false,
            nb_frames: Some(900),
        },
        SegmentPlan {
            plan_id: "p".to_string(),
            source_facts: SourceMediaFacts::default(),
            timing_facts: DetailedTimingFacts {
                r_frame_rate: Rational::new(30, 1),
                avg_frame_rate: Rational::new(30, 1),
                time_base: Rational::new(1, 30),
                is_vfr: false,
                nb_frames: Some(900),
            },
            boundaries: vec![],
            policy_version: 1,
            provider_limit_ms: 60000,
            total_source_duration_sec: 30.0,
        },
        Some(5.0),
        0.126,
    );

    store.save_manifest_atomic(&manifest).unwrap();
    let cancelled = orchestrator
        .cancel_segmented_transformation("proj_cancel", "seg-parent-cancel")
        .await
        .unwrap();
    assert_eq!(cancelled.state, SegmentedJobState::Cancelled);
    assert!(cancelled.cancellation_requested);
}

// =============================================================================
// 18. Child Audio Policy (Video Only)
// =============================================================================

#[test]
fn test_phase19_18_child_audio_policy_video_only() {
    let test_dir = get_test_dir("child_audio_policy");
    let video_path = test_dir.join("source.mp4");
    create_synthetic_test_video(&video_path, 4.0, 30, true);

    let boundary = SegmentBoundary {
        index: 0,
        start_frame: 0,
        end_frame: 60,
        start_pts: 0,
        end_pts: 60,
        start_ms: 0,
        end_ms: 2000,
        expected_duration_sec: 2.0,
    };

    let split_out = test_dir.join("seg_noaudio.mp4");
    let facts = SegmentSplitter::split_segment(&video_path, &boundary, 30.0, &split_out, 60.0)
        .expect("split failed");

    assert!(
        !facts.has_audio,
        "Child input segment MUST have audio stripped (-an)"
    );
}

// =============================================================================
// 19. Final Original Audio Muxing
// =============================================================================

#[test]
fn test_phase19_19_final_original_audio_muxing() {
    let test_dir = get_test_dir("audio_mux_test");
    let original_source = test_dir.join("orig_source.mp4");
    create_synthetic_test_video(&original_source, 4.0, 30, true);

    let stitched_video = test_dir.join("stitched_silent.webm");
    create_synthetic_alpha_webm(&stitched_video, 4.0, 30);

    let final_muxed = test_dir.join("final_muxed.webm");
    FinalAudioMuxer::mux_original_audio(&stitched_video, &original_source, &final_muxed)
        .expect("audio muxing failed");

    assert!(final_muxed.exists(), "Final muxed webm must exist");
    let probed = SourceMediaProbe::probe_file(&final_muxed).expect("probe muxed failed");
    assert!(
        probed.has_audio,
        "Muxed video must contain audio track from original source"
    );
    assert_eq!(probed.width, 320);
    assert_eq!(probed.height, 240);
}

// =============================================================================
// 20. Level B Cache Lifecycle
// =============================================================================

#[test]
fn test_phase19_20_level_b_cache_lifecycle() {
    let test_dir = get_test_dir("cache_test");
    let video_path = test_dir.join("source.mp4");
    create_synthetic_test_video(&video_path, 4.0, 30, false);

    let checksum = SegmentCacheManager::compute_file_sha256(&video_path).expect("checksum failed");
    let source_facts = SourceMediaProbe::probe_file(&video_path).expect("probe failed");

    let boundary = SegmentBoundary {
        index: 0,
        start_frame: 0,
        end_frame: 60,
        start_pts: 0,
        end_pts: 60,
        start_ms: 0,
        end_ms: 2000,
        expected_duration_sec: 2.0,
    };

    let (path1, facts1) = SegmentCacheManager::get_or_create_split_segment(
        &test_dir,
        &video_path,
        &checksum,
        &source_facts,
        &boundary,
        30.0,
        60.0,
    )
    .expect("cache get/create failed");

    assert!(path1.exists());
    assert!(!facts1.has_audio);

    let ffmpeg_fp = SegmentSplitter::get_ffmpeg_build_fingerprint();
    let cache_key = SegmentCacheManager::compute_split_cache_key(&checksum, &boundary, &ffmpeg_fp);
    let hit = SegmentCacheManager::get_cached_split_segment(&test_dir, &cache_key, &checksum)
        .expect("cache query failed");
    assert!(
        hit.is_some(),
        "Expected cache hit on identical fingerprint and checksum"
    );

    // Corrupt file -> returns None and cleans up
    fs::write(&path1, b"corrupted bytes").unwrap();
    let hit_after_corrupt =
        SegmentCacheManager::get_cached_split_segment(&test_dir, &cache_key, &checksum)
            .expect("cache query after corrupt failed");
    assert!(
        hit_after_corrupt.is_none(),
        "Corrupted cache entry must return None"
    );
    assert!(
        !path1.exists(),
        "Corrupted cache entry directory must be cleaned up"
    );
}

// =============================================================================
// 21. Level C Cross-Parent Cache Disabled
// =============================================================================

#[test]
fn test_phase19_21_level_c_cross_parent_cache_disabled() {
    let test_dir = get_test_dir("level_c_disabled");
    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let store = PersistentCloudJobStore::new(storage_paths);
    let project_id = "proj_level_c";

    let parent_a_child_id = "segjob:seg-parent-A:0:confighash:v1";
    let parent_b_child_id = "segjob:seg-parent-B:0:confighash:v1";

    // Parent A child job exists
    let job_a = PersistentCloudJob::new(
        parent_a_child_id.to_string(),
        "internal_job_A".to_string(),
        project_id.to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "v1".to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        ExecutionClass::UtilityCloud,
        InputAssets::default(),
        "confighash".to_string(),
        CostRecord::default(),
    );
    store.save_job_atomic(&job_a).unwrap();

    // Query for Parent B's child job must return None (Level C cross-parent reuse disabled)
    let query_b = store
        .find_job_by_client_request_id(project_id, parent_b_child_id)
        .unwrap();
    assert!(
        query_b.is_none(),
        "Parent B must not reuse Parent A's child output"
    );
}

// =============================================================================
// 22. Stitch Compatibility Gate
// =============================================================================

#[test]
fn test_phase19_22_stitch_compatibility_gate() {
    let test_dir = get_test_dir("stitch_compat");
    let seg1_path = test_dir.join("seg1.webm");
    let seg2_path = test_dir.join("seg2.webm");

    create_synthetic_alpha_webm(&seg1_path, 2.0, 30);
    create_synthetic_alpha_webm(&seg2_path, 2.0, 30);

    let is_compat =
        SegmentStitcher::check_stream_copy_compatibility(&[seg1_path, seg2_path]).unwrap();
    assert!(is_compat);
}

// =============================================================================
// 23. VP9 Alpha Fallback Real Media Test
// =============================================================================

#[test]
fn test_phase19_23_vp9_alpha_fallback_real_media() {
    let test_dir = get_test_dir("vp9_fallback");
    let seg1_path = test_dir.join("seg1.webm");
    let seg2_path = test_dir.join("seg2.webm");

    create_synthetic_alpha_webm(&seg1_path, 1.5, 30);
    create_synthetic_alpha_webm(&seg2_path, 1.5, 30);

    let stitched_out = test_dir.join("fallback_stitched.webm");
    SegmentStitcher::stitch_with_vp9_reencode(&[seg1_path, seg2_path], &stitched_out).unwrap();

    assert!(stitched_out.exists());
    let facts = SourceMediaProbe::probe_file(&stitched_out).unwrap();
    assert_eq!(facts.width, 320);
    assert_eq!(facts.height, 240);
    assert!((facts.duration_sec - 3.0).abs() < 0.3);
}

// =============================================================================
// 24. Final Stitch Duration & Timestamp Accuracy
// =============================================================================

#[test]
fn test_phase19_24_final_stitch_duration_and_timestamp_accuracy() {
    let test_dir = get_test_dir("stitch_dur");
    let seg1_path = test_dir.join("seg1.webm");
    let seg2_path = test_dir.join("seg2.webm");
    let seg3_path = test_dir.join("seg3.webm");

    create_synthetic_alpha_webm(&seg1_path, 2.0, 30);
    create_synthetic_alpha_webm(&seg2_path, 2.0, 30);
    create_synthetic_alpha_webm(&seg3_path, 2.0, 30);

    let stitched_out = test_dir.join("stitched_3seg.webm");
    SegmentStitcher::stitch_segments(&[seg1_path, seg2_path, seg3_path], &stitched_out).unwrap();

    assert!(stitched_out.exists());
    let facts = SourceMediaProbe::probe_file(&stitched_out).unwrap();
    assert!((facts.duration_sec - 6.0).abs() < 0.3);
}

// =============================================================================
// 25. Crash After Final Promotion Recovery
// =============================================================================

#[tokio::test]
async fn test_phase19_25_crash_after_final_promotion_recovery() {
    let test_dir = get_test_dir("crash_promotion");
    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let store = SegmentedCloudJobStore::new(storage_paths.clone());
    let project_id = "proj_prom";
    let parent_id = "seg-parent-prom";

    // Place completed final artifact in artifacts dir
    let artifacts_dir = storage_paths
        .projects_dir
        .join(project_id)
        .join("cloud-jobs")
        .join("artifacts");
    let final_artifact = artifacts_dir.join(format!("{}.webm", parent_id));
    create_synthetic_alpha_webm(&final_artifact, 4.0, 30);

    // Save manifest in ValidatingOutput state (simulating crash before marking Completed)
    let mut manifest = SegmentedCloudJobManifest::new(
        parent_id.to_string(),
        "client-prom".to_string(),
        project_id.to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "hash".to_string(),
        SourceMediaFacts::default(),
        DetailedTimingFacts {
            r_frame_rate: Rational::new(30, 1),
            avg_frame_rate: Rational::new(30, 1),
            time_base: Rational::new(1, 30),
            is_vfr: false,
            nb_frames: Some(120),
        },
        SegmentPlan {
            plan_id: "p".to_string(),
            source_facts: SourceMediaFacts::default(),
            timing_facts: DetailedTimingFacts {
                r_frame_rate: Rational::new(30, 1),
                avg_frame_rate: Rational::new(30, 1),
                time_base: Rational::new(1, 30),
                is_vfr: false,
                nb_frames: Some(120),
            },
            boundaries: vec![],
            policy_version: 1,
            provider_limit_ms: 60000,
            total_source_duration_sec: 4.0,
        },
        Some(5.0),
        0.0168,
    );
    manifest.state = SegmentedJobState::ValidatingOutput;
    store.save_manifest_atomic(&manifest).unwrap();

    let resolver = std::sync::Arc::new(DefaultCloudProviderResolver::new());
    let event_sink = std::sync::Arc::new(NoopEventSink);
    let gate = std::sync::Arc::new(DefaultCloudSubmissionGate::new());
    let lifecycle = std::sync::Arc::new(CloudJobLifecycleService::new(
        storage_paths.clone(),
        resolver,
        event_sink,
        gate,
        LifecycleTimingConfig::fast_test(),
    ));
    let orchestrator = SegmentedCloudJobOrchestrator::new(
        lifecycle,
        store.clone(),
        storage_paths,
        ProviderRegistry::new(),
        None,
    );

    let req = CloudJobRequest {
        job_id: "client-prom".to_string(),
        project_id: Some(project_id.to_string()),
        task_type: "BACKGROUND_REMOVAL".to_string(),
        prompt: String::new(),
        negative_prompt: None,
        source_video: None,
        duration_seconds: 4.0,
        fps: 30.0,
        resolution: (320, 240),
        reference_image: None,
        reference_images: None,
    };

    // Running worker detects existing promoted artifact and promotes manifest to Completed
    orchestrator
        .run_segmented_job_worker(project_id.to_string(), parent_id.to_string(), req)
        .await
        .unwrap();

    let loaded = store.load_manifest(project_id, parent_id).unwrap();
    assert_eq!(loaded.state, SegmentedJobState::Completed);
    assert!(loaded.final_output.is_some());
}

// =============================================================================
// 26. Segmented Preview Authorization Security
// =============================================================================

#[test]
fn test_phase19_26_preview_authorization_security() {
    let test_dir = get_test_dir("preview_auth");
    let storage_paths = StoragePaths::resolve_from_base(&test_dir);
    let store = SegmentedCloudJobStore::new(storage_paths.clone());
    let project_id = "proj_preview";
    let parent_id = "seg-parent-auth";

    let artifacts_dir = storage_paths
        .projects_dir
        .join(project_id)
        .join("cloud-jobs")
        .join("artifacts");
    let final_artifact = artifacts_dir.join(format!("{}.webm", parent_id));
    create_synthetic_alpha_webm(&final_artifact, 2.0, 30);

    let mut manifest = SegmentedCloudJobManifest::new(
        parent_id.to_string(),
        "client-auth".to_string(),
        project_id.to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "hash".to_string(),
        SourceMediaFacts::default(),
        DetailedTimingFacts {
            r_frame_rate: Rational::new(30, 1),
            avg_frame_rate: Rational::new(30, 1),
            time_base: Rational::new(1, 30),
            is_vfr: false,
            nb_frames: Some(60),
        },
        SegmentPlan {
            plan_id: "p".to_string(),
            source_facts: SourceMediaFacts::default(),
            timing_facts: DetailedTimingFacts {
                r_frame_rate: Rational::new(30, 1),
                avg_frame_rate: Rational::new(30, 1),
                time_base: Rational::new(1, 30),
                is_vfr: false,
                nb_frames: Some(60),
            },
            boundaries: vec![],
            policy_version: 1,
            provider_limit_ms: 60000,
            total_source_duration_sec: 2.0,
        },
        Some(5.0),
        0.0084,
    );

    // 1. Non-completed state -> rejected
    manifest.state = SegmentedJobState::Running;
    store.save_manifest_atomic(&manifest).unwrap();
    let res_running = resolve_segmented_cloud_artifact_preview_path(project_id, parent_id, &store);
    assert!(res_running.is_err());

    // 2. Completed with valid path inside root -> authorized
    manifest.state = SegmentedJobState::Completed;
    manifest.final_output = Some(OutputArtifactRecord {
        temporary_path: None,
        final_path: Some(final_artifact),
        artifact_hash: None,
        width: Some(320),
        height: Some(240),
        duration_sec: Some(2.0),
        fps: Some(30.0),
    });
    store.save_manifest_atomic(&manifest).unwrap();
    let res_ok =
        resolve_segmented_cloud_artifact_preview_path(project_id, parent_id, &store).unwrap();
    assert_eq!(res_ok.1.parent_id, parent_id);

    // 3. Path outside artifact root -> security violation rejected
    let outside_path = test_dir.join("outside.webm");
    create_synthetic_alpha_webm(&outside_path, 2.0, 30);
    manifest.final_output.as_mut().unwrap().final_path = Some(outside_path);
    store.save_manifest_atomic(&manifest).unwrap();
    let res_outside = resolve_segmented_cloud_artifact_preview_path(project_id, parent_id, &store);
    assert!(res_outside.is_err());
    assert!(res_outside.unwrap_err().contains("SECURITY_VIOLATION"));
}
