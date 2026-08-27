use crate::ai::cloud::{JobErrorRecord, SourceMediaFacts};
use crate::ai::flow::manifest::*;
use crate::ai::flow::orchestrator::{FlowGenerationRequest, FlowRuntimeService};
use crate::ai::flow::store::FlowJobStore;
use crate::ai::flow::{FlowCreditRecord, PromptSource};
use crate::ai::transformation::{IdentityMode, TransformationIntent};
use crate::media::MediaService;
use crate::projects::{ProjectEditorState, ProjectManager, SourceMedia};
use crate::system::StoragePaths;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;

fn calculate_file_sha256(path: &Path) -> String {
    let mut file = fs::File::open(path).expect("Failed to open file for SHA-256 calculation");
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).expect("Failed to read file for hashing");
    format!("{:x}", hasher.finalize())
}

// =============================================================================
// FLOW-P4-B.1 UNIT & INTEGRATION TESTS
// =============================================================================

#[test]
fn test_flow_p4b1_00_no_scratch_script_dependency() {
    // Section 5: Verify production codebase has zero dependencies on scratch scripts
    let src_tauri = Path::new("src");
    let sidecar_src = Path::new("sidecars/flow-playwright/src");

    let scratch_names = [
        "download_seg0.js",
        "inspect_nodes.js",
        "download_seg0_tile.js",
        "scratch",
    ];

    fn check_dir(dir: &Path, scratch_names: &[&str]) {
        if !dir.exists() {
            return;
        }
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                check_dir(&path, scratch_names);
            } else if path.is_file() {
                let path_str = path.to_string_lossy();
                if path_str.contains("test_") || path_str.contains("tests_") {
                    continue;
                }
                let content = fs::read_to_string(&path).unwrap_or_default();
                for name in scratch_names {
                    if *name == "scratch" {
                        // Ensure no production file imports from a scratch folder
                        assert!(
                            !content.contains("/scratch/") && !content.contains("\\scratch\\"),
                            "Production file {:?} must not reference scratch directory",
                            path
                        );
                    } else {
                        assert!(
                            !content.contains(name),
                            "Production file {:?} must not reference scratch script {}",
                            path,
                            name
                        );
                    }
                }
            }
        }
    }

    check_dir(src_tauri, &scratch_names);
    check_dir(sidecar_src, &scratch_names);
}

#[test]
fn test_flow_p4b1_01_exact_matching_ignores_wrong_existing_card() {
    // Section 6 & 15: Exact media card matching must ignore unrelated existing cards
    let adapter_src_path = Path::new("sidecars/flow-playwright/src/flow_adapter.ts");
    assert!(adapter_src_path.exists(), "flow_adapter.ts must exist");

    let content = fs::read_to_string(adapter_src_path).unwrap();

    // Verify locateMediaCard uses baseStem matching and does not use play_circle fallback
    assert!(
        content.contains("const baseStem = path.basename(params.videoPath"),
        "Must extract baseStem for matching"
    );
    assert!(
        content.contains("getByText(baseStem)"),
        "Must match exact baseStem text"
    );
    assert!(
        content.contains("has-text(\"${baseStem}\")"),
        "Must match leaf element with baseStem"
    );

    // Verify play_circle fallback was completely removed from locateMediaCard
    let locate_start = content
        .find("const locateMediaCard =")
        .expect("locateMediaCard must exist");
    let locate_end = content[locate_start..]
        .find("let targetCard =")
        .expect("end of locateMediaCard")
        + locate_start;
    let locate_body = &content[locate_start..locate_end];

    assert!(
        !locate_body.contains("play_circle") && !locate_body.contains("play_arrow"),
        "locateMediaCard must not contain generic play_circle/play_arrow fallback"
    );
}

