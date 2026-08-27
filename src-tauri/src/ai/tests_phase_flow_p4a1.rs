use super::flow::manifest::{
    FlowCanonicalGeometry, FlowFaceContinuityStatus, FlowFinalAudioPolicy, FlowGenerationManifest,
    FlowIdentityContinuityStrategy, FlowJobKind, FlowJobState, FlowLongVideoPlan,
    FlowNormalizedSegment, FlowPlannedSegment, FlowRationalFrameRate,
    FlowRequestedGenerationConfig, FlowSeamStatus, FlowSegmentPlan,
};
use super::flow::orchestrator::{FlowGenerationRequest, FlowPreflightTicket, FlowRuntimeService};
use super::flow::{FlowContinuityManager, FlowStitcher, FlowVideoNormalizer, FlowVideoSegmenter};
use crate::ai::transformation::{IdentityMode, TransformationIntent};
use crate::projects::ProjectManager;
use crate::system::StoragePaths;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn create_synthetic_test_video_frames(path: &Path, frames: u64, fps: f64, width: u32, height: u32) {
    if let Some(p) = path.parent() {
        let _ = fs::create_dir_all(p);
    }
    let dur = (frames as f64) / fps;
    let _ = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!(
                "testsrc=duration={:.4}:size={}x{}:rate={:.4}",
                dur, width, height, fps
            ),
            "-vframes",
            &frames.to_string(),
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-an",
            path.to_str().unwrap_or_default(),
        ])
        .output()
        .expect("create test video frames");
}

fn create_synthetic_testsrc_image(
    path: &Path,
    filter: &str,
    width: u32,
    height: u32,
    q_scale: u32,
) {
    if let Some(p) = path.parent() {
        let _ = fs::create_dir_all(p);
    }
    let _ = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("{}=size={}x{}:d=1", filter, width, height),
            "-vframes",
            "1",
            "-q:v",
            &q_scale.to_string(),
            "-pix_fmt",
            "yuvj420p",
            "-strict",
            "unofficial",
            path.to_str().unwrap_or_default(),
        ])
        .output()
        .expect("create synthetic testsrc image");
}

fn create_synthetic_image(path: &Path, color: &str, width: u32, height: u32, q_scale: u32) {
    if let Some(p) = path.parent() {
        let _ = fs::create_dir_all(p);
    }
    let _ = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("color=c={}:s={}x{}:d=1", color, width, height),
            "-vframes",
            "1",
            "-q:v",
            &q_scale.to_string(),
            "-pix_fmt",
            "yuvj420p",
            "-strict",
            "unofficial",
            path.to_str().unwrap_or_default(),
        ])
        .output()
        .expect("create synthetic image");
}

// -----------------------------------------------------------------------------
// 1. Decoded Pixel Metric Correction (Section 29)
// -----------------------------------------------------------------------------

#[test]
fn test_flow_p4a1_01_decoded_pixel_metric_distinguishes_content_from_compression_artifacts() {
    let temp_dir = tempdir().unwrap();

    // Create same smptebars image with very different JPEG qualities (Q2 vs Q31)
    let img_bars_q2 = temp_dir.path().join("bars_q2.jpg");
    let img_bars_q31 = temp_dir.path().join("bars_q31.jpg");
    create_synthetic_testsrc_image(&img_bars_q2, "smptebars", 256, 256, 2);
    create_synthetic_testsrc_image(&img_bars_q31, "smptebars", 256, 256, 31);

    // Compressed byte differences are substantial:
    let bytes_q2 = fs::read(&img_bars_q2).unwrap();
    let bytes_q31 = fs::read(&img_bars_q31).unwrap();
    assert_ne!(bytes_q2.len(), bytes_q31.len());

    // But true decoded pixel difference between the same image is near 0.0:
    let delta_same_content =
        FlowContinuityManager::compute_decoded_pixel_delta(&img_bars_q2, &img_bars_q31)
            .expect("compute delta same content");
    assert!(
        delta_same_content < 0.05,
        "Decoded pixel difference for same image across JPEG qualities must be near 0, got {:.4}",
        delta_same_content
    );

    // Now compare with a completely different image (blue):
    let img_blue = temp_dir.path().join("blue.jpg");
    create_synthetic_image(&img_blue, "blue", 256, 256, 2);

    let delta_different_content =
        FlowContinuityManager::compute_decoded_pixel_delta(&img_bars_q2, &img_blue)
            .expect("compute delta different content");
    assert!(
        delta_different_content > 0.25,
        "Decoded pixel difference for different content must be substantial, got {:.4}",
        delta_different_content
    );
    assert!(
        delta_different_content > delta_same_content * 5.0,
        "Different content delta must be much larger than compression delta"
    );

    // Verify boundary extraction assigns VISUAL_SEAM_METRIC and UNVERIFIED seam status
    let evidence_dir = temp_dir.path().join("evidence");
    let v0 = temp_dir.path().join("v0.mp4");
    let v1 = temp_dir.path().join("v1.mp4");
    create_synthetic_test_video_frames(&v0, 60, 30.0, 256, 256);
    create_synthetic_test_video_frames(&v1, 60, 30.0, 256, 256);

    let ev = FlowContinuityManager::extract_boundary_evidence(0, &v0, 0, &v1, 1, &evidence_dir)
        .expect("extract evidence");

    assert_eq!(ev.metric_name, Some("mean_pixel_delta".to_string()));
    assert_eq!(ev.metric_category, Some("VISUAL_SEAM_METRIC".to_string()));
    assert_eq!(
        ev.face_continuity_status,
        FlowFaceContinuityStatus::Unverified
    );
    assert_eq!(
        ev.seam_status,
        FlowSeamStatus::Unverified,
        "Uncalibrated threshold must NOT assign PASS/FAIL"
    );
    assert!(ev.contact_sheet_path.is_some());
    assert!(ev.contact_sheet_path.unwrap().exists());
}

