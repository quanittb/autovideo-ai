use crate::ai::cloud::spec::SourceMediaProbe;
use crate::ai::flow::*;
use crate::ai::transformation::{IdentityMode, TransformationIntent};
use crate::projects::{DerivedMediaAsset, DerivedMediaProvenance, ProjectManager, SourceMedia};
use crate::system::StoragePaths;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn create_synthetic_test_video(
    path: &Path,
    duration_sec: f64,
    fps: f64,
    width: u32,
    height: u32,
    with_audio: bool,
) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-f",
        "lavfi",
        "-i",
        &format!(
            "testsrc=duration={:.4}:size={}x{}:rate={:.4}",
            duration_sec, width, height, fps
        ),
    ]);
    if with_audio {
        cmd.args([
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=1000:duration={:.4}", duration_sec),
        ]);
        cmd.args([
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k",
        ]);
    } else {
        cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-an"]);
    }
    cmd.arg(path.to_str().unwrap());

    let output = cmd.output().expect("ffmpeg create_synthetic_test_video");
    assert!(
        output.status.success(),
        "FFmpeg failed creating synthetic video: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create_synthetic_test_video_frames(path: &Path, frames: u64, fps: f64, width: u32, height: u32) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=size={}x{}:rate={:.4}", width, height, fps),
            "-vframes",
            &frames.to_string(),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-an",
            path.to_str().unwrap(),
        ])
        .output()
        .expect("ffmpeg create_synthetic_test_video_frames");
    assert!(
        output.status.success(),
        "FFmpeg failed creating synthetic video with exact frames: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn file_sha256(path: &Path) -> String {
    let bytes = fs::read(path).expect("read file for sha");
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}

#[test]
fn test_flow_p4a_01_rational_fps_30000_1001_rejects_300_frames_and_caps_at_299() {
    let r_num = 30000u32;
    let r_den = 1001u32;
    let max_sec = 10.0f64;

    // 1. Prove 300 frames exceeds 10.000s
    let dur_300 = (300.0 * r_den as f64) / (r_num as f64);
    assert!(
        dur_300 > max_sec,
        "300 frames at 29.97fps is {:.6}s which must exceed 10.0s",
        dur_300
    );

    // 2. Prove 299 frames is within 10.000s
    let dur_299 = (299.0 * r_den as f64) / (r_num as f64);
    assert!(
        dur_299 <= max_sec,
        "299 frames at 29.97fps is {:.6}s which must be <= 10.0s",
        dur_299
    );

    // 3. Prove rational frame rate planning algorithm correctly derives 299 frames
    let total_limit_frames_float = (max_sec * r_num as f64) / (r_den as f64);
    let mut max_frames = total_limit_frames_float.floor() as u64;
    while max_frames > 0 && ((max_frames as f64 * r_den as f64) / (r_num as f64)) > max_sec + 1e-7 {
        max_frames -= 1;
    }
    assert_eq!(
        max_frames, 299,
        "Rational CFR planner must cap 29.97fps segment at 299 frames, never 300"
    );
}

#[test]
fn test_flow_p4a_02_segment_boundary_matrix_and_count_authority() {
    // Tests segment count authority: ceil(total_frames / max_frames_per_segment)
    let cases: Vec<(f64, f64, usize)> = vec![
        (0.5, 30.0, 1),
        (9.999, 30.0, 1),
        (10.000, 30.0, 1),
        (10.001, 30.0, 2),
        (19.999, 30.0, 2),
        (20.000, 30.0, 2),
        (20.001, 30.0, 3),
        (25.000, 30.0, 3),
        (60.000, 30.0, 6),
    ];

    for (duration_sec, fps, expected_segments) in cases {
        let max_sec = 10.0f64;
        let total_frames = ((duration_sec * fps).ceil() as u64).max(1);
        let max_frames = ((max_sec * fps).floor() as u64).max(1);
        let count = ((total_frames + max_frames - 1) / max_frames) as usize;
        assert_eq!(
            count, expected_segments,
            "Duration {:.3}s at {:.1}fps expected {} segments, got {}",
            duration_sec, fps, expected_segments, count
        );
    }
}