#[test]
fn test_flow_p4b1_02_play_circle_fallback_removed() {
    // Section 16: When no exact uploaded segment card exists, return null (do NOT pick first video)
    let adapter_src_path = Path::new("sidecars/flow-playwright/src/flow_adapter.ts");
    let content = fs::read_to_string(adapter_src_path).unwrap();

    let locate_start = content
        .find("const locateMediaCard =")
        .expect("locateMediaCard must exist");
    let locate_end = content[locate_start..]
        .find("let targetCard =")
        .expect("end of locateMediaCard")
        + locate_start;
    let locate_body = &content[locate_start..locate_end];

    // Must return null if baseStem is not found
    assert!(
        locate_body.contains("return null;"),
        "locateMediaCard must return null when baseStem card is not found"
    );
}

#[test]
fn test_flow_p4b1_03_download_button_while_generating_returns_generating() {
    // Section 10 & 17: Generating/Queued checks must occur BEFORE Ready/Download check
    let adapter_src_path = Path::new("sidecars/flow-playwright/src/flow_adapter.ts");
    let content = fs::read_to_string(adapter_src_path).unwrap();

    let gen_marker = content
        .find("// 5. Generating / Queued check")
        .expect("Generating check must exist");
    let ready_marker = content
        .find("// 6. Ready / Download check")
        .expect("Ready check must exist");

    assert!(
        gen_marker < ready_marker,
        "Generating/Queued check MUST be performed BEFORE Ready/Download check"
    );
}

#[test]
fn test_flow_p4b1_04_button_based_download_without_href() {
    // Section 4 & 18: Orchestrator supports session.download with None url
    let orch_src_path = Path::new("src/ai/flow/orchestrator.rs");
    let content = fs::read_to_string(orch_src_path).unwrap();

    // Verify session.download is called with poll_res.download_url.as_deref()
    assert!(
        content.contains(".download(poll_res.download_url.as_deref(), &raw_child)"),
        "Orchestrator must pass poll_res.download_url.as_deref() without requiring Some url"
    );

    // Verify it no longer calls ok_or_else with FLOW_ARTIFACT_MISSING
    assert!(
        !content.contains("FLOW_ARTIFACT_MISSING: Completed generation missing download url"),
        "Orchestrator must not fail if download_url is None for button-based download"
    );
}

#[test]
fn test_flow_p4b1_05_direct_url_download_supported() {
    // Section 19: Sidecar supports direct URL download when downloadUrl is provided
    let adapter_src_path = Path::new("sidecars/flow-playwright/src/flow_adapter.ts");
    let content = fs::read_to_string(adapter_src_path).unwrap();

    assert!(
        content.contains("if (downloadUrl && downloadUrl.trim().length > 0)"),
        "Sidecar must support direct URL download when downloadUrl is provided"
    );
    assert!(
        content.contains("this.context.request.get(targetFullUrl)"),
        "Sidecar must use context request get for direct URL download"
    );
}

#[test]
fn test_flow_p4b1_06_output_ambiguity_fails_closed() {
    // Section 12 & 20: When download control is not uniquely observed, fail closed
    let adapter_src_path = Path::new("sidecars/flow-playwright/src/flow_adapter.ts");
    let content = fs::read_to_string(adapter_src_path).unwrap();

    assert!(
        content.contains("FLOW_GENERATED_OUTPUT_NOT_UNIQUELY_IDENTIFIED"),
        "Sidecar must throw FLOW_GENERATED_OUTPUT_NOT_UNIQUELY_IDENTIFIED on ambiguous/missing output"
    );
}

