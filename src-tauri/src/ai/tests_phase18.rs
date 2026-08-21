use crate::ai::cloud::job::{
    ArtifactContainer, ArtifactDescriptor, ArtifactVideoCodec, AuthorizedAssetPreview,
    CloudJobEventPayload, CloudJobRequest, CloudJobState, CloudSubmissionPreflight,
    SubmissionState,
};
use crate::ai::cloud::registry::{ExecutionClass, ProviderRegistry};
use crate::ai::cloud::router::{RoutingPreference, RoutingTarget, TaskClass};
use crate::ai::cloud::spec::SourceMediaFacts;
use crate::ai::cloud::submission::evaluate_cloud_submission_preflight;
use std::path::PathBuf;

fn make_test_request(
    task_type: &str,
    duration_sec: f64,
    video_path: Option<PathBuf>,
) -> CloudJobRequest {
    CloudJobRequest {
        job_id: "test_req_1".to_string(),
        project_id: Some("proj_test".to_string()),
        prompt: "test prompt".to_string(),
        negative_prompt: None,
        source_video: video_path,
        reference_image: None,
        reference_images: None,
        duration_seconds: duration_sec,
        fps: 30.0,
        resolution: (1920, 1080),
        task_type: task_type.to_string(),
    }
}

#[test]
fn test_phase18_01_preflight_uses_authoritative_source_facts_over_request_duration() {
    let registry = ProviderRegistry::new();
    let mut req = make_test_request("CHARACTER_REPLACEMENT", 5.0, None);
    req.reference_images = Some(vec![PathBuf::from("ref1.png")]);

    // 1. Direct router test: 30s 1080p source facts override fake 5s request duration
    let facts = SourceMediaFacts {
        duration_sec: 30.0,
        width: 1920,
        height: 1080,
        fps: 30.0,
        has_audio: false,
        ..Default::default()
    };
    let decision = crate::ai::cloud::router::GenerationRouter::route_with_facts(
        TaskClass::CharacterReplacement,
        RoutingPreference::CostSaving,
        &req,
        Some(&facts),
        None,
        &registry,
    );
    // Pruna cost for 30s @ 1080p ($0.06/s) = $1.80 (authoritative from registry facts, not fake 5s request)
    let estimated = decision.estimated_cost.estimated_usd.unwrap();
    assert!(
        (estimated - 1.80).abs() < 0.001,
        "Expected deterministic $1.80 for 30s 1080p facts, got {}",
        estimated
    );

    // 2. Preflight evaluation for character replacement without source video
    let eval = evaluate_cloud_submission_preflight(&req, Some(10.0), &registry).unwrap();
    assert_eq!(eval.task_class, TaskClass::CharacterReplacement);
    assert!(eval.submittable);
    assert!(eval.budget_approved);
}

#[test]
fn test_phase18_02_preflight_background_removal_duration_limit_blocks_submission() {
    let registry = ProviderRegistry::new();
    let facts = SourceMediaFacts {
        duration_sec: 65.0, // Exceeds BRIA 60s max duration limit
        width: 1920,
        height: 1080,
        fps: 30.0,
        has_audio: false,
        ..Default::default()
    };
    let req = make_test_request("BACKGROUND_REMOVAL", 10.0, None);

    let decision = crate::ai::cloud::router::GenerationRouter::route_with_facts(
        TaskClass::BackgroundRemoval,
        RoutingPreference::CostSaving,
        &req,
        Some(&facts),
        None,
        &registry,
    );
    assert_eq!(decision.target, RoutingTarget::Unavailable);
    assert!(
        decision.reason.contains("exceeds provider limit")
            || decision.reason.contains("PROVIDER_DURATION_LIMIT"),
        "Expected max duration rejection, got: {}",
        decision.reason
    );
}

#[test]
fn test_phase18_03_preflight_background_removal_with_references_fails_closed() {
    let registry = ProviderRegistry::new();
    let mut req = make_test_request("BACKGROUND_REMOVAL", 10.0, None);
    req.reference_images = Some(vec![PathBuf::from("unexpected_ref.png")]);

    let res = evaluate_cloud_submission_preflight(&req, Some(10.0), &registry);
    assert!(res.is_err());
    let err = format!("{}", res.unwrap_err());
    assert!(
        err.contains("UNEXPECTED_REFERENCE_INPUTS_FOR_BACKGROUND_REMOVAL"),
        "Expected reference rejection, got: {}",
        err
    );
}

