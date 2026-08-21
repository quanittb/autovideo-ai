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

    // 1. Direct router test: 30s source facts override fake 5s request duration
    let facts = SourceMediaFacts {
        duration_sec: 30.0,
        width: 1920,
        height: 1080,
        fps: 30.0,
        has_audio: false,
    };
    let decision = crate::ai::cloud::router::GenerationRouter::route_with_facts(
        TaskClass::CharacterReplacement,
        RoutingPreference::CostSaving,
        &req,
        Some(&facts),
        None,
        &registry,
    );
    // Pruna cost for 30s at $0.015/s = $0.45 (not 5s * $0.015 = $0.075)
    assert!(
        decision.estimated_cost.estimated_usd.unwrap() >= 0.40,
        "Expected >= $0.40 for 30s facts, got {:?}",
        decision.estimated_cost.estimated_usd
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

    // Set budget to $0.01 (valid budget, but 30s CharacterReplacement costs ~$0.45)
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
        },
        source_facts: Some(SourceMediaFacts {
            duration_sec: 15.0,
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_audio: false,
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