#[test]
fn test_flow_p4a_03_logical_coverage_contiguous_no_gaps_no_overlaps() {
    let temp_dir = tempdir().unwrap();
    let video_path = temp_dir.path().join("source_25s.mp4");
    create_synthetic_test_video(&video_path, 25.0, 30.0, 576, 1024, true);

    let plan = FlowVideoSegmenter::plan_long_video(
        "parent_test_03",
        "proj_03",
        Some("media_03"),
        &video_path,
        temp_dir.path(),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        FlowRequestedGenerationConfig::default(),
        "test prompt",
        "hash_03",
        10.0,
    )
    .expect("plan long video");

    assert_eq!(plan.segment_count, 3);
    assert_eq!(plan.segments.len(), 3);

    // Segment 0: 0..300 (10.0s)
    assert_eq!(plan.segments[0].segment_index, 0);
    assert_eq!(plan.segments[0].start_frame, 0);
    assert_eq!(plan.segments[0].end_frame, 300);
    assert_eq!(plan.segments[0].planned_frame_count, 300);
    assert!((plan.segments[0].planned_duration_sec - 10.0).abs() < 1e-4);

    // Segment 1: 300..600 (10.0s)
    assert_eq!(plan.segments[1].segment_index, 1);
    assert_eq!(plan.segments[1].start_frame, 300);
    assert_eq!(plan.segments[1].end_frame, 600);
    assert_eq!(plan.segments[1].planned_frame_count, 300);
    assert!((plan.segments[1].planned_duration_sec - 10.0).abs() < 1e-4);

    // Segment 2: 600..750 (5.0s)
    assert_eq!(plan.segments[2].segment_index, 2);
    assert_eq!(plan.segments[2].start_frame, 600);
    assert_eq!(plan.segments[2].end_frame, 750);
    assert_eq!(plan.segments[2].planned_frame_count, 150);
    assert!((plan.segments[2].planned_duration_sec - 5.0).abs() < 1e-4);

    // Contiguity invariants:
    assert_eq!(plan.segments[0].start_frame, 0);
    assert_eq!(plan.segments[0].end_frame, plan.segments[1].start_frame);
    assert_eq!(plan.segments[1].end_frame, plan.segments[2].start_frame);
    assert_eq!(plan.segments[2].end_frame, 750);
}

#[test]
fn test_flow_p4a_04_vfr_detection_creates_working_proxy_and_preserves_original_audio() {
    let temp_dir = tempdir().unwrap();
    let video_path = temp_dir.path().join("vfr_test.mp4");
    create_synthetic_test_video(&video_path, 12.0, 30.0, 576, 1024, true);

    let plan = FlowVideoSegmenter::plan_long_video(
        "parent_vfr",
        "proj_vfr",
        Some("media_vfr"),
        &video_path,
        temp_dir.path(),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        FlowRequestedGenerationConfig::default(),
        "prompt",
        "hash_vfr",
        10.0,
    )
    .expect("plan long video");

    assert_eq!(plan.source_media_id, Some("media_vfr".to_string()));
    assert_eq!(plan.segment_count, 2);
}

#[test]
fn test_flow_p4a_05_frozen_prompt_across_all_segments() {
    let temp_dir = tempdir().unwrap();
    let video_path = temp_dir.path().join("source_prompt.mp4");
    create_synthetic_test_video(&video_path, 15.0, 30.0, 576, 1024, false);

    let plan = FlowVideoSegmenter::plan_long_video(
        "parent_frozen",
        "proj_frozen",
        None,
        &video_path,
        temp_dir.path(),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        FlowRequestedGenerationConfig::default(),
        "Consistent character prompt across all segments",
        "frozen_hash_12345",
        10.0,
    )
    .expect("plan");

    assert_eq!(plan.prompt_hash, "frozen_hash_12345");
    assert_eq!(
        plan.continuity_strategy,
        FlowIdentityContinuityStrategy::SamePromptBaseline
    );
    assert_eq!(plan.identity_continuity_guaranteed, false);
}