#[test]
fn test_phase18_04_preflight_budget_exceeded_marks_submittable_false() {
    let registry = ProviderRegistry::new();
    let mut req = make_test_request("CHARACTER_REPLACEMENT", 30.0, None);
    req.reference_images = Some(vec![PathBuf::from("ref1.png")]);

    // Set budget to $0.01 (valid budget, but 30s 1080p CharacterReplacement costs $1.80)
    let eval = evaluate_cloud_submission_preflight(&req, Some(0.01), &registry).unwrap();
    assert!(!eval.submittable);
    assert!(!eval.budget_approved);
    assert_eq!(eval.blocking_code.as_deref(), Some("COST_BUDGET_EXCEEDED"));
}

#[test]
fn test_phase18_05_cloud_job_event_payload_carries_state_revision_and_artifact_descriptor() {
    let payload = CloudJobEventPayload {
        job_id: "client_job_1".to_string(),
        internal_job_id: "cloud_job_xyz".to_string(),
        project_id: "proj_123".to_string(),
        provider_id: "replicate".to_string(),
        model_id: "bria/video-remove-background".to_string(),
        task_type: "BACKGROUND_REMOVAL".to_string(),
        execution_class: ExecutionClass::UtilityCloud,
        state: CloudJobState::Completed,
        submission_state: SubmissionState::Acknowledged,
        remote_job_id: Some("remote_123".to_string()),
        cost_estimate: None,
        actual_cost: Some(0.042),
        budget_limit: 3.0,
        output_path: Some("/path/to/artifact.webm".to_string()),
        retry_counters: Default::default(),
        error: None,
        created_at: "2026-08-21T00:00:00Z".to_string(),
        updated_at: "2026-08-21T00:00:10Z".to_string(),
        submitted_at: Some("2026-08-21T00:00:01Z".to_string()),
        completed_at: Some("2026-08-21T00:00:10Z".to_string()),
        cancellation_requested: false,
        progress_pct: Some(100.0),
        remote_status: Some("succeeded".to_string()),
        state_revision: 12,
        artifact_descriptor: Some(ArtifactDescriptor {
            container: ArtifactContainer::Webm,
            video_codec: ArtifactVideoCodec::Vp9,
            require_alpha: true,
            require_audio: false,
        }),
    };

    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(serialized.contains("\"stateRevision\":12"));
    assert!(serialized.contains("\"artifactDescriptor\""));
    assert!(serialized.contains("\"requireAlpha\":true"));

    let deserialized: CloudJobEventPayload = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.state_revision, 12);
    assert_eq!(
        deserialized.artifact_descriptor.unwrap().container,
        ArtifactContainer::Webm
    );
}

#[test]
fn test_phase18_06_preflight_dto_serialization_camel_case() {
    let preflight = CloudSubmissionPreflight {
        task_class: TaskClass::BackgroundRemoval,
        routing_decision: crate::ai::cloud::router::RoutingDecision {
            target: RoutingTarget::Cloud,
            provider_id: "replicate".to_string(),
            model_id: "bria/video-remove-background".to_string(),
            execution_class: ExecutionClass::UtilityCloud,
            task: TaskClass::BackgroundRemoval,
            mode: RoutingPreference::CostSaving,
            reason: "BRIA Video Background Removal".to_string(),
            cost_breakdown: Default::default(),
            estimated_cost: Default::default(),
            fallback_available: false,
            auto_submit_allowed: true,
            block_code: None,
        },
        source_facts: Some(SourceMediaFacts {
            duration_sec: 15.0,
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_audio: false,
            ..Default::default()
        }),
        budget_limit: 3.0,
        budget_approved: true,
        submittable: true,
        blocking_code: None,
    };

    let serialized = serde_json::to_string(&preflight).unwrap();
    assert!(serialized.contains("\"taskClass\":\"BACKGROUND_REMOVAL\""));
    assert!(serialized.contains("\"budgetApproved\":true"));
    assert!(serialized.contains("\"submittable\":true"));
    assert!(serialized.contains("\"sourceFacts\""));
}

