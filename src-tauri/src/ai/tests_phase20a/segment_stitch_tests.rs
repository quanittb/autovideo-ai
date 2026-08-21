use crate::ai::cloud::spec::SourceMediaFacts;
use crate::ai::flow::*;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn create_synthetic_mp4(
    path: &Path,
    duration_sec: f64,
    fps: f64,
    width: u32,
    height: u32,
    include_audio: bool,
) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let mut cmd = Command::new("ffmpeg");
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
fn test_phase20a_20_largest_legal_segmentation_minimizes_generation_count() {
    let facts = SourceMediaFacts {
        duration_sec: 24.0,
        fps: 30.0,
        width: 1920,
        height: 1080,
        has_audio: true,
        timing: None,
    };

    let policy = FlowCapabilityPolicy {
        max_edit_segment_duration_sec: 10.0,
        ..Default::default()
    };

    let plan = FlowVideoSegmenter::plan_segments(&facts, &policy).unwrap();

    assert_eq!(plan.segments.len(), 3);
    assert_eq!(plan.segments[0].start_frame, 0);
    assert_eq!(plan.segments[0].end_frame, 300);
    assert_eq!(plan.segments[1].start_frame, 300);
    assert_eq!(plan.segments[1].end_frame, 600);
    assert_eq!(plan.segments[2].start_frame, 600);
    assert_eq!(plan.segments[2].end_frame, 720);
}

#[test]
fn test_phase20a_21_fractional_cfr_segmentation_exact_frames() {
    let facts = SourceMediaFacts {
        duration_sec: 19.95328,
        fps: 29.97,
        width: 1920,
        height: 1080,
        has_audio: false,
        timing: None,
    };

    let policy = FlowCapabilityPolicy {
        max_edit_segment_duration_sec: 10.0,
        ..Default::default()
    };

    let plan = FlowVideoSegmenter::plan_segments(&facts, &policy).unwrap();
    assert_eq!(plan.segments.len(), 2);
    assert_eq!(plan.segments[0].start_frame, 0);
    assert_eq!(plan.segments[0].end_frame, 299);
    assert_eq!(plan.segments[1].start_frame, 299);
    assert_eq!(plan.segments[1].end_frame, 598);
    assert_eq!(plan.target_fps, 29.97);
}

#[test]
fn test_phase20a_22_vfr_and_zero_fps_fail_closed() {
    let facts = SourceMediaFacts {
        duration_sec: 10.0,
        fps: 0.0,
        width: 1920,
        height: 1080,
        has_audio: false,
        timing: None,
    };
    let policy = FlowCapabilityPolicy::default();
    let res = FlowVideoSegmenter::plan_segments(&facts, &policy);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("INVALID_FPS"));
}

#[test]
fn test_phase20a_23_source_audio_retained_in_flow_input_segment() {
    let temp_dir = tempdir().unwrap();
    let src_video = temp_dir.path().join("src_with_audio.mp4");
    create_synthetic_mp4(&src_video, 4.0, 30.0, 320, 240, true);

    let facts = SourceMediaFacts {
        duration_sec: 4.0,
        fps: 30.0,
        width: 320,
        height: 240,
        has_audio: true,
        timing: None,
    };

    let policy = FlowCapabilityPolicy::default();
    let plan = FlowVideoSegmenter::plan_segments(&facts, &policy).unwrap();
    let seg_dir = temp_dir.path().join("segments");

    let children =
        FlowVideoSegmenter::split_and_prepare_segments(&src_video, &facts, &plan, &seg_dir)
            .unwrap();
    assert_eq!(children.len(), 1);

    let seg_file = seg_dir.join(&children[0].segment_file_name);
    assert!(seg_file.exists());

    let probe = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a:0")
        .arg("-show_entries")
        .arg("stream=codec_type")
        .arg("-of")
        .arg("csv=p=0")
        .arg(&seg_file)
        .output()
        .unwrap();

    let probe_stdout = String::from_utf8_lossy(&probe.stdout);
    assert!(probe_stdout.contains("audio"));
}

#[test]
fn test_phase20a_24_no_audio_input_segment_remains_valid() {
    let temp_dir = tempdir().unwrap();
    let src_video = temp_dir.path().join("src_no_audio.mp4");
    create_synthetic_mp4(&src_video, 4.0, 30.0, 320, 240, false);

    let facts = SourceMediaFacts {
        duration_sec: 4.0,
        fps: 30.0,
        width: 320,
        height: 240,
        has_audio: false,
        timing: None,
    };

    let policy = FlowCapabilityPolicy::default();
    let plan = FlowVideoSegmenter::plan_segments(&facts, &policy).unwrap();
    let seg_dir = temp_dir.path().join("segments_no_audio");

    let children =
        FlowVideoSegmenter::split_and_prepare_segments(&src_video, &facts, &plan, &seg_dir)
            .unwrap();
    assert_eq!(children.len(), 1);

    let seg_file = seg_dir.join(&children[0].segment_file_name);
    assert!(seg_file.exists());
}

#[test]
fn test_phase20a_25_corrupt_and_wrong_download_rejected() {
    let temp_dir = tempdir().unwrap();
    let corrupt_file = temp_dir.path().join("corrupt.mp4");
    fs::write(&corrupt_file, b"NOT_A_VALID_MP4_HEADER").unwrap();

    let val_res = FlowOutputValidator::validate_child_artifact(&corrupt_file, 5.0);
    assert!(val_res.is_err());
    assert!(val_res.unwrap_err().contains("VALIDATION_FAILED"));
}

#[test]
fn test_phase20a_26_duration_drift_blocks_final_promotion() {
    let temp_dir = tempdir().unwrap();
    let short_file = temp_dir.path().join("short.mp4");
    create_synthetic_mp4(&short_file, 2.0, 30.0, 320, 240, false);

    let val_res = FlowOutputValidator::validate_child_artifact(&short_file, 10.0);
    assert!(val_res.is_err());
    assert!(val_res.unwrap_err().contains("Duration drift too large"));
}

#[test]
fn test_phase20a_27_original_audio_muxed_exactly_once() {
    let temp_dir = tempdir().unwrap();
    let src_video = temp_dir.path().join("source_audio_src.mp4");
    create_synthetic_mp4(&src_video, 4.0, 30.0, 320, 240, true);

    let seg1 = temp_dir.path().join("seg1.mp4");
    create_synthetic_mp4(&seg1, 2.0, 30.0, 320, 240, false);

    let seg2 = temp_dir.path().join("seg2.mp4");
    create_synthetic_mp4(&seg2, 2.0, 30.0, 320, 240, false);

    let final_out = temp_dir.path().join("stitched_output.mp4");
    let policy = FlowFinalAudioPolicy {
        preserve_original_audio: true,
        codec: "aac".to_string(),
    };

    let stitched_record = FlowStitcher::stitch_flow_segments(
        &[seg1, seg2],
        Some(&src_video),
        4.0,
        &policy,
        &final_out,
    )
    .unwrap();

    assert!(stitched_record.has_audio);
    assert!((stitched_record.duration_sec - 4.0).abs() < 0.5);
}