// -----------------------------------------------------------------------------
// 2. Rational FPS End-to-End Type & Math (Section 30)
// -----------------------------------------------------------------------------

#[test]
fn test_flow_p4a1_02_rational_fps_30000_1001_exact_math_and_args() {
    let r_fps = FlowRationalFrameRate::new(30000, 1001);
    assert_eq!(r_fps.numerator, 30000);
    assert_eq!(r_fps.denominator, 1001);
    assert_eq!(r_fps.to_ffmpeg_arg(), "30000/1001");

    // Exact rational duration calculation
    let dur_299 = r_fps.expected_duration_sec(299);
    assert!(
        dur_299 <= 10.0000001,
        "299 frames at 30000/1001 must be <= 10.0s, was {:.7}",
        dur_299
    );

    let dur_300 = r_fps.expected_duration_sec(300);
    assert!(
        dur_300 > 10.0,
        "300 frames at 30000/1001 is {:.4}s and MUST be rejected as > 10.0s",
        dur_300
    );

    // Plan long video on 29.97 fps synthetic media
    let temp_dir = tempdir().unwrap();
    let video_path = temp_dir.path().join("ntsc_source.mp4");
    create_synthetic_test_video_frames(&video_path, 600, 29.97002997, 576, 1024);

    let plan = FlowVideoSegmenter::plan_long_video(
        "parent_ntsc",
        "proj_ntsc",
        None,
        &video_path,
        temp_dir.path(),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        FlowRequestedGenerationConfig::default(),
        "prompt",
        "hash",
        10.0,
    )
    .expect("plan ntsc");

    for seg in &plan.segments {
        assert!(
            seg.planned_frame_count <= 299,
            "No segment under NTSC 30000/1001 may have > 299 frames"
        );
        let seg_dur = r_fps.expected_duration_sec(seg.planned_frame_count);
        assert!(
            seg_dur <= 10.000001,
            "Segment duration {:.5}s must be <= 10.0s",
            seg_dur
        );
    }
}

// -----------------------------------------------------------------------------
// 3. Two-Pass Child Normalization (Section 31)
// -----------------------------------------------------------------------------