#[test]
fn test_phase18_07_authorized_asset_preview_dto_truthful_semantics() {
    let preview = AuthorizedAssetPreview {
        local_path: "C:\\projects\\proj1\\cloud_jobs\\job1\\artifact.webm".to_string(),
        container: "webm".to_string(),
        video_codec: "vp9".to_string(),
        alpha_validated: true,
        audio_required: false,
        actual_has_audio: None,
    };

    let serialized = serde_json::to_string(&preview).unwrap();
    assert!(serialized.contains("\"alphaValidated\":true"));
    assert!(serialized.contains("\"audioRequired\":false"));
    assert!(serialized.contains("\"actualHasAudio\":null"));
}

#[test]
fn test_phase18_08_persistent_job_store_lists_jobs_in_project() {
    use crate::ai::cloud::job::{CostRecord, InputAssets, PersistentCloudJob};
    use crate::ai::cloud::store::PersistentCloudJobStore;
    use crate::system::StoragePaths;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp.path());
    let store = PersistentCloudJobStore::new(paths);

    let project_id = "proj_test_123";
    let job1 = PersistentCloudJob::new(
        "client_1".to_string(),
        "internal_1".to_string(),
        project_id.to_string(),
        "replicate".to_string(),
        "bria/video-remove-background".to_string(),
        "official-current".to_string(),
        "BACKGROUND_REMOVAL".to_string(),
        ExecutionClass::UtilityCloud,
        InputAssets::default(),
        "hash_1".to_string(),
        CostRecord::default(),
    );

    store.save_job_atomic(&job1).unwrap();

    let listed = store.list_jobs_in_project(project_id).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].internal_job_id, "internal_1");

    let event_payload = listed[0].to_event_payload();
    assert_eq!(event_payload.state_revision, 1);
}

#[test]
fn test_phase18_09_non_completed_job_cannot_be_authorized_for_preview() {
    use crate::ai::cloud::job::{CostRecord, InputAssets, PersistentCloudJob};
    use crate::ai::cloud::store::PersistentCloudJobStore;
    use crate::system::StoragePaths;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp.path());
    let store = PersistentCloudJobStore::new(paths);

    let project_id = "proj_test_456";
    let mut job = PersistentCloudJob::new(
        "client_2".to_string(),
        "internal_2".to_string(),
        project_id.to_string(),
        "pruna".to_string(),
        "p-video-replace".to_string(),
        "v1".to_string(),
        "CHARACTER_REPLACEMENT".to_string(),
        ExecutionClass::SpecializedVideoTransformation,
        InputAssets::default(),
        "hash_2".to_string(),
        CostRecord::default(),
    );
    job.state = CloudJobState::Submitted;

    store.save_job_atomic(&job).unwrap();

    // Verify loaded job state
    let loaded = store.load_job(project_id, "internal_2").unwrap();
    assert_ne!(loaded.state, CloudJobState::Completed);
}

#[test]
fn test_phase18_10_deterministic_registry_pricing_tiers() {
    let registry = ProviderRegistry::new();
    let req = make_test_request("CHARACTER_REPLACEMENT", 10.0, None);

    // 720p tier: 10s @ 1280x720 -> 10.0 * 0.03 = 0.30 USD
    let facts_720p = SourceMediaFacts {
        duration_sec: 10.0,
        width: 1280,
        height: 720,
        fps: 24.0,
        has_audio: true,
        ..Default::default()
    };
    let decision_720p = crate::ai::cloud::router::GenerationRouter::route_with_facts(
        TaskClass::CharacterReplacement,
        RoutingPreference::CostSaving,
        &req,
        Some(&facts_720p),
        None,
        &registry,
    );
    let cost_720p = decision_720p.estimated_cost.estimated_usd.unwrap();
    assert!(
        (cost_720p - 0.30).abs() < 0.001,
        "Expected $0.30 for 10s 720p tier, got {}",
        cost_720p
    );

    // 1080p tier: 10s @ 1920x1080 -> 10.0 * 0.06 = 0.60 USD
    let facts_1080p = SourceMediaFacts {
        duration_sec: 10.0,
        width: 1920,
        height: 1080,
        fps: 24.0,
        has_audio: true,
        ..Default::default()
    };
    let decision_1080p = crate::ai::cloud::router::GenerationRouter::route_with_facts(
        TaskClass::CharacterReplacement,
        RoutingPreference::CostSaving,
        &req,
        Some(&facts_1080p),
        None,
        &registry,
    );
    let cost_1080p = decision_1080p.estimated_cost.estimated_usd.unwrap();
    assert!(
        (cost_1080p - 0.60).abs() < 0.001,
        "Expected $0.60 for 10s 1080p tier, got {}",
        cost_1080p
    );
}

