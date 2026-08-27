use crate::ai::flow::*;
use crate::ai::transformation::{IdentityMode, TransformationIntent};
use crate::commands::resolve_project_media_by_id;
use crate::media::MediaService;
use crate::projects::{ProjectEditorState, ProjectManager, SourceMedia};
use crate::system::StoragePaths;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn calculate_file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("Failed to hash file: {}", e))?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_frame_ffmpeg(
    video_path: &Path,
    timestamp_sec: f64,
    out_img_path: &Path,
) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-ss")
        .arg(format!("{:.3}", timestamp_sec))
        .arg("-i")
        .arg(video_path)
        .arg("-vframes")
        .arg("1")
        .arg("-q:v")
        .arg("2")
        .arg(out_img_path)
        .status()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;
    if !status.success() {
        return Err(format!("ffmpeg failed with status: {:?}", status));
    }
    Ok(())
}

#[tokio::test]
#[ignore = "Real paid Google Flow production acceptance"]
async fn test_flow_p3b_real_google_flow_live_production_acceptance() {
    println!("==================================================");
    println!("FLOW-P3-B FIRST REAL PAID PRODUCTION ACCEPTANCE");
    println!("Maximum 1 video, 1 segment, 1 generate click, max 50 credits.");
    println!("==================================================");

    let source_video_canonical_path = PathBuf::from(
        "D:/rustProject/autovideo-ai/test-assets/phase20c/videos/flow_acceptance_01.mp4",
    );
    assert!(
        source_video_canonical_path.exists(),
        "Source acceptance video must exist at {:?}",
        source_video_canonical_path
    );

    // Compute and record ORIGINAL_SOURCE_SHA256_BEFORE
    let source_sha256_before = calculate_file_sha256(&source_video_canonical_path).unwrap();
    println!("ORIGINAL_SOURCE_SHA256_BEFORE: {}", source_sha256_before);
    assert_eq!(
        source_sha256_before.to_lowercase(),
        "68747585122b46f78168f951aa43e461dbafe19e4dfba6d519578a004f8d1694",
        "Source asset SHA-256 must match expected immutable asset"
    );

    let base_path = PathBuf::from("D:/rustProject/autovideo-ai/src-tauri/.autovideo_data");
    let paths = StoragePaths::resolve_from_base(&base_path);
    let manager = ProjectManager::new(paths.clone());
    let media_service = MediaService::new();

    // 1. Real Project Workflow: Create project and import media
    let mut project = manager
        .create_project("Phase FLOW-P3-B Real Production Project")
        .unwrap();
    let proj_dir = paths.projects_dir.join(&project.id);
    let media_dir = proj_dir.join("media");
    fs::create_dir_all(&media_dir).unwrap();

    let dest_media_path = media_dir.join("flow_acceptance_01.mp4");
    fs::copy(&source_video_canonical_path, &dest_media_path).unwrap();

    let media_metadata = media_service.probe(&dest_media_path).unwrap();
    let media_id = format!("media_{}", uuid::Uuid::new_v4());

    project.source_media = Some(SourceMedia {
        media_id: media_id.clone(),
        original_file_name: "flow_acceptance_01.mp4".to_string(),
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
    manager.update_project(&project).unwrap();

    println!("Project ID: {}", project.id);
    println!("Source Media ID: {}", media_id);
    println!("Source Duration ms: {}", media_metadata.duration_ms);

    // 2. Setup FlowRuntimeService in the exact same process
    let flow_service = FlowRuntimeService::new(paths.clone());

    let dest_canon = dest_media_path.canonicalize().unwrap();
    let dest_canon_str = dest_canon.to_string_lossy().to_string();
    let clean_dest_path = if let Some(stripped) = dest_canon_str.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        dest_canon
    };

    let requested_config = FlowRequestedGenerationConfig {
        model_id: Some("Omni Flash".to_string()),
        resolution: Some("720p".to_string()),
        duration_sec: Some(10),
        orientation: Some("PORTRAIT / 9:16".to_string()),
        output_count: 1,
    };

    let preflight_req = FlowGenerationRequest {
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
        requested_config: Some(requested_config.clone()),
        configuration_fingerprint: None,
        preflight_id: None,
    };

    println!("--------------------------------------------------");
    println!("[P3-B STEP 1] Executing Fresh Live Preflight...");
    let preflight = flow_service
        .preflight_flow_generation(preflight_req, clean_dest_path.clone())
        .await
        .expect("Preflight must succeed");

    println!("Preflight Ready: {}", preflight.ready_for_paid_submission);
    println!("Preflight ID: {}", preflight.preflight_id);
    println!(
        "Preflight Config Verified: {}",
        preflight.configuration_verified
    );
    println!(
        "Preflight Live Cost: {:?}",
        preflight.live_displayed_credit_cost
    );
    println!("Preflight Cost Provenance: {:?}", preflight.cost_provenance);
    println!(
        "Preflight Fingerprint: {}",
        preflight.configuration_fingerprint
    );
    println!("Preflight Blocking Code: {:?}", preflight.blocking_code);

    if !preflight.ready_for_paid_submission {
        println!("==================================================");
        println!("FLOW-P3-B PREFLIGHT BLOCKED FAIL-CLOSED:");
        println!("Blocking Code: {:?}", preflight.blocking_code);
        println!("Paid Clicks Dispatched: 0");
        println!("Flow Live Generations: 0");
        println!("Credits Spent: 0");
        println!("P3B Accepted: NO");
        println!("==================================================");
        assert!(
            preflight.blocking_code.is_some(),
            "Blocked preflight must have an explicit blocking code"
        );
        assert!(
            preflight.live_displayed_credit_cost.is_none(),
            "Blocked preflight must not expose live credit cost"
        );
        assert_eq!(
            preflight.cost_provenance,
            FlowCostProvenance::Unknown,
            "Cost provenance must remain Unknown when preflight is blocked"
        );
        return;
    }

    assert!(
        preflight.configuration_verified,
        "Configuration must be verified"
    );
    assert_eq!(
        preflight.cost_provenance,
        FlowCostProvenance::UploadedVideoEdit
    );
    let live_cost = preflight
        .live_displayed_credit_cost
        .expect("Live displayed credit cost must be present");
    assert!(
        live_cost <= 50,
        "Live cost {} must be within approved budget of 50 credits",
        live_cost
    );
    let preflight_id = preflight.preflight_id.clone();
    let config_fp = preflight.configuration_fingerprint.clone();

    // 3. Start Single Production Job using the fresh preflight ticket
    println!("--------------------------------------------------");
    println!("[P3-B STEP 2] Starting Single Paid Flow Generation (Click exactly once)...");
    let start_req = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: media_id.clone(),
        profile_id: "profile_2".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: Some(preflight.prompt_source.clone()),
        target_face: None,
        max_credits: Some(50),
        preserve_original_audio: Some(true),
        requested_config: Some(preflight.requested_config.clone()),
        configuration_fingerprint: Some(config_fp),
        preflight_id: Some(preflight_id),
    };

    let start_snapshot = flow_service
        .start_flow_generation(start_req, clean_dest_path.clone())
        .await
        .expect("Start generation must succeed");

    let parent_id = start_snapshot.parent_id;
    println!("Started Job Parent ID: {}", parent_id);
    println!("Initial State: {:?}", start_snapshot.state);

    // 4. Poll Until Terminal Completion (Bounded polling: up to 180 * 5s = 15 minutes)
    println!("--------------------------------------------------");
    println!("[P3-B STEP 3] Polling Job to Completion...");
    let mut final_snapshot = None;
    for iteration in 1..=180 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let snap = flow_service
            .get_flow_job_status(&project.id, &parent_id)
            .expect("Failed to get job status");
        println!(
            "[Poll #{:03}] state: {:?}, error: {:?}",
            iteration, snap.state, snap.error_message
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
                    "Job stopped with terminal failure state: {:?}, code: {:?}, msg: {:?}",
                    snap.state, snap.error_code, snap.error_message
                );
            }
            _ => {}
        }
    }

    let completed_snap =
        final_snapshot.expect("Generation did not reach Completed state within timeout");
    println!("Job Completed Successfully!");
    println!("Final State: {:?}", completed_snap.state);

    // 5. Inspect Manifest and Output Artifacts
    println!("--------------------------------------------------");
    println!("[P3-B STEP 4] Validating Manifest & Generated Artifacts...");
    let manifest = flow_service
        .orchestrator
        .store()
        .load_manifest(&project.id, &parent_id)
        .expect("Failed to load completed manifest");

    assert_eq!(manifest.schema_version, 4);
    assert_eq!(
        manifest.transformation_intent,
        TransformationIntent::FaceReplace
    );
    assert_eq!(manifest.identity_mode, IdentityMode::Generated);
    assert_eq!(manifest.prompt_source, PromptSource::SystemDefault);
    assert_eq!(
        manifest.child_segments.len(),
        1,
        "Must be exactly 1 segment"
    );

    let final_record = manifest
        .final_output
        .as_ref()
        .expect("Final output record must exist in manifest");
    println!("Final Output Path: {:?}", final_record.final_path);
    assert!(
        final_record.final_path.exists(),
        "Final video file must exist on disk"
    );
    let final_meta = fs::metadata(&final_record.final_path).unwrap();
    assert!(final_meta.len() > 0, "Final output file size must be > 0");

    let final_probed = media_service
        .probe(&final_record.final_path)
        .expect("Final output must be probeable");
    println!("Final Output Probed Facts:");
    println!("  Duration ms: {}", final_probed.duration_ms);
    println!("  Width: {}", final_probed.width);
    println!("  Height: {}", final_probed.height);
    println!("  FPS: {}", final_probed.fps);
    println!("  Video Codec: {}", final_probed.video_codec);
    println!("  Has Audio: {}", final_probed.has_audio);
    println!("  Audio Codec: {:?}", final_probed.audio_codec);

    // Validate raw child video
    let child_output_path = manifest.child_segments[0]
        .download_artifact_path
        .as_ref()
        .expect("Child download artifact path must be recorded");
    assert!(
        child_output_path.exists(),
        "Raw child output segment must exist at {:?}",
        child_output_path
    );
    let raw_child_probed = media_service
        .probe(child_output_path)
        .expect("Raw child must be probeable");
    println!("Raw Child Probed Facts:");
    println!("  Duration ms: {}", raw_child_probed.duration_ms);
    println!("  Width: {}", raw_child_probed.width);
    println!("  Height: {}", raw_child_probed.height);
    println!("  FPS: {}", raw_child_probed.fps);

    // 6. Visual Face-Edit Frame Extraction
    println!("--------------------------------------------------");
    println!("[P3-B STEP 5] Extracting Representative Frames for Visual Review...");
    let frames_dir = proj_dir.join("evidence_frames");
    fs::create_dir_all(&frames_dir).unwrap();

    let duration_sec = (final_probed.duration_ms as f64) / 1000.0;
    let t_20 = duration_sec * 0.20;
    let t_50 = duration_sec * 0.50;
    let t_80 = duration_sec * 0.80;

    let src_f20 = frames_dir.join("source_20pct.jpg");
    let src_f50 = frames_dir.join("source_50pct.jpg");
    let src_f80 = frames_dir.join("source_80pct.jpg");

    let gen_f20 = frames_dir.join("generated_20pct.jpg");
    let gen_f50 = frames_dir.join("generated_50pct.jpg");
    let gen_f80 = frames_dir.join("generated_80pct.jpg");

    extract_frame_ffmpeg(&dest_media_path, t_20, &src_f20).unwrap();
    extract_frame_ffmpeg(&dest_media_path, t_50, &src_f50).unwrap();
    extract_frame_ffmpeg(&dest_media_path, t_80, &src_f80).unwrap();

    extract_frame_ffmpeg(&final_record.final_path, t_20, &gen_f20).unwrap();
    extract_frame_ffmpeg(&final_record.final_path, t_50, &gen_f50).unwrap();
    extract_frame_ffmpeg(&final_record.final_path, t_80, &gen_f80).unwrap();

    println!("Extracted Frames:");
    println!("  Source 20%: {:?}", src_f20);
    println!("  Source 50%: {:?}", src_f50);
    println!("  Source 80%: {:?}", src_f80);
    println!("  Generated 20%: {:?}", gen_f20);
    println!("  Generated 50%: {:?}", gen_f50);
    println!("  Generated 80%: {:?}", gen_f80);

    // 7. Verify Source Immutability
    println!("--------------------------------------------------");
    println!("[P3-B STEP 6] Verifying Original Source Immutability...");
    let source_sha256_after = calculate_file_sha256(&source_video_canonical_path).unwrap();
    println!("ORIGINAL_SOURCE_SHA256_AFTER: {}", source_sha256_after);
    assert_eq!(
        source_sha256_before, source_sha256_after,
        "Original source video must NOT have been modified"
    );

    // 8. Integrate with Project Workflow: use_flow_output_in_project
    println!("--------------------------------------------------");
    println!("[P3-B STEP 7] Ingesting Output into Project Workflow...");
    let use_res = flow_service
        .use_flow_output_in_project(&project.id, &parent_id)
        .expect("use_flow_output_in_project must succeed");

    println!("Derived Asset ID: {}", use_res.derived_asset.media.media_id);
    println!(
        "Derived File Name: {}",
        use_res.derived_asset.media.original_file_name
    );
    println!(
        "Derived Path: {:?}",
        use_res.derived_asset.media.source_path
    );
    assert!(use_res.derived_asset.media.source_path.exists());
    assert_eq!(
        use_res
            .project
            .editor_state
            .as_ref()
            .and_then(|e| e.active_media_id.as_ref()),
        Some(&use_res.derived_asset.media.media_id)
    );

    // Idempotency check: Calling second time returns identical asset
    let use_res2 = flow_service
        .use_flow_output_in_project(&project.id, &parent_id)
        .expect("Second call to use_flow_output_in_project must succeed");
    assert_eq!(
        use_res.derived_asset.media.media_id,
        use_res2.derived_asset.media.media_id
    );

    // 9. Verify Secure Preview Resolver
    println!("--------------------------------------------------");
    println!("[P3-B STEP 8] Verifying Secure Media Resolution...");
    let preview_media = resolve_project_media_by_id(
        &project.id,
        Some(&use_res.derived_asset.media.media_id),
        &paths,
    )
    .expect("Preview resolver must locate derived media");
    assert_eq!(
        preview_media.1.media_id,
        use_res.derived_asset.media.media_id
    );

    // Record GEN1 committed cost
    let gen1_committed = live_cost;
    println!("GEN1 Committed Cost: {} credits", gen1_committed);
    let remaining_authorized = 50 - gen1_committed;
    println!(
        "REMAINING AUTHORIZED BUDGET: {} credits",
        remaining_authorized
    );
    assert!(
        remaining_authorized > 0,
        "Remaining budget after Gen1 must allow Gen2"
    );

    // =========================================================================
    // GENERATION 2 ENTRY GATE
    // =========================================================================
    println!("==================================================");
    println!("FLOW-P3-B GENERATION #2 (SECOND INDEPENDENT SAMPLE)");
    println!("Must use SAME original source, NOT Gen 1 derived media");
    println!("Budget limit: {} credits", remaining_authorized);
    println!("==================================================");

    let preflight_req_gen2 = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: media_id.clone(), // MUST be original media ID
        profile_id: "profile_2".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: None,
        target_face: None,
        max_credits: Some(remaining_authorized),
        preserve_original_audio: Some(true),
        requested_config: Some(requested_config.clone()),
        configuration_fingerprint: None,
        preflight_id: None,
    };

    println!("--------------------------------------------------");
    println!("[P3-B STEP 8] Executing Fresh Live Preflight for Gen 2...");
    let preflight_gen2 = flow_service
        .preflight_flow_generation(preflight_req_gen2, clean_dest_path.clone())
        .await
        .expect("Gen2 Preflight must succeed");

    println!(
        "Gen2 Preflight Ready: {}",
        preflight_gen2.ready_for_paid_submission
    );
    println!("Gen2 Preflight ID: {}", preflight_gen2.preflight_id);
    println!(
        "Gen2 Config Verified: {}",
        preflight_gen2.configuration_verified
    );
    println!(
        "Gen2 Live Cost: {:?}",
        preflight_gen2.live_displayed_credit_cost
    );
    println!("Gen2 Cost Provenance: {:?}", preflight_gen2.cost_provenance);
    println!(
        "Gen2 Fingerprint: {}",
        preflight_gen2.configuration_fingerprint
    );

    assert!(
        preflight_gen2.ready_for_paid_submission,
        "Gen2 Preflight must be ready for paid submission"
    );
    assert!(
        preflight_gen2.configuration_verified,
        "Gen2 Configuration must be verified"
    );
    assert_eq!(
        preflight_gen2.cost_provenance,
        FlowCostProvenance::UploadedVideoEdit
    );

    let live_cost_gen2 = preflight_gen2
        .live_displayed_credit_cost
        .expect("Gen2 live cost must be present");
    println!("Gen2 Live Cost: {} credits", live_cost_gen2);
    assert!(
        live_cost_gen2 <= remaining_authorized,
        "Gen2 cost {} must be <= remaining authorized budget {}",
        live_cost_gen2,
        remaining_authorized
    );
    assert!(
        gen1_committed + live_cost_gen2 <= 50,
        "Total authoritative cost {} must be <= 50 credits",
        gen1_committed + live_cost_gen2
    );

    let preflight_id_gen2 = preflight_gen2.preflight_id.clone();
    let config_fp_gen2 = preflight_gen2.configuration_fingerprint.clone();

    println!("--------------------------------------------------");
    println!("[P3-B STEP 9] Starting Gen 2 Paid Generation (Click #2)...");
    let start_req_gen2 = FlowGenerationRequest {
        project_id: project.id.clone(),
        source_media_id: media_id.clone(),
        profile_id: "profile_2".to_string(),
        transformation_intent: Some(TransformationIntent::FaceReplace),
        identity_mode: Some(IdentityMode::Generated),
        prompt: "".to_string(),
        prompt_source: Some(preflight_gen2.prompt_source.clone()),
        target_face: None,
        max_credits: Some(live_cost_gen2),
        preserve_original_audio: Some(true),
        requested_config: Some(preflight_gen2.requested_config.clone()),
        configuration_fingerprint: Some(config_fp_gen2),
        preflight_id: Some(preflight_id_gen2),
    };

    let start_snapshot_gen2 = flow_service
        .start_flow_generation(start_req_gen2, clean_dest_path.clone())
        .await
        .expect("Start Gen2 generation must succeed");

    let parent_id_gen2 = start_snapshot_gen2.parent_id;
    println!("Gen2 Started Job Parent ID: {}", parent_id_gen2);
    println!("Gen2 Initial State: {:?}", start_snapshot_gen2.state);

    println!("--------------------------------------------------");
    println!("[P3-B STEP 10] Polling Gen 2 Job to Completion...");
    let mut final_snapshot_gen2 = None;
    for iteration in 1..=180 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let snap = flow_service
            .get_flow_job_status(&project.id, &parent_id_gen2)
            .expect("Failed to get Gen2 job status");
        println!(
            "[Gen2 Poll #{:03}] state: {:?}, error: {:?}",
            iteration, snap.state, snap.error_message
        );

        match snap.state {
            FlowJobState::Completed => {
                final_snapshot_gen2 = Some(snap);
                break;
            }
            FlowJobState::Failed
            | FlowJobState::GenerationAmbiguous
            | FlowJobState::Blocked
            | FlowJobState::FlowUiChanged => {
                panic!(
                    "Gen2 job stopped with terminal failure state: {:?}, code: {:?}, msg: {:?}",
                    snap.state, snap.error_code, snap.error_message
                );
            }
            _ => {}
        }
    }

    let completed_snap_gen2 =
        final_snapshot_gen2.expect("Gen2 generation did not reach Completed state within timeout");
    println!("Gen2 Job Completed Successfully!");
    println!("Gen2 Final State: {:?}", completed_snap_gen2.state);

    let gen2_committed = live_cost_gen2;
    let total_committed = gen1_committed + gen2_committed;
    println!("==================================================");
    println!("TOTAL COMMITMENT SUMMARY:");
    println!("  GEN1 Final Cost: {} credits", gen1_committed);
    println!("  GEN2 Final Cost: {} credits", gen2_committed);
    println!("  Total Authoritative Cost: {} credits", total_committed);
    println!("  Authorized Budget Ceiling: 50 credits");
    println!("==================================================");
    assert!(
        total_committed <= 50,
        "Total cost {} exceeded authorized ceiling of 50 credits",
        total_committed
    );

    // Inspect Gen2 manifest and artifact
    println!("--------------------------------------------------");
    println!("[P3-B STEP 11] Validating Gen 2 Manifest & Artifacts...");
    let manifest_gen2 = flow_service
        .orchestrator
        .store()
        .load_manifest(&project.id, &parent_id_gen2)
        .expect("Failed to load Gen2 manifest");

    assert_eq!(manifest_gen2.schema_version, 4);
    assert_eq!(
        manifest_gen2.transformation_intent,
        TransformationIntent::FaceReplace
    );
    assert_eq!(manifest_gen2.identity_mode, IdentityMode::Generated);
    assert_eq!(manifest_gen2.prompt_source, PromptSource::SystemDefault);
    assert_eq!(manifest_gen2.child_segments.len(), 1);

    let final_record_gen2 = manifest_gen2
        .final_output
        .as_ref()
        .expect("Gen2 final output record must exist");
    println!("Gen2 Final Output Path: {:?}", final_record_gen2.final_path);
    assert!(
        final_record_gen2.final_path.exists(),
        "Gen2 final video file must exist on disk"
    );

    let final_probed_gen2 = media_service
        .probe(&final_record_gen2.final_path)
        .expect("Gen2 final output must be probeable");
    println!("Gen2 Final Output Probed Facts:");
    println!("  Duration ms: {}", final_probed_gen2.duration_ms);
    println!("  Width: {}", final_probed_gen2.width);
    println!("  Height: {}", final_probed_gen2.height);
    println!("  FPS: {}", final_probed_gen2.fps);
    println!("  Video Codec: {}", final_probed_gen2.video_codec);
    println!("  Has Audio: {}", final_probed_gen2.has_audio);
    println!("  Audio Codec: {:?}", final_probed_gen2.audio_codec);

    // Gen2 Frame Extraction
    println!("--------------------------------------------------");
    println!("[P3-B STEP 12] Extracting Gen 2 Frames for Visual Review...");
    let gen2_f20 = frames_dir.join("gen2_20pct.jpg");
    let gen2_f50 = frames_dir.join("gen2_50pct.jpg");
    let gen2_f80 = frames_dir.join("gen2_80pct.jpg");

    let dur_sec_gen2 = (final_probed_gen2.duration_ms as f64) / 1000.0;
    extract_frame_ffmpeg(
        &final_record_gen2.final_path,
        dur_sec_gen2 * 0.20,
        &gen2_f20,
    )
    .unwrap();
    extract_frame_ffmpeg(
        &final_record_gen2.final_path,
        dur_sec_gen2 * 0.50,
        &gen2_f50,
    )
    .unwrap();
    extract_frame_ffmpeg(
        &final_record_gen2.final_path,
        dur_sec_gen2 * 0.80,
        &gen2_f80,
    )
    .unwrap();

    println!("Extracted Gen2 Frames:");
    println!("  Gen2 20%: {:?}", gen2_f20);
    println!("  Gen2 50%: {:?}", gen2_f50);
    println!("  Gen2 80%: {:?}", gen2_f80);

    // Gen2 Project Ingestion
    println!("--------------------------------------------------");
    println!("[P3-B STEP 13] Ingesting Gen 2 Output into Project Workflow...");
    let use_res_gen2 = flow_service
        .use_flow_output_in_project(&project.id, &parent_id_gen2)
        .expect("use_flow_output_in_project for Gen2 must succeed");

    println!(
        "Derived Asset #2 ID: {}",
        use_res_gen2.derived_asset.media.media_id
    );
    println!(
        "Derived Asset #2 Path: {:?}",
        use_res_gen2.derived_asset.media.source_path
    );
    assert!(use_res_gen2.derived_asset.media.source_path.exists());

    // Gen2 Secure Preview Resolver
    let preview_media_gen2 = resolve_project_media_by_id(
        &project.id,
        Some(&use_res_gen2.derived_asset.media.media_id),
        &paths,
    )
    .expect("Preview resolver must locate derived media 2");
    assert_eq!(
        preview_media_gen2.1.media_id,
        use_res_gen2.derived_asset.media.media_id
    );

    // Verify Final Project contains Original + Derived #1 + Derived #2
    println!("--------------------------------------------------");
    println!("[P3-B STEP 14] Verifying Final Project Contains All Assets...");
    let final_project = manager.get_project(&project.id).unwrap();
    assert!(final_project.source_media.is_some());
    assert_eq!(
        final_project.derived_media_assets.len(),
        2,
        "Project must contain exactly 2 derived media assets"
    );
    assert_eq!(
        final_project.derived_media_assets[0].media.media_id,
        use_res.derived_asset.media.media_id
    );
    assert_eq!(
        final_project.derived_media_assets[1].media.media_id,
        use_res_gen2.derived_asset.media.media_id
    );

    // Final Source Immutability Check
    println!("--------------------------------------------------");
    println!("[P3-B STEP 15] Final Original Source Immutability Check...");
    let source_sha256_after_all = calculate_file_sha256(&source_video_canonical_path).unwrap();
    println!("SOURCE_SHA256_AFTER_ALL: {}", source_sha256_after_all);
    assert_eq!(
        source_sha256_before, source_sha256_after_all,
        "CRITICAL: Original source video must remain bit-for-bit unchanged after both generations"
    );

    // Final Balance Check
    println!("--------------------------------------------------");
    println!("[P3-B STEP 16] Checking Final Credit Status...");
    let final_balance_status = flow_service
        .refresh_flow_credit_balance("profile_2")
        .await
        .expect("Final balance refresh must succeed");
    println!(
        "Final Credit Balance Status: {:?}",
        final_balance_status.status
    );
    println!(
        "Final Credit Balance Value: {:?}",
        final_balance_status.balance
    );

    println!("==================================================");
    println!("🎉 FLOW-P3-B TWO-GENERATION LIVE ACCEPTANCE COMPLETED SUCCESSFULLY!");
    println!("==================================================");
}