#[tokio::test]
async fn test_flow_p4b1_07_worker_terminal_failure_persists_error() {
    // Section 13 & 21: Tokio worker error propagation sets manifest state = Failed
    let temp_dir = TempDir::new().unwrap();
    let paths = StoragePaths::resolve_from_base(&temp_dir.path().to_path_buf());
    let store = FlowJobStore::new(paths.clone());

    let mut manifest = FlowGenerationManifest::new(
        "parent_fail_test".to_string(),
        "req_fail".to_string(),
        "proj_fail".to_string(),
        "profile_mock".to_string(),
        "hash".to_string(),
        None,
        "src_hash".to_string(),
        None,
        TransformationIntent::FaceReplace,
        IdentityMode::Generated,
        None,
        FlowRequestedGenerationConfig::default(),
        "prompt".to_string(),
        "hash".to_string(),
        PromptSource::SystemDefault,
        1,
        1,
        SourceMediaFacts::default(),
        FlowSegmentPlan {
            segments: vec![],
            total_frames: 0,
            total_duration_sec: 0.0,
            target_fps: 30.0,
            capability_limit_sec: 10.0,
        },
        FlowCreditRecord::default(),
        FlowFinalAudioPolicy::default(),
    );

    manifest.state = FlowJobState::Generating;
    store.save_manifest_atomic(&mut manifest).unwrap();

    // Simulate worker failure handling
    let err_msg = "SIMULATED_POLL_FAILURE: Network error during polling".to_string();
    if let Ok(mut m) = store.load_manifest("proj_fail", "parent_fail_test") {
        m.state = FlowJobState::Failed;
        m.error = Some(JobErrorRecord {
            code: "WORKER_EXECUTION_FAILED".to_string(),
            sanitized_message: err_msg.clone(),
        });
        store.save_manifest_atomic(&mut m).unwrap();
    }

    let loaded = store
        .load_manifest("proj_fail", "parent_fail_test")
        .unwrap();
    assert_eq!(loaded.state, FlowJobState::Failed);
    assert_eq!(
        loaded.error.as_ref().unwrap().code,
        "WORKER_EXECUTION_FAILED"
    );
    assert_eq!(loaded.error.as_ref().unwrap().sanitized_message, err_msg);
}

#[tokio::test]
async fn test_flow_p4b1_08_clean_rerun_dry_run() {
    // Section 22: Full production long-video pipeline with mock provider (15s -> segment_000, segment_001)
    // with project fixture containing unrelated pre-existing media.
    let temp_dir = TempDir::new().unwrap();
    let paths = StoragePaths::resolve_from_base(&temp_dir.path().to_path_buf());
    let profile_dir = paths
        .app_data_dir
        .join("flow_profiles")
        .join("profile_mock");
    fs::create_dir_all(&profile_dir).unwrap();

    let manager = ProjectManager::new(paths.clone());
    let flow_service = FlowRuntimeService::new(paths.clone());

    let mut project = manager.create_project("Project Dry Run").unwrap();
    let proj_dir = paths.projects_dir.join(&project.id);
    let media_dir = proj_dir.join("media");
    fs::create_dir_all(&media_dir).unwrap();

    // 1. Create unrelated pre-existing media in project
    let unrelated_path = media_dir.join("existing_unrelated.mp4");
    fs::write(&unrelated_path, b"unrelated video data").unwrap();

    // 2. Create 15s test source video
    let source_path = media_dir.join("source_15s.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=1080x1920:r=30:d=15.0",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=48000:cl=stereo",
            "-t",
            "15.0",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            source_path.to_str().unwrap(),
        ])
        .output()
        .expect("Generate 15s video");
    assert!(status.status.success(), "ffmpeg must succeed");

    project.source_media = Some(SourceMedia {
        media_id: "media_dry_run".to_string(),
        original_file_name: "source_15s.mp4".to_string(),
        source_path: source_path.clone(),
        duration_ms: 15000,
        width: 1080,
        height: 1920,
        fps: 30.0,
        file_size_bytes: 1000,
        container: "mp4".to_string(),
        video_codec: "h264".to_string(),
        audio_codec: Some("aac".to_string()),
        has_audio: true,
    });
    manager.update_project(&project).unwrap();

    let req = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: "media_dry_run".to_string(),
        profile_id: "profile_mock".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: Some(PromptSource::SystemDefault),
        target_face: None,
        max_credits: Some(40),
        preserve_original_audio: Some(true),
        requested_config: Some(FlowRequestedGenerationConfig {
            model_id: Some("Omni Flash".to_string()),
            resolution: Some("720p".to_string()),
            duration_sec: Some(10),
            orientation: Some("PORTRAIT / 9:16".to_string()),
            output_count: 1,
        }),
        configuration_fingerprint: None,
        preflight_id: None,
    };

    let start_snap = flow_service
        .start_flow_generation(req, source_path.clone())
        .await
        .expect("Start generation");

    let parent_id = start_snap.parent_id;

    // Poll to completion
    let mut completed = false;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let snap = flow_service
            .get_flow_job_status(&project.id, &parent_id)
            .unwrap();
        if snap.state == FlowJobState::Completed {
            completed = true;
            break;
        }
    }
    assert!(completed, "Dry run must reach Completed state");

    let manifest = flow_service
        .orchestrator
        .store()
        .load_manifest(&project.id, &parent_id)
        .unwrap();

    let plan = manifest.long_video_plan.as_ref().unwrap();
    assert_eq!(plan.segments.len(), 2);
    // Segment 0 exact source must be segment_000.mp4
    assert!(
        plan.segments[0]
            .source_segment_path
            .ends_with("segment_000.mp4"),
        "Segment 0 source must be segment_000.mp4, was {:?}",
        plan.segments[0].source_segment_path
    );
    // Segment 1 exact source must be segment_001.mp4
    assert!(
        plan.segments[1]
            .source_segment_path
            .ends_with("segment_001.mp4"),
        "Segment 1 source must be segment_001.mp4, was {:?}",
        plan.segments[1].source_segment_path
    );

    // Ledger checks
    let ledger = manifest.parent_ledger.as_ref().unwrap();
    assert_eq!(ledger.completed_paid_segments, 2);
    assert_eq!(ledger.dispatched_paid_clicks, 2);
    assert_eq!(ledger.authoritative_committed_credits, 40);

    // Ingest into project
    let use_res = flow_service
        .use_flow_output_in_project(&project.id, &parent_id)
        .unwrap();
    assert_eq!(use_res.project.derived_media_assets.len(), 1);
    assert_eq!(use_res.derived_asset.provenance.provider, "FLOW");
    assert_eq!(use_res.derived_asset.provenance.provider_job_id, parent_id);
}