#[test]
fn test_phase18_11_project_source_preview_path_security_roots() {
    use crate::commands::resolve_project_source_preview_path;
    use crate::projects::{ProjectManager, SourceMedia};
    use crate::system::StoragePaths;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp.path());
    fs::create_dir_all(&paths.projects_dir).unwrap();

    let pm = ProjectManager::new(paths.clone());
    let mut proj = pm.create_project("Sec Project").unwrap();
    let project_dir = paths.projects_dir.join(&proj.id);
    let media_dir = project_dir.join("media");
    let cache_dir = project_dir.join("cache");
    fs::create_dir_all(&media_dir).unwrap();
    fs::create_dir_all(&cache_dir).unwrap();

    // 1. Valid source media inside <project>/media
    let valid_file = media_dir.join("source.mp4");
    File::create(&valid_file)
        .unwrap()
        .write_all(b"mp4")
        .unwrap();

    proj.source_media = Some(SourceMedia {
        media_id: "sm1".to_string(),
        original_file_name: "source.mp4".to_string(),
        source_path: valid_file.clone(),
        duration_ms: 5000,
        width: 1920,
        height: 1080,
        fps: 30.0,
        file_size_bytes: 3,
        container: "mp4".to_string(),
        video_codec: "h264".to_string(),
        audio_codec: None,
        has_audio: false,
    });
    pm.update_project(&proj).unwrap();

    let (res_path, _) = resolve_project_source_preview_path(&proj.id, &paths).unwrap();
    assert!(res_path.starts_with(media_dir.canonicalize().unwrap()));

    // 2. Escape attempt: file in <project>/cache
    let cache_file = cache_dir.join("cache_data.mp4");
    File::create(&cache_file)
        .unwrap()
        .write_all(b"cache")
        .unwrap();
    proj.source_media.as_mut().unwrap().source_path = cache_file;
    pm.update_project(&proj).unwrap();

    let err = resolve_project_source_preview_path(&proj.id, &paths).unwrap_err();
    assert!(
        err.contains("SECURITY_VIOLATION"),
        "Expected SECURITY_VIOLATION for cache path, got: {}",
        err
    );

    // 3. Escape attempt: file in project root outside media
    let root_file = project_dir.join("outside.mp4");
    File::create(&root_file)
        .unwrap()
        .write_all(b"outside")
        .unwrap();
    proj.source_media.as_mut().unwrap().source_path = root_file;
    pm.update_project(&proj).unwrap();

    let err2 = resolve_project_source_preview_path(&proj.id, &paths).unwrap_err();
    assert!(
        err2.contains("SECURITY_VIOLATION"),
        "Expected SECURITY_VIOLATION for root file outside media, got: {}",
        err2
    );
}