#[test]
fn test_flow_p4a1_03_two_pass_normalization_exact_frame_counts() {
    let temp_dir = tempdir().unwrap();

    let planned_seg = FlowPlannedSegment {
        segment_index: 0,
        start_frame: 0,
        end_frame: 300,
        start_ms: 0,
        end_ms: 10000,
        planned_duration_sec: 10.0,
        planned_frame_count: 300,
        source_segment_path: PathBuf::new(),
        source_segment_sha256: String::new(),
        child_job_id: None,
        state: FlowJobState::Completed,
    };

    let geom = FlowCanonicalGeometry {
        width: 576,
        height: 1024,
        orientation: "PORTRAIT".to_string(),
        sar: "1:1".to_string(),
    };

    let fps = FlowRationalFrameRate::new(30, 1);

    // Case A: Short by 1 frame (299 frames) -> clone-padded to exact 300
    let raw_short_1 = temp_dir.path().join("child_short_1.mp4");
    create_synthetic_test_video_frames(&raw_short_1, 299, 30.0, 576, 1024);
    let norm_short_1 = temp_dir.path().join("norm_short_1.mp4");
    let probe_short_1 = FlowVideoNormalizer::normalize_child_segment(
        &raw_short_1,
        &planned_seg,
        &geom,
        fps,
        &norm_short_1,
    )
    .expect("normalize short by 1");
    let frames_1 = probe_short_1
        .timing
        .and_then(|t| t.nb_frames)
        .unwrap_or_else(|| (probe_short_1.duration_sec * 30.0).round() as u64);
    assert_eq!(frames_1, 300);

    // Case B: Long by 1 frame (301 frames) -> trimmed to exact 300
    let raw_long_1 = temp_dir.path().join("child_long_1.mp4");
    create_synthetic_test_video_frames(&raw_long_1, 301, 30.0, 576, 1024);
    let norm_long_1 = temp_dir.path().join("norm_long_1.mp4");
    let probe_long_1 = FlowVideoNormalizer::normalize_child_segment(
        &raw_long_1,
        &planned_seg,
        &geom,
        fps,
        &norm_long_1,
    )
    .expect("normalize long by 1");
    let frames_2 = probe_long_1
        .timing
        .and_then(|t| t.nb_frames)
        .unwrap_or_else(|| (probe_long_1.duration_sec * 30.0).round() as u64);
    assert_eq!(frames_2, 300);

    // Case C: Excessive drift (> 2 frames, e.g. 3 frames short) -> strictly fails
    let raw_short_3 = temp_dir.path().join("child_short_3.mp4");
    create_synthetic_test_video_frames(&raw_short_3, 297, 30.0, 576, 1024);
    let norm_short_3 = temp_dir.path().join("norm_short_3.mp4");
    let err_3 = FlowVideoNormalizer::normalize_child_segment(
        &raw_short_3,
        &planned_seg,
        &geom,
        fps,
        &norm_short_3,
    );
    assert!(err_3.is_err());
    assert!(err_3
        .unwrap_err()
        .contains("FLOW_CHILD_DURATION_DRIFT_EXCEEDED"));

    // Verify temp pass1 files are cleaned up
    assert!(!temp_dir.path().join("norm_short_1.pass1.mp4").exists());
    assert!(!temp_dir.path().join("norm_long_1.pass1.mp4").exists());
}

// -----------------------------------------------------------------------------
// 4. Explicit Stitch Order Input & Validation (Section 32)
// -----------------------------------------------------------------------------

#[test]
fn test_flow_p4a1_04_stitch_order_sorting_and_validation() {
    let temp_dir = tempdir().unwrap();
    let seg0 = temp_dir.path().join("part_0.mp4");
    let seg1 = temp_dir.path().join("part_1.mp4");
    let seg2 = temp_dir.path().join("part_2.mp4");

    create_synthetic_test_video_frames(&seg0, 60, 30.0, 576, 1024);
    create_synthetic_test_video_frames(&seg1, 60, 30.0, 576, 1024);
    create_synthetic_test_video_frames(&seg2, 60, 30.0, 576, 1024);

    let fps = FlowRationalFrameRate::new(30, 1);

    // Pass in deliberately scrambled order: [2, 0, 1]
    let scrambled = vec![
        FlowNormalizedSegment::from_path(2, seg2.clone()),
        FlowNormalizedSegment::from_path(0, seg0.clone()),
        FlowNormalizedSegment::from_path(1, seg1.clone()),
    ];

    let out_valid = temp_dir.path().join("out_sorted.mp4");
    let (rec, _) = FlowStitcher::stitch_long_video_timeline(&scrambled, None, 180, fps, &out_valid)
        .expect("stitch scrambled segments");
    assert_eq!(rec.frame_count, 180);

    // Duplicate index test: [0, 0, 1]
    let duplicate = vec![
        FlowNormalizedSegment::from_path(0, seg0.clone()),
        FlowNormalizedSegment::from_path(0, seg1.clone()),
    ];
    let out_dup = temp_dir.path().join("out_dup.mp4");
    let err_dup = FlowStitcher::stitch_long_video_timeline(&duplicate, None, 120, fps, &out_dup);
    assert!(err_dup.is_err());
    assert!(err_dup.unwrap_err().contains("STITCH_ORDER_DUPLICATE"));

    // Gap index test: [0, 2]
    let gap = vec![
        FlowNormalizedSegment::from_path(0, seg0),
        FlowNormalizedSegment::from_path(2, seg2),
    ];
    let out_gap = temp_dir.path().join("out_gap.mp4");
    let err_gap = FlowStitcher::stitch_long_video_timeline(&gap, None, 120, fps, &out_gap);
    assert!(err_gap.is_err());
    assert!(err_gap.unwrap_err().contains("STITCH_ORDER_GAP"));
}