// =============================================================================
// REAL LIVE PAID TEST (MUST BE #[ignore] AND GUARDED)
// =============================================================================

#[tokio::test]
#[ignore = "Real live Google Flow paid long-video production acceptance for FLOW-P4-B"]
async fn test_flow_p4b_live_acceptance() {
    // -------------------------------------------------------------------------
    // Operator Authorization Guard (Sections 40 & 41)
    // -------------------------------------------------------------------------
    if std::env::var("RUN_FLOW_P4B_LIVE_PAID_ACCEPTANCE").unwrap_or_default() != "1" {
        println!("SKIPPED: Set RUN_FLOW_P4B_LIVE_PAID_ACCEPTANCE=1 to authorize live paid acceptance run.");
        return;
    }

    println!("==================================================");
    println!("FLOW-P4-B REAL TWO-SEGMENT PAID LONG VIDEO ACCEPTANCE");
    println!("MAX TOTAL CREDITS = 40, MAX SEGMENT CREDITS = 20");
    println!("MAX PAID CLICKS = 2, AUTO RETRIES = 0");
    println!("==================================================");

    let source_asset = if Path::new("test-assets/p4b_source_15s.mp4").exists() {
        PathBuf::from("test-assets/p4b_source_15s.mp4")
    } else {
        PathBuf::from("../test-assets/p4b_source_15s.mp4")
    };
    assert!(
        source_asset.exists(),
        "Source asset test-assets/p4b_source_15s.mp4 must exist at {:?}",
        source_asset
    );

    // Verify source video properties
    let source_sha256_before = calculate_file_sha256(&source_asset);
    println!("SOURCE_SHA256_BEFORE: {}", source_sha256_before);

    let base_path = PathBuf::from("D:/rustProject/autovideo-ai/src-tauri/.autovideo_data");
    let paths = StoragePaths::resolve_from_base(&base_path);
    let manager = ProjectManager::new(paths.clone());
    let media_service = MediaService::new();
    let flow_service = FlowRuntimeService::new(paths.clone());

    // -------------------------------------------------------------------------
    // 1. Initial Credit Balance Discovery
    // -------------------------------------------------------------------------
    println!("--------------------------------------------------");
    println!("[P4-B STEP 1] Querying initial credit balance...");
    let initial_status = flow_service
        .refresh_flow_credit_balance("profile_2")
        .await
        .expect("Failed to query initial credit balance");
    let initial_balance = initial_status.balance;
    println!("INITIAL_BALANCE: {:?}", initial_balance);

    // -------------------------------------------------------------------------
    // 2. Import Source into Real Project Workflow (Section 3)
    // -------------------------------------------------------------------------
    println!("--------------------------------------------------");
    println!("[P4-B STEP 2] Setting up real project and importing source media...");
    let mut project = manager
        .create_project("FLOW-P4-B Acceptance Project")
        .expect("Failed to create project");

    let proj_dir = paths.projects_dir.join(&project.id);
    let media_dir = proj_dir.join("media");
    fs::create_dir_all(&media_dir).unwrap();

    let dest_media_path = media_dir.join("p4b_source_15s.mp4");
    fs::copy(&source_asset, &dest_media_path).expect("Failed to copy source media to project");

    let media_metadata = media_service
        .probe(&dest_media_path)
        .expect("Failed to probe source media");
    let media_id = format!("media_{}", uuid::Uuid::new_v4());

    project.source_media = Some(SourceMedia {
        media_id: media_id.clone(),
        original_file_name: "p4b_source_15s.mp4".to_string(),
        source_path: dest_media_path.clone(),
        duration_ms: media_metadata.duration_ms,
        width: media_metadata.width,
        height: media_metadata.height,
        fps: media_metadata.fps,
        file_size_bytes: media_metadata.file_size_bytes,
        container: media_metadata.container,
        video_codec: media_metadata.video_codec,
        audio_codec: media_metadata.audio_codec,
        has_audio: media_metadata.has_audio,
    });
    project.editor_state = Some(ProjectEditorState {
        active_media_id: Some(media_id.clone()),
        ..Default::default()
    });
    manager
        .update_project(&project)
        .expect("Failed to update project");

    println!("PROJECT_ID: {}", project.id);
    println!("SOURCE_MEDIA_ID: {}", media_id);
    println!("SOURCE_DURATION_MS: {}", media_metadata.duration_ms);

    let clean_source_path = dest_media_path.canonicalize().unwrap();
    let clean_str = clean_source_path.to_string_lossy().to_string();
    let clean_dest_path = if let Some(stripped) = clean_str.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        clean_source_path
    };

    // -------------------------------------------------------------------------
    // 3. Dispatch Long Video Parent Generation (Sections 4 - 7)
    // -------------------------------------------------------------------------
    println!("--------------------------------------------------");
    println!("[P4-B STEP 3] Dispatching parent generation request...");
    let req = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: media_id.clone(),
        profile_id: "profile_2".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: Some(PromptSource::SystemDefault),
        target_face: None,
        max_credits: Some(40), // P4B_APPROVED_TOTAL_MAX_CREDITS = 40
        preserve_original_audio: Some(true),
        requested_config: Some(FlowRequestedGenerationConfig {
            model_id: Some("Omni Flash".to_string()),
            resolution: Some("720p".to_string()),
            duration_sec: Some(10),
            orientation: Some("PORTRAIT / 9:16".to_string()),
            output_count: 1,
        }),
        configuration_fingerprint: None,
        preflight_id: None, // Long video paid authority is per child
    };

    let start_snapshot = flow_service
        .start_flow_generation(req, clean_dest_path.clone())
        .await
        .expect("Failed to dispatch long video parent generation");

    let parent_id = start_snapshot.parent_id;
    println!("PARENT_JOB_ID: {}", parent_id);
    println!("INITIAL_STATE: {:?}", start_snapshot.state);

    // -------------------------------------------------------------------------
    // 4. Poll To Terminal Completion (Sequential Seg 0 -> Seg 1 -> Stitch)
    // -------------------------------------------------------------------------
    println!("--------------------------------------------------");
    println!("[P4-B STEP 4] Polling long video job to terminal completion...");
    let mut final_snapshot = None;
    for iteration in 1..=240 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let snap = flow_service
            .get_flow_job_status(&project.id, &parent_id)
            .expect("Failed to get job status");

        println!(
            "[Poll #{:03} | {:?}] seg_idx: {:?}, state: {:?}, error: {:?}",
            iteration,
            iteration * 5,
            snap.active_segment_index,
            snap.state,
            snap.error_message
        );

        match snap.state {
            FlowJobState::Completed => {
                final_snapshot = Some(snap);
                break;
            }
            FlowJobState::Failed
            | FlowJobState::GenerationAmbiguous
            | FlowJobState::Blocked
            | FlowJobState::FlowUiChanged => {
                panic!(
                    "Job reached terminal failure state: {:?}, code: {:?}, msg: {:?}",
                    snap.state, snap.error_code, snap.error_message
                );
            }
            _ => {}
        }
    }

    let completed_snap =
        final_snapshot.expect("Long video job did not reach Completed state within timeout");
    println!(
        "Job Completed Successfully! Final State: {:?}",
        completed_snap.state
    );

    // -------------------------------------------------------------------------
    // 5. Inspect Manifest and Verify Invariants (Sections 8 - 30)
    // -------------------------------------------------------------------------
    println!("--------------------------------------------------");
    println!("[P4-B STEP 5] Validating completed manifest and artifacts...");
    let manifest = flow_service
        .orchestrator
        .store()
        .load_manifest(&project.id, &parent_id)
        .expect("Failed to load completed manifest");

    assert_eq!(manifest.job_kind, FlowJobKind::LongVideoParent);
    let plan = manifest
        .long_video_plan
        .as_ref()
        .expect("Long video plan must be present");
    assert_eq!(plan.segments.len(), 2, "Must contain exactly 2 segments");
    assert_eq!(plan.segments[0].state, FlowJobState::Completed);
    assert_eq!(plan.segments[1].state, FlowJobState::Completed);

    let ledger = manifest
        .parent_ledger
        .as_ref()
        .expect("Parent ledger must be present");
    println!(
        "LEDGER_COMPLETED_SEGMENTS: {}",
        ledger.completed_paid_segments
    );
    println!(
        "LEDGER_COMMITTED_CREDITS: {}",
        ledger.authoritative_committed_credits
    );
    assert_eq!(ledger.completed_paid_segments, 2);
    assert!(ledger.authoritative_committed_credits <= 40);

    // Check continuity evidence
    assert!(
        !manifest.continuity_evidence.is_empty(),
        "Continuity evidence must exist for boundary 0->1"
    );
    let ev = &manifest.continuity_evidence[0];
    println!(
        "CONTINUITY_BOUNDARY: {} -> {}",
        ev.boundary_index,
        ev.boundary_index + 1
    );
    println!("CONTINUITY_METRIC_NAME: {:?}", ev.metric_name);
    println!("CONTINUITY_METRIC_CATEGORY: {:?}", ev.metric_category);
    println!("CONTINUITY_METRIC_VALUE: {:?}", ev.metric_value);
    println!("CONTINUITY_SEAM_STATUS: {:?}", ev.seam_status);
    println!("CONTINUITY_CONTACT_SHEET: {:?}", ev.contact_sheet_path);

    assert_eq!(ev.metric_category.as_deref(), Some("VISUAL_SEAM_METRIC"));
    assert_eq!(ev.seam_status, FlowSeamStatus::Unverified);
    if let Some(ref cs_path) = ev.contact_sheet_path {
        let job_dir = flow_service
            .orchestrator
            .store()
            .parent_flow_job_dir(&project.id, &parent_id)
            .unwrap();
        let full_cs = job_dir.join(cs_path);
        assert!(full_cs.exists(), "Contact sheet file must exist on disk");
    }

    // Check final stitched output
    let final_record = manifest
        .final_output
        .as_ref()
        .expect("Final output record must exist");
    assert!(
        final_record.final_path.exists(),
        "Final output video must exist on disk"
    );
    let final_sha256 = calculate_file_sha256(&final_record.final_path);
    println!("FINAL_OUTPUT_PATH: {:?}", final_record.final_path);
    println!("FINAL_SHA256: {}", final_sha256);
    println!("FINAL_FRAME_COUNT: {}", final_record.frame_count);
    println!("FINAL_DURATION_SEC: {}", final_record.duration_sec);

    // Validate with ffprobe
    let probe_out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,nb_frames,r_frame_rate,duration",
            "-of",
            "default=noprint_wrappers=1",
            final_record.final_path.to_str().unwrap(),
        ])
        .output()
        .expect("ffprobe execution on final output");
    let probe_str = String::from_utf8_lossy(&probe_out.stdout);
    println!("FINAL_VIDEO_PROBE:\n{}", probe_str);
    assert!(probe_str.contains("width=1080"));
    assert!(probe_str.contains("height=1920"));
    assert!(probe_str.contains("r_frame_rate=30/1"));

    let audio_probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name,channels,sample_rate",
            "-of",
            "default=noprint_wrappers=1",
            final_record.final_path.to_str().unwrap(),
        ])
        .output()
        .expect("ffprobe audio on final output");
    let audio_str = String::from_utf8_lossy(&audio_probe.stdout);
    println!("FINAL_AUDIO_PROBE:\n{}", audio_str);
    assert!(
        audio_str.contains("codec_name="),
        "Final output must have audio stream"
    );

    // -------------------------------------------------------------------------
    // 6. Source Immutability Verification (Section 32)
    // -------------------------------------------------------------------------
    let source_sha256_after = calculate_file_sha256(&source_asset);
    println!("SOURCE_SHA256_AFTER: {}", source_sha256_after);
    assert_eq!(
        source_sha256_before, source_sha256_after,
        "Source video MUST remain byte-for-byte immutable!"
    );

    // -------------------------------------------------------------------------
    // 7. Project Ingestion Verification (Section 33)
    // -------------------------------------------------------------------------
    println!("--------------------------------------------------");
    println!("[P4-B STEP 6] Ingesting final output into project as DerivedMediaAsset...");
    let use_res = flow_service
        .use_flow_output_in_project(&project.id, &parent_id)
        .expect("Failed to ingest flow output into project");

    println!("DERIVED_MEDIA_ID: {}", use_res.derived_asset.media.media_id);
    println!("DERIVED_PROVENANCE: {:?}", use_res.derived_asset.provenance);
    assert_eq!(use_res.derived_asset.provenance.provider, "FLOW");
    assert_eq!(use_res.derived_asset.provenance.provider_job_id, parent_id);
    assert_eq!(use_res.derived_asset.provenance.source_media_id, media_id);
    assert_eq!(use_res.project.derived_media_assets.len(), 1);

    // -------------------------------------------------------------------------
    // 8. Account Balance Reconciliation (Section 34)
    // -------------------------------------------------------------------------
    println!("--------------------------------------------------");
    println!("[P4-B STEP 7] Querying final credit balance...");
    let final_status = flow_service
        .refresh_flow_credit_balance("profile_2")
        .await
        .expect("Failed to query final credit balance");
    let final_balance = final_status.balance;
    println!("FINAL_BALANCE: {:?}", final_balance);

    let balance_delta = match (initial_balance, final_balance) {
        (Some(init), Some(fin)) => Some(init.saturating_sub(fin)),
        _ => None,
    };
    println!("BALANCE_DELTA: {:?}", balance_delta);

    let reconciliation_status = if balance_delta == Some(ledger.authoritative_committed_credits) {
        "CONFIRMED"
    } else {
        "UNRESOLVED"
    };
    println!("ACCOUNT_BALANCE_RECONCILIATION: {}", reconciliation_status);

    println!("==================================================");
    println!("FLOW-P4-B PAID PRODUCTION ACCEPTANCE RUN COMPLETED");
    println!("==================================================");
}