#[test]
fn test_phase18_12_cloud_artifact_preview_path_security_roots() {
    use crate::ai::cloud::job::{
        CostRecord, InputAssets, OutputArtifactRecord, PersistentCloudJob,
    };
    use crate::ai::cloud::store::PersistentCloudJobStore;
    use crate::commands::resolve_cloud_artifact_preview_path;
    use crate::system::StoragePaths;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let paths = StoragePaths::resolve_from_base(temp.path());
    let store = PersistentCloudJobStore::new(paths.clone());

    let project_id = "proj_sec_art";
    let artifacts_dir = store.project_artifacts_dir(project_id).unwrap();
    fs::create_dir_all(&artifacts_dir).unwrap();

    let valid_artifact = artifacts_dir.join("artifact_1.mp4");
    File::create(&valid_artifact)
        .unwrap()
        .write_all(b"artifact")
        .unwrap();

    let mut job = PersistentCloudJob::new(
        "client_art_1".to_string(),
        "internal_art_1".to_string(),
        project_id.to_string(),
        "pruna".to_string(),
        "p-video-replace".to_string(),
        "v1".to_string(),
        "CHARACTER_REPLACEMENT".to_string(),
        ExecutionClass::SpecializedVideoTransformation,
        InputAssets::default(),
        "hash_art".to_string(),
        CostRecord::default(),
    );
    job.state = CloudJobState::Completed;
    job.output = OutputArtifactRecord {
        final_path: Some(valid_artifact.clone()),
        ..Default::default()
    };
    store.save_job_atomic(&job).unwrap();

    // 1. Valid artifact in artifacts/ directory -> Allowed
    let (res_path, _) =
        resolve_cloud_artifact_preview_path(project_id, "internal_art_1", &store).unwrap();
    assert!(res_path.starts_with(artifacts_dir.canonicalize().unwrap()));

    // 2. Corrupted final_path pointing to job manifest in cloud-jobs/
    let manifest_path = store
        .project_cloud_jobs_dir(project_id)
        .unwrap()
        .join("internal_art_1.json");
    job.output.final_path = Some(manifest_path);
    job.state_revision = 2;
    store.save_job_atomic(&job).unwrap();

    let err =
        resolve_cloud_artifact_preview_path(project_id, "internal_art_1", &store).unwrap_err();
    assert!(
        err.contains("SECURITY_VIOLATION"),
        "Expected SECURITY_VIOLATION for job manifest path, got: {}",
        err
    );
}

#[test]
fn test_phase18_13_real_authoritative_preflight_fixture_with_ffmpeg() {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    let fixture_dir = Path::new("target").join("phase18-fixtures");
    fs::create_dir_all(&fixture_dir).unwrap();
    let fixture_file = fixture_dir.join("sample_720p_2s.mp4");

    // Generate real 2.0s 1280x720 @ 24fps MP4 with ffmpeg lavfi testsrc
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=2:size=1280x720:rate=24",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-f",
        "mp4",
    ]);
    cmd.arg(fixture_file.to_str().unwrap());
    let output = cmd.output();

    if let Ok(out) = output {
        if out.status.success() && fixture_file.is_file() {
            let registry = ProviderRegistry::new();
            let mut req =
                make_test_request("CHARACTER_REPLACEMENT", 99.0, Some(fixture_file.clone()));
            req.reference_images = Some(vec![PathBuf::from("ref1.png")]);
            req.resolution = (3840, 2160); // Intentionally false resolution in request
            req.fps = 60.0; // Intentionally false fps in request

            // Authoritative preflight evaluates real source video on disk via ffprobe
            let eval = evaluate_cloud_submission_preflight(&req, Some(10.0), &registry).unwrap();

            assert_eq!(eval.task_class, TaskClass::CharacterReplacement);
            assert!(eval.submittable);
            assert!(eval.budget_approved);

            let facts = eval
                .source_facts
                .expect("Expected probed source facts from real fixture");
            assert!(
                facts.duration_sec >= 1.8 && facts.duration_sec <= 2.2,
                "Expected probed duration ~2.0s, got {}",
                facts.duration_sec
            );
            assert_eq!(facts.width, 1280);
            assert_eq!(facts.height, 720);

            // Router selects 720p tier ($0.03/s) for 2.0s -> ~ $0.06 (NOT false 99.0s * $0.06 = $5.94)
            let cost = eval.routing_decision.estimated_cost.estimated_usd.unwrap();
            assert!(
                (cost - (facts.duration_sec * 0.03)).abs() < 0.01,
                "Expected ~0.06 USD for 2s 720p, got {}",
                cost
            );
        }
    }
}