// -----------------------------------------------------------------------------
// 5. Canonical Runtime Entrypoint for Long Video (Section 33, 36, 37)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_flow_p4a1_05_canonical_runtime_long_video_parent() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let service = FlowRuntimeService::new(paths.clone());

    let proj_manager = ProjectManager::new(paths.clone());
    let project = proj_manager
        .create_project("Long Runtime Test Project")
        .unwrap();

    let media_dir = paths.projects_dir.join(&project.id).join("media");
    fs::create_dir_all(&media_dir).unwrap();
    let video_15s = media_dir.join("source_15s.mp4");
    create_synthetic_test_video_frames(&video_15s, 450, 30.0, 576, 1024);

    let profile_dir = paths
        .app_data_dir
        .join("flow_profiles")
        .join("profile_mock");
    fs::create_dir_all(&profile_dir).unwrap();

    let intent = TransformationIntent::FaceReplace;
    let identity_mode = IdentityMode::Generated;
    let requested_config = FlowRequestedGenerationConfig::default();
    let prompt_text = "A continuous long video test";
    let prompt_hash = super::flow::prompt_optimizer::calculate_prompt_hash(prompt_text);
    let fp = super::flow::orchestrator::compute_configuration_fingerprint(
        "profile_mock",
        "media_15s",
        &prompt_hash,
        intent,
        identity_mode,
        &requested_config,
    );

    // Issue preflight ticket
    let preflight_ticket = FlowPreflightTicket {
        preflight_id: "pre_runtime_15s".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "profile_mock".to_string(),
        project_id: project.id.clone(),
        source_media_id: "media_15s".to_string(),
        prompt_hash: prompt_hash.clone(),
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20),
        cost_provenance: super::flow::orchestrator::FlowCostProvenance::UploadedVideoEdit,
        checked_at: chrono::Utc::now().to_rfc3339(),
        expires_at: (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339(),
        ready_for_paid_submission: true,
    };
    service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(preflight_ticket);

    let req = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: "media_15s".to_string(),
        profile_id: "profile_mock".to_string(),
        transformation_intent: Some(intent),
        identity_mode: Some(identity_mode),
        prompt: prompt_text.to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: Some(40), // 2 segments * 20 = 40
        preserve_original_audio: Some(false),
        requested_config: Some(requested_config),
        configuration_fingerprint: Some(fp),
        preflight_id: Some("pre_runtime_15s".to_string()),
    };

    // Call canonical runtime entrypoint!
    let snapshot = service
        .start_flow_generation(req, video_15s.clone())
        .await
        .expect("start flow generation");

    // Must automatically select LongVideoParent
    assert_eq!(snapshot.job_kind, Some(FlowJobKind::LongVideoParent));

    // Wait briefly for sequential worker to process mock segments
    let mut finished = false;
    for _ in 0..30 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if let Ok(st) = service.get_flow_job_status(&project.id, &snapshot.parent_id) {
            if st.state == FlowJobState::Completed || st.state == FlowJobState::Failed {
                finished = true;
                let m = service
                    .orchestrator
                    .store()
                    .load_manifest(&project.id, &snapshot.parent_id)
                    .ok();
                assert_eq!(
                    st.state,
                    FlowJobState::Completed,
                    "Failure reason: {:?}",
                    m.and_then(|x| x.error)
                );
                break;
            }
        }
    }
    assert!(finished, "Long video worker did not finish in time");

    // Use output in project -> produces DerivedMediaAsset
    let use_res = service
        .use_flow_output_in_project(&project.id, &snapshot.parent_id)
        .expect("use flow output in project");

    assert_eq!(use_res.project.derived_media_assets.len(), 1);
    assert_eq!(use_res.derived_asset.provenance.provider, "FLOW");
    assert_eq!(
        use_res.derived_asset.provenance.provider_job_id,
        snapshot.parent_id
    );
}

// -----------------------------------------------------------------------------
// 6. Rehydration Preserves Normalized Segments (Section 34)
// -----------------------------------------------------------------------------