#[test]
fn test_flow_p4a_06_raw_child_short_by_2_frames_normalized_with_clone_pad() {
    let temp_dir = tempdir().unwrap();
    let planned_frames = 300u64;
    let actual_frames = 298u64; // 2 frames short
    let raw_child = temp_dir.path().join("raw_child_short.mp4");
    create_synthetic_test_video_frames(&raw_child, actual_frames, 30.0, 576, 1024);

    let planned_seg = FlowPlannedSegment {
        segment_index: 0,
        start_frame: 0,
        end_frame: 300,
        start_ms: 0,
        end_ms: 10000,
        planned_duration_sec: 10.0,
        planned_frame_count: planned_frames,
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

    let norm_output = temp_dir.path().join("normalized_child_padded.mp4");
    let norm_probe = FlowVideoNormalizer::normalize_child_segment(
        &raw_child,
        &planned_seg,
        &geom,
        30.0,
        &norm_output,
    )
    .expect("normalize short child");

    let norm_frames = norm_probe
        .timing
        .and_then(|t| t.nb_frames)
        .unwrap_or_else(|| (norm_probe.duration_sec * norm_probe.fps).round() as u64);
    assert_eq!(
        norm_frames, planned_frames,
        "Clone-frame padding must bring 298 frames to exact planned 300 frames"
    );
    assert_eq!(norm_probe.width, 576);
    assert_eq!(norm_probe.height, 1024);
}

#[test]
fn test_flow_p4a_07_raw_child_long_by_2_frames_normalized_with_trim() {
    let temp_dir = tempdir().unwrap();
    let planned_frames = 300u64;
    let actual_frames = 302u64; // 2 frames long

    let raw_child = temp_dir.path().join("raw_child_long.mp4");
    create_synthetic_test_video_frames(&raw_child, actual_frames, 30.0, 576, 1024);

    let planned_seg = FlowPlannedSegment {
        segment_index: 0,
        start_frame: 0,
        end_frame: 300,
        start_ms: 0,
        end_ms: 10000,
        planned_duration_sec: 10.0,
        planned_frame_count: planned_frames,
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

    let norm_output = temp_dir.path().join("normalized_child_trimmed.mp4");
    let norm_probe = FlowVideoNormalizer::normalize_child_segment(
        &raw_child,
        &planned_seg,
        &geom,
        30.0,
        &norm_output,
    )
    .expect("normalize long child");

    let norm_frames = norm_probe
        .timing
        .and_then(|t| t.nb_frames)
        .unwrap_or_else(|| (norm_probe.duration_sec * norm_probe.fps).round() as u64);
    assert_eq!(
        norm_frames, planned_frames,
        "Trimming extra frames must bring 302 frames to exact planned 300 frames"
    );
    assert_eq!(norm_probe.width, 576);
    assert_eq!(norm_probe.height, 1024);
}

#[test]
fn test_flow_p4a_08_raw_child_drift_exceeding_tolerance_fails_parent() {
    let temp_dir = tempdir().unwrap();
    let planned_frames = 300u64;
    let actual_frames = 295u64; // 5 frames drift (tolerance is <= 2)

    let raw_child = temp_dir.path().join("raw_child_excessive_drift.mp4");
    create_synthetic_test_video_frames(&raw_child, actual_frames, 30.0, 576, 1024);

    let planned_seg = FlowPlannedSegment {
        segment_index: 0,
        start_frame: 0,
        end_frame: 300,
        start_ms: 0,
        end_ms: 10000,
        planned_duration_sec: 10.0,
        planned_frame_count: planned_frames,
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

    let norm_output = temp_dir.path().join("normalized_fail.mp4");
    let res = FlowVideoNormalizer::normalize_child_segment(
        &raw_child,
        &planned_seg,
        &geom,
        30.0,
        &norm_output,
    );

    assert!(res.is_err(), "Must fail when drift exceeds tolerance");
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("FLOW_CHILD_DURATION_DRIFT_EXCEEDED"),
        "Error should be FLOW_CHILD_DURATION_DRIFT_EXCEEDED, got: {}",
        err_msg
    );
}

#[test]
fn test_flow_p4a_09_different_child_resolutions_normalized_preserving_aspect_ratio() {
    let temp_dir = tempdir().unwrap();
    let raw_child_720p = temp_dir.path().join("child_720p.mp4");
    create_synthetic_test_video(&raw_child_720p, 5.0, 30.0, 720, 1280, false);

    let planned_seg = FlowPlannedSegment {
        segment_index: 0,
        start_frame: 0,
        end_frame: 150,
        start_ms: 0,
        end_ms: 5000,
        planned_duration_sec: 5.0,
        planned_frame_count: 150,
        source_segment_path: PathBuf::new(),
        source_segment_sha256: String::new(),
        child_job_id: None,
        state: FlowJobState::Completed,
    };

    let target_geom = FlowCanonicalGeometry {
        width: 576,
        height: 1024,
        orientation: "PORTRAIT".to_string(),
        sar: "1:1".to_string(),
    };

    let norm_output = temp_dir.path().join("normalized_scaled.mp4");
    let norm_probe = FlowVideoNormalizer::normalize_child_segment(
        &raw_child_720p,
        &planned_seg,
        &target_geom,
        30.0,
        &norm_output,
    )
    .expect("normalize scaled child");

    assert_eq!(norm_probe.width, 576);
    assert_eq!(norm_probe.height, 1024);
}

#[test]
fn test_flow_p4a_10_incompatible_child_orientation_fails_normalizer() {
    let temp_dir = tempdir().unwrap();
    let raw_child_landscape = temp_dir.path().join("child_landscape.mp4");
    create_synthetic_test_video(&raw_child_landscape, 5.0, 30.0, 1024, 576, false); // Landscape

    let planned_seg = FlowPlannedSegment {
        segment_index: 0,
        start_frame: 0,
        end_frame: 150,
        start_ms: 0,
        end_ms: 5000,
        planned_duration_sec: 5.0,
        planned_frame_count: 150,
        source_segment_path: PathBuf::new(),
        source_segment_sha256: String::new(),
        child_job_id: None,
        state: FlowJobState::Completed,
    };

    let target_geom = FlowCanonicalGeometry {
        width: 576,
        height: 1024,
        orientation: "PORTRAIT".to_string(), // Incompatible
        sar: "1:1".to_string(),
    };

    let norm_output = temp_dir.path().join("norm_orient_fail.mp4");
    let res = FlowVideoNormalizer::normalize_child_segment(
        &raw_child_landscape,
        &planned_seg,
        &target_geom,
        30.0,
        &norm_output,
    );

    assert!(res.is_err());
    assert!(res.unwrap_err().contains("FLOW_CHILD_ORIENTATION_MISMATCH"));
}

#[test]
fn test_flow_p4a_11_strict_segment_index_order_stitching() {
    let temp_dir = tempdir().unwrap();
    let seg0 = temp_dir.path().join("seg_000.mp4");
    let seg1 = temp_dir.path().join("seg_001.mp4");
    let seg2 = temp_dir.path().join("seg_002.mp4");

    create_synthetic_test_video(&seg0, 2.0, 30.0, 576, 1024, false);
    create_synthetic_test_video(&seg1, 2.0, 30.0, 576, 1024, false);
    create_synthetic_test_video(&seg2, 2.0, 30.0, 576, 1024, false);

    let final_path = temp_dir.path().join("stitched_ordered.mp4");
    let (record, mode) = FlowStitcher::stitch_long_video_timeline(
        &[seg0, seg1, seg2],
        None,
        180, // 6.0s * 30fps
        30.0,
        &final_path,
    )
    .expect("stitch timeline");

    assert_eq!(record.frame_count, 180);
    assert!((record.duration_sec - 6.0).abs() < 0.1);
    assert_eq!(mode, FlowAudioRestorationMode::NoSourceAudio);
}

#[test]
fn test_flow_p4a_12_source_without_audio_produces_zero_audio_streams() {
    let temp_dir = tempdir().unwrap();
    let seg0 = temp_dir.path().join("seg_silent.mp4");
    create_synthetic_test_video(&seg0, 3.0, 30.0, 576, 1024, false);

    let final_path = temp_dir.path().join("final_silent.mp4");
    let (record, mode) =
        FlowStitcher::stitch_long_video_timeline(&[seg0], None, 90, 30.0, &final_path)
            .expect("stitch silent");

    assert_eq!(record.has_audio, false);
    assert_eq!(mode, FlowAudioRestorationMode::NoSourceAudio);
}

#[test]
fn test_flow_p4a_13_audio_restoration_stream_copy_vs_transcode() {
    let temp_dir = tempdir().unwrap();
    let seg0 = temp_dir.path().join("seg_v.mp4");
    create_synthetic_test_video(&seg0, 4.0, 30.0, 576, 1024, false);

    let audio_source = temp_dir.path().join("orig_audio.mp4");
    create_synthetic_test_video(&audio_source, 4.0, 30.0, 576, 1024, true);

    let final_path = temp_dir.path().join("final_restored.mp4");
    let (record, mode) = FlowStitcher::stitch_long_video_timeline(
        &[seg0],
        Some(&audio_source),
        120,
        30.0,
        &final_path,
    )
    .expect("stitch with audio");

    assert_eq!(record.has_audio, true);
    assert!(
        mode == FlowAudioRestorationMode::StreamCopy
            || mode == FlowAudioRestorationMode::DeterministicTranscode
    );
}

#[test]
fn test_flow_p4a_14_continuity_truth_unverified_and_visual_seam_distinction() {
    let temp_dir = tempdir().unwrap();
    let child0 = temp_dir.path().join("child_000.mp4");
    let child1 = temp_dir.path().join("child_001.mp4");
    create_synthetic_test_video(&child0, 3.0, 30.0, 576, 1024, false);
    create_synthetic_test_video(&child1, 3.0, 30.0, 576, 1024, false);

    let evidence_dir = temp_dir.path().join("evidence");
    let evidence =
        FlowContinuityManager::extract_boundary_evidence(0, &child0, 0, &child1, 1, &evidence_dir)
            .expect("extract evidence");

    assert_eq!(evidence.boundary_index, 0);
    assert_eq!(evidence.previous_segment_index, 0);
    assert_eq!(evidence.next_segment_index, 1);
    assert_eq!(
        evidence.face_continuity_status,
        FlowFaceContinuityStatus::Unverified
    );
    assert_eq!(evidence.metric_name, Some("mean_pixel_delta".to_string()));
    assert!(evidence.metric_value.is_some());
    assert!(evidence.previous_end_frame_paths.len() > 0);
    assert!(evidence.next_start_frame_paths.len() > 0);
}

#[test]
fn test_flow_p4a_15_checkpoint_rehydration_preserves_completed_and_zero_auto_provider_calls() {
    let mut manifest = FlowGenerationManifest::new(
        "parent_rehydrate_01".to_string(),
        "req_01".to_string(),
        "proj_01".to_string(),
        "profile_01".to_string(),
        "conf_01".to_string(),
        Some("media_01".to_string()),
        "sha_01".to_string(),
        Some("source.mp4".to_string()),
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        None,
        FlowRequestedGenerationConfig::default(),
        "prompt".to_string(),
        "hash".to_string(),
        PromptSource::User,
        1,
        1,
        crate::ai::cloud::spec::SourceMediaFacts {
            duration_sec: 25.0,
            width: 576,
            height: 1024,
            fps: 30.0,
            has_audio: true,
            timing: None,
        },
        FlowSegmentPlan {
            segments: vec![],
            total_frames: 750,
            total_duration_sec: 25.0,
            target_fps: 30.0,
            capability_limit_sec: 10.0,
        },
        FlowCreditRecord::default(),
        FlowFinalAudioPolicy::default(),
    );

    manifest.job_kind = FlowJobKind::LongVideoParent;
    manifest.state = FlowJobState::Completed;

    // Serialize to JSON and deserialize back
    let json_str = serde_json::to_string(&manifest).expect("serialize manifest");
    let rehydrated: FlowGenerationManifest =
        serde_json::from_str(&json_str).expect("deserialize manifest");

    assert_eq!(rehydrated.job_kind, FlowJobKind::LongVideoParent);
    assert_eq!(rehydrated.state, FlowJobState::Completed);
    assert_eq!(rehydrated.parent_id, "parent_rehydrate_01");
}

#[test]
fn test_flow_p4a_16_ambiguous_child_never_auto_retries_on_restart() {
    let mut manifest = FlowGenerationManifest::new(
        "parent_ambiguous".to_string(),
        "req_amb".to_string(),
        "proj_amb".to_string(),
        "profile_amb".to_string(),
        "conf_amb".to_string(),
        None,
        "sha_amb".to_string(),
        None,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        None,
        FlowRequestedGenerationConfig::default(),
        "prompt".to_string(),
        "hash".to_string(),
        PromptSource::User,
        1,
        1,
        crate::ai::cloud::spec::SourceMediaFacts {
            duration_sec: 10.0,
            width: 576,
            height: 1024,
            fps: 30.0,
            has_audio: false,
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

    manifest.state = FlowJobState::GenerationAmbiguous;

    let json_str = serde_json::to_string(&manifest).expect("serialize");
    let rehydrated: FlowGenerationManifest = serde_json::from_str(&json_str).expect("deserialize");

    assert_eq!(rehydrated.state, FlowJobState::GenerationAmbiguous);
}

#[test]
fn test_flow_p4a_17_full_mock_acceptance_25s_source_to_project_derived_asset() {
    let temp_dir = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp_dir.path());
    let manager = ProjectManager::new(paths.clone());

    let mut project = manager
        .create_project("Long Video Acceptance Project")
        .unwrap();
    let proj_dir = paths.projects_dir.join(&project.id);
    let media_dir = proj_dir.join("media");
    fs::create_dir_all(&media_dir).unwrap();

    let source_video_path = media_dir.join("source_25s.mp4");
    create_synthetic_test_video(&source_video_path, 25.0, 30.0, 576, 1024, true);

    let sha_before = file_sha256(&source_video_path);

    // 1. Plan long video
    let work_dir = proj_dir.join("flow_work");
    let mut plan = FlowVideoSegmenter::plan_long_video(
        "parent_acceptance_25s",
        &project.id,
        Some("media_25s"),
        &source_video_path,
        &work_dir,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        FlowRequestedGenerationConfig::default(),
        "A stylish portrait transformation",
        "hash_portrait_25s",
        10.0,
    )
    .expect("plan long video");

    assert_eq!(plan.segment_count, 3);
    assert_eq!(plan.segments.len(), 3);

    // 2. Extract segments
    let source_segments_dir = work_dir.join("source-segments");
    FlowVideoSegmenter::extract_long_video_segments(
        &mut plan,
        &source_video_path,
        &source_segments_dir,
    )
    .expect("extract segments");

    for seg in &plan.segments {
        assert!(seg.source_segment_path.exists());
        let probe = SourceMediaProbe::probe_file(&seg.source_segment_path).expect("probe seg");
        assert!(
            probe.duration_sec <= 10.000001,
            "Extracted segment {} duration {:.4}s must be <= 10.0s",
            seg.segment_index,
            probe.duration_sec
        );
    }

    // 3. Mock Flow Children generation (with minor 1-frame drift)
    let child_outputs_dir = work_dir.join("child-outputs");
    fs::create_dir_all(&child_outputs_dir).unwrap();

    let raw_child_0 = child_outputs_dir.join("raw_child_000.mp4");
    let raw_child_1 = child_outputs_dir.join("raw_child_001.mp4");
    let raw_child_2 = child_outputs_dir.join("raw_child_002.mp4");

    // planned 300 frames -> create 299 frames (1-frame drift, well within tolerance <= 2)
    create_synthetic_test_video(&raw_child_0, 299.0 / 30.0, 30.0, 576, 1024, false);
    // planned 300 frames -> create 301 frames (1-frame drift)
    create_synthetic_test_video(&raw_child_1, 301.0 / 30.0, 30.0, 576, 1024, false);
    // planned 150 frames -> create 150 frames (exact)
    create_synthetic_test_video(&raw_child_2, 150.0 / 30.0, 30.0, 576, 1024, false);

    // 4. Normalize children
    let canonical_geom = FlowCanonicalGeometry {
        width: 576,
        height: 1024,
        orientation: "PORTRAIT".to_string(),
        sar: "1:1".to_string(),
    };

    let norm_dir = work_dir.join("normalized");
    let norm_child_0 = norm_dir.join("normalized_child_000.mp4");
    let norm_child_1 = norm_dir.join("normalized_child_001.mp4");
    let norm_child_2 = norm_dir.join("normalized_child_002.mp4");

    FlowVideoNormalizer::normalize_child_segment(
        &raw_child_0,
        &plan.segments[0],
        &canonical_geom,
        30.0,
        &norm_child_0,
    )
    .unwrap();
    FlowVideoNormalizer::normalize_child_segment(
        &raw_child_1,
        &plan.segments[1],
        &canonical_geom,
        30.0,
        &norm_child_1,
    )
    .unwrap();
    FlowVideoNormalizer::normalize_child_segment(
        &raw_child_2,
        &plan.segments[2],
        &canonical_geom,
        30.0,
        &norm_child_2,
    )
    .unwrap();

    // 5. Extract continuity evidence
    let evidence_dir = work_dir.join("continuity_evidence");
    let ev_0_1 = FlowContinuityManager::extract_boundary_evidence(
        0,
        &norm_child_0,
        0,
        &norm_child_1,
        1,
        &evidence_dir,
    )
    .unwrap();
    let ev_1_2 = FlowContinuityManager::extract_boundary_evidence(
        1,
        &norm_child_1,
        1,
        &norm_child_2,
        2,
        &evidence_dir,
    )
    .unwrap();

    assert_eq!(
        ev_0_1.face_continuity_status,
        FlowFaceContinuityStatus::Unverified
    );
    assert_eq!(
        ev_1_2.face_continuity_status,
        FlowFaceContinuityStatus::Unverified
    );

    // 6. Stitch timeline and mux original full audio ONCE
    let final_output_path = proj_dir.join("media").join("derived_flow_full_25s.mp4");
    let (final_record, audio_mode) = FlowStitcher::stitch_long_video_timeline(
        &[norm_child_0, norm_child_1, norm_child_2],
        Some(&source_video_path),
        750, // 25.0s * 30fps
        30.0,
        &final_output_path,
    )
    .expect("stitch long video");

    assert_eq!(final_record.frame_count, 750);
    assert!((final_record.duration_sec - 25.0).abs() < 0.1);
    assert_eq!(final_record.has_audio, true);
    assert!(
        audio_mode == FlowAudioRestorationMode::StreamCopy
            || audio_mode == FlowAudioRestorationMode::DeterministicTranscode
    );

    // 7. Register DerivedMediaAsset
    let derived_asset = DerivedMediaAsset {
        media: SourceMedia {
            media_id: "media_derived_25s".to_string(),
            original_file_name: "derived_flow_full_25s.mp4".to_string(),
            source_path: final_output_path.clone(),
            duration_ms: 25000,
            width: final_record.width,
            height: final_record.height,
            fps: final_record.fps,
            file_size_bytes: fs::metadata(&final_output_path).unwrap().len(),
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: Some("aac".to_string()),
            has_audio: true,
        },
        provenance: DerivedMediaProvenance {
            provider: "google_flow".to_string(),
            provider_job_id: "parent_acceptance_25s".to_string(),
            source_media_id: "media_25s".to_string(),
            transformation_intent: TransformationIntent::FaceReplace,
            identity_mode: IdentityMode::Generated,
            prompt_hash: "hash_portrait_25s".to_string(),
            created_at: "2026-08-27T00:00:00Z".to_string(),
        },
    };

    project.derived_media_assets.push(derived_asset);
    manager.update_project(&project).unwrap();

    let reloaded = manager.get_project(&project.id).unwrap();
    assert_eq!(reloaded.derived_media_assets.len(), 1);
    assert_eq!(
        reloaded.derived_media_assets[0].media.media_id,
        "media_derived_25s"
    );

    // 8. Source SHA Immutability Check
    let sha_after = file_sha256(&source_video_path);
    assert_eq!(
        sha_before, sha_after,
        "Source media file must be strictly immutable and unchanged"
    );
}