#[test]
fn test_flow_p4a1_06_rehydration_preserves_normalized_children_and_zero_calls() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let store = super::flow::store::FlowJobStore::new(paths.clone());

    let mut manifest = FlowGenerationManifest::new(
        "parent_rehydrate_p4a1".to_string(),
        "req_p4a1".to_string(),
        "proj_p4a1".to_string(),
        "profile_01".to_string(),
        "hash_p4a1".to_string(),
        Some("media_p4a1".to_string()),
        "prompt_hash_p4a1".to_string(),
        Some("source.mp4".to_string()),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        None,
        FlowRequestedGenerationConfig::default(),
        "prompt".to_string(),
        "prompt_hash_p4a1".to_string(),
        super::flow::prompt_optimizer::PromptSource::SystemDefault,
        1,
        1,
        crate::ai::cloud::spec::SourceMediaFacts {
            duration_sec: 20.0,
            fps: 30.0,
            width: 576,
            height: 1024,
            has_audio: false,
            timing: None,
        },
        FlowSegmentPlan {
            segments: vec![],
            total_frames: 600,
            total_duration_sec: 20.0,
            target_fps: 30.0,
            capability_limit_sec: 10.0,
        },
        super::flow::capability::FlowCreditRecord::default(),
        FlowFinalAudioPolicy {
            preserve_original_audio: false,
            codec: "aac".to_string(),
        },
    );

    manifest.job_kind = FlowJobKind::LongVideoParent;
    let seg0 = FlowPlannedSegment {
        segment_index: 0,
        start_frame: 0,
        end_frame: 300,
        start_ms: 0,
        end_ms: 10000,
        planned_duration_sec: 10.0,
        planned_frame_count: 300,
        source_segment_path: PathBuf::from("seg_000.mp4"),
        source_segment_sha256: "sha_seg0".to_string(),
        child_job_id: Some("child_000".to_string()),
        state: FlowJobState::Completed, // Normalized and completed!
    };
    let seg1 = FlowPlannedSegment {
        segment_index: 1,
        start_frame: 300,
        end_frame: 600,
        start_ms: 10000,
        end_ms: 20000,
        planned_duration_sec: 10.0,
        planned_frame_count: 300,
        source_segment_path: PathBuf::from("seg_001.mp4"),
        source_segment_sha256: "sha_seg1".to_string(),
        child_job_id: None,
        state: FlowJobState::Planning, // Pending!
    };

    manifest.long_video_plan = Some(FlowLongVideoPlan {
        parent_job_id: "parent_rehydrate_p4a1".to_string(),
        project_id: "proj_p4a1".to_string(),
        source_media_id: Some("media_p4a1".to_string()),
        source_duration_ms: 20000,
        source_fps_rational: (30, 1),
        rational_fps: Some(FlowRationalFrameRate::new(30, 1)),
        fps_numerator: Some(30),
        fps_denominator: Some(1),
        source_timing_mode: "CFR".to_string(),
        working_proxy_created: false,
        working_proxy_path: None,
        working_proxy_sha256: None,
        strategy: "CONTIGUOUS_FRAME_ALIGNED".to_string(),
        segment_count: 2,
        segments: vec![seg0, seg1],
        requested_config: FlowRequestedGenerationConfig::default(),
        prompt_hash: "prompt_hash_p4a1".to_string(),
        transformation_intent: TransformationIntent::FaceReplace,
        identity_mode: IdentityMode::Generated,
        continuity_strategy: FlowIdentityContinuityStrategy::SamePromptBaseline,
        identity_continuity_guaranteed: false,
        created_at: chrono::Utc::now().to_rfc3339(),
    });

    store
        .save_manifest_atomic(&mut manifest)
        .expect("save manifest");

    // Rehydrate by reloading manifest from store
    let rehydrated = store
        .load_manifest("proj_p4a1", "parent_rehydrate_p4a1")
        .expect("load manifest");
    let plan = rehydrated.long_video_plan.unwrap();

    assert_eq!(
        plan.segments[0].state,
        FlowJobState::Completed,
        "Child 0 must remain completed & reusable"
    );
    assert_eq!(
        plan.segments[1].state,
        FlowJobState::Planning,
        "Pending child must remain pending without auto-submitting"
    );
}

// -----------------------------------------------------------------------------
// 7. Budget Guard Tests (Section 36 & 37)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_flow_p4a1_07_budget_guard_requires_explicit_max_total_credits() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let service = FlowRuntimeService::new(paths.clone());

    let proj_manager = ProjectManager::new(paths.clone());
    let project = proj_manager.create_project("Budget Guard Project").unwrap();

    let media_dir = paths.projects_dir.join(&project.id).join("media");
    fs::create_dir_all(&media_dir).unwrap();
    let video_15s = media_dir.join("source_15s.mp4");
    create_synthetic_test_video_frames(&video_15s, 450, 30.0, 576, 1024);

    let profile_dir = paths
        .app_data_dir
        .join("flow_profiles")
        .join("profile_mock");
    fs::create_dir_all(&profile_dir).unwrap();

    let intent = TransformationIntent::FaceReplace;
    let identity_mode = IdentityMode::Generated;
    let requested_config = FlowRequestedGenerationConfig::default();
    let prompt_text = "A continuous long video test";
    let prompt_hash = super::flow::prompt_optimizer::calculate_prompt_hash(prompt_text);
    let fp = super::flow::orchestrator::compute_configuration_fingerprint(
        "profile_mock",
        "media_15s",
        &prompt_hash,
        intent,
        identity_mode,
        &requested_config,
    );

    let preflight_ticket = FlowPreflightTicket {
        preflight_id: "pre_budget_guard".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "profile_mock".to_string(),
        project_id: project.id.clone(),
        source_media_id: "media_15s".to_string(),
        prompt_hash: prompt_hash.clone(),
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20),
        cost_provenance: super::flow::orchestrator::FlowCostProvenance::UploadedVideoEdit,
        checked_at: chrono::Utc::now().to_rfc3339(),
        expires_at: (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339(),
        ready_for_paid_submission: true,
    };
    service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(preflight_ticket);

    // Request without max_credits on long video > 10s MUST be rejected (Section 36)
    let req = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: "media_15s".to_string(),
        profile_id: "profile_mock".to_string(),
        transformation_intent: Some(intent),
        identity_mode: Some(identity_mode),
        prompt: prompt_text.to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: None, // Missing explicit maxTotalCredits budget!
        preserve_original_audio: Some(false),
        requested_config: Some(requested_config),
        configuration_fingerprint: Some(fp),
        preflight_id: Some("pre_budget_guard".to_string()),
    };

    let res = service.start_flow_generation(req, video_15s).await;
    assert!(res.is_err());
    let err_msg = res.err().unwrap();
    assert!(
        err_msg.contains("FLOW_TOTAL_CREDIT_BUDGET_REQUIRED"),
        "Missing max_credits on multi-segment video must return FLOW_TOTAL_CREDIT_BUDGET_REQUIRED, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_flow_p4a1_08_budget_guard_exceeded_fails_job() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let service = FlowRuntimeService::new(paths.clone());

    let proj_manager = ProjectManager::new(paths.clone());
    let project = proj_manager
        .create_project("Budget Exceeded Project")
        .unwrap();

    let media_dir = paths.projects_dir.join(&project.id).join("media");
    fs::create_dir_all(&media_dir).unwrap();
    let video_15s = media_dir.join("source_15s.mp4");
    create_synthetic_test_video_frames(&video_15s, 450, 30.0, 576, 1024);

    let profile_dir = paths
        .app_data_dir
        .join("flow_profiles")
        .join("profile_mock");
    fs::create_dir_all(&profile_dir).unwrap();

    let intent = TransformationIntent::FaceReplace;
    let identity_mode = IdentityMode::Generated;
    let requested_config = FlowRequestedGenerationConfig::default();
    let prompt_text = "A continuous long video test";
    let prompt_hash = super::flow::prompt_optimizer::calculate_prompt_hash(prompt_text);
    let fp = super::flow::orchestrator::compute_configuration_fingerprint(
        "profile_mock",
        "media_15s",
        &prompt_hash,
        intent,
        identity_mode,
        &requested_config,
    );

    let preflight_ticket = FlowPreflightTicket {
        preflight_id: "pre_budget_exceeded".to_string(),
        configuration_fingerprint: fp.clone(),
        profile_id: "profile_mock".to_string(),
        project_id: project.id.clone(),
        source_media_id: "media_15s".to_string(),
        prompt_hash: prompt_hash.clone(),
        requested_config: requested_config.clone(),
        live_displayed_credit_cost: Some(20),
        cost_provenance: super::flow::orchestrator::FlowCostProvenance::UploadedVideoEdit,
        checked_at: chrono::Utc::now().to_rfc3339(),
        expires_at: (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339(),
        ready_for_paid_submission: true,
    };
    service
        .orchestrator
        .preflight_tickets()
        .insert_ticket(preflight_ticket);

    // 15s video needs 2 segments = 40 credits. But max_credits is only 20!
    let req = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: "media_15s".to_string(),
        profile_id: "profile_mock".to_string(),
        transformation_intent: Some(intent),
        identity_mode: Some(identity_mode),
        prompt: prompt_text.to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: Some(20), // Only enough for 1 segment!
        preserve_original_audio: Some(false),
        requested_config: Some(requested_config),
        configuration_fingerprint: Some(fp),
        preflight_id: Some("pre_budget_exceeded".to_string()),
    };

    let snapshot = service
        .start_flow_generation(req, video_15s)
        .await
        .expect("start flow generation");

    // Wait for worker to encounter budget ceiling on segment 1
    let mut finished = false;
    for _ in 0..30 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if let Ok(st) = service.get_flow_job_status(&project.id, &snapshot.parent_id) {
            if st.state == FlowJobState::Failed {
                finished = true;
                let m = service
                    .orchestrator
                    .store()
                    .load_manifest(&project.id, &snapshot.parent_id)
                    .unwrap();
                let err = m.error.unwrap();
                assert_eq!(err.code, "FLOW_TOTAL_CREDIT_BUDGET_EXCEEDED");
                break;
            }
        }
    }
    assert!(finished, "Worker did not fail with budget exceeded");
}

// -----------------------------------------------------------------------------
// 8. Real Non-Submitting Preflight Acceptance (Phase FLOW-P4-B0)
// -----------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Real live Google Flow non-submitting preflight for P4-B0"]
async fn test_flow_p4b0_real_non_submitting_preflight_acceptance() {
    println!("==================================================");
    println!("FLOW-P4-B0 REAL TWO-SEGMENT NON-SUBMITTING PREFLIGHT");
    println!("FLOW_PAID_CLICKS = 0, FLOW_LIVE_GENERATIONS = 0, FLOW_CREDITS_SPENT = 0");
    println!("==================================================");

    let base_path = PathBuf::from("D:/rustProject/autovideo-ai/src-tauri/.autovideo_data");
    let paths = StoragePaths::resolve_from_base(&base_path);
    let service = FlowRuntimeService::new(paths.clone());
    let proj_manager = ProjectManager::new(paths.clone());

    // Step 1: Initial safe credit balance refresh
    println!("[P4-B0 STEP 1] Refreshing live credit balance before preflight...");
    let init_credit_status = service
        .refresh_flow_credit_balance("profile_2")
        .await
        .expect("Initial credit balance refresh failed");

    println!("INITIAL_PROFILE_STATUS: {:?}", init_credit_status.status);
    println!("INITIAL_CREDIT_BALANCE: {:?}", init_credit_status.balance);
    assert_eq!(
        init_credit_status.status,
        super::flow::orchestrator::FlowCreditStatus::Ready,
        "profile_2 must be authenticated and READY"
    );
    let initial_balance = init_credit_status
        .balance
        .expect("profile_2 must have a valid credit balance");

    // Step 2: Prepare deterministic source asset (15s = 2 segments)
    let source_asset = PathBuf::from("D:/rustProject/autovideo-ai/test-assets/p4b_source_15s.mp4");
    assert!(
        source_asset.exists(),
        "Source asset D:/rustProject/autovideo-ai/test-assets/p4b_source_15s.mp4 must exist"
    );

    let temp_dir = tempdir().unwrap();
    let seg0_path = temp_dir.path().join("segment_000.mp4");
    let seg1_path = temp_dir.path().join("segment_001.mp4");

    // Segment 0: 0.0s to 10.0s (300 frames @ 30fps)
    let out0 = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            "0.000000",
            "-i",
            source_asset.to_str().unwrap(),
            "-t",
            "10.000000",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-r",
            "30",
            "-an",
            seg0_path.to_str().unwrap(),
        ])
        .output()
        .expect("Extract segment 0");
    assert!(out0.status.success(), "Extract segment 0 failed");

    // Segment 1: 10.0s to 15.0s (150 frames @ 30fps)
    let out1 = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            "10.000000",
            "-i",
            source_asset.to_str().unwrap(),
            "-t",
            "5.000000",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-pix_fmt",
            "yuv420p",
            "-r",
            "30",
            "-an",
            seg1_path.to_str().unwrap(),
        ])
        .output()
        .expect("Extract segment 1");
    assert!(out1.status.success(), "Extract segment 1 failed");

    let project = proj_manager
        .create_project("Phase FLOW-P4-B0 Non-Submitting Preflight")
        .unwrap();

    let requested_config = FlowRequestedGenerationConfig {
        model_id: Some("Omni Flash".to_string()),
        resolution: Some("720p".to_string()),
        duration_sec: Some(10),
        orientation: Some("PORTRAIT / 9:16".to_string()),
        output_count: 1,
    };

    // Step 3: Segment 0 Preflight (NON-SUBMITTING)
    println!("--------------------------------------------------");
    println!("[P4-B0 STEP 2] Executing Preflight for Segment 0 (0-10s)...");
    let preflight_req_0 = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: "media_seg0".to_string(),
        profile_id: "profile_2".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "A continuous long video test".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: Some(50),
        preserve_original_audio: Some(true),
        requested_config: Some(requested_config.clone()),
        configuration_fingerprint: None,
        preflight_id: None,
    };

    let preflight_0 = service
        .preflight_flow_generation(preflight_req_0, seg0_path.clone())
        .await
        .expect("Segment 0 preflight must succeed");

    println!(
        "SEGMENT_0_PREFLIGHT_READY: {}",
        preflight_0.ready_for_paid_submission
    );
    println!(
        "SEGMENT_0_VIDEO_EDIT_ACTIVE: {}",
        preflight_0.video_edit_active
    );
    println!(
        "SEGMENT_0_CONFIG_VERIFIED: {}",
        preflight_0.configuration_verified
    );
    let seg0_cost = preflight_0
        .live_displayed_credit_cost
        .expect("Segment 0 must report live credit cost");
    println!("SEGMENT_0_LIVE_COST: {}", seg0_cost);

    // Invalidate Segment 0 preflight ticket immediately (ZERO SUBMISSION)
    service
        .orchestrator
        .preflight_tickets()
        .consume_ticket(&preflight_0.preflight_id);

    // Step 4: Segment 1 Preflight (NON-SUBMITTING)
    println!("--------------------------------------------------");
    println!("[P4-B0 STEP 3] Executing Preflight for Segment 1 (10-15s)...");
    let preflight_req_1 = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: "media_seg1".to_string(),
        profile_id: "profile_2".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "A continuous long video test".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: Some(50),
        preserve_original_audio: Some(true),
        requested_config: Some(requested_config.clone()),
        configuration_fingerprint: None,
        preflight_id: None,
    };

    let preflight_1 = service
        .preflight_flow_generation(preflight_req_1, seg1_path.clone())
        .await
        .expect("Segment 1 preflight must succeed");

    println!(
        "SEGMENT_1_PREFLIGHT_READY: {}",
        preflight_1.ready_for_paid_submission
    );
    println!(
        "SEGMENT_1_VIDEO_EDIT_ACTIVE: {}",
        preflight_1.video_edit_active
    );
    println!(
        "SEGMENT_1_CONFIG_VERIFIED: {}",
        preflight_1.configuration_verified
    );
    let seg1_cost = preflight_1
        .live_displayed_credit_cost
        .expect("Segment 1 must report live credit cost");
    println!("SEGMENT_1_LIVE_COST: {}", seg1_cost);

    // Invalidate Segment 1 preflight ticket immediately (ZERO SUBMISSION)
    service
        .orchestrator
        .preflight_tickets()
        .consume_ticket(&preflight_1.preflight_id);

    // Step 5: Final Safe Credit Balance Refresh
    println!("--------------------------------------------------");
    println!("[P4-B0 STEP 4] Refreshing final credit balance...");
    let final_credit_status = service
        .refresh_flow_credit_balance("profile_2")
        .await
        .expect("Final credit balance refresh failed");

    let final_balance = final_credit_status
        .balance
        .expect("Final balance must be present");
    println!("FINAL_CREDIT_BALANCE: {}", final_balance);

    let credits_spent = initial_balance.saturating_sub(final_balance);
    println!("CREDITS_SPENT: {}", credits_spent);
    assert_eq!(credits_spent, 0, "ZERO CREDITS MUST BE SPENT IN PREFLIGHT");

    let projected_cost = seg0_cost + seg1_cost;

    println!("==================================================");
    println!("P4-B0 DISCOVERY SUMMARY & AUTHORIZATION FORMAT");
    println!("FLOW_PAID_CLICKS = 0");
    println!("FLOW_LIVE_GENERATIONS = 0");
    println!("FLOW_CREDITS_SPENT = 0");
    println!("SEGMENT_0_LIVE_COST = {}", seg0_cost);
    println!("SEGMENT_1_LIVE_COST = {}", seg1_cost);
    println!("PROJECTED_CURRENT_LIVE_COST = {}", projected_cost);
    println!();
    println!("PROPOSED AUTHORIZATION FORMAT:");
    println!(
        "Approve FLOW-P4-B: max {} credits total, exactly 2 generations.",
        projected_cost
    );
    println!("==================================================");
}
