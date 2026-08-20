use super::error::CloudProviderError;
use super::job::{
    ArtifactContainer, ArtifactDescriptor, ArtifactVideoCodec, CloudJobEventPayload,
    CloudJobRequest, CloudJobState, CostRecord, InputAssets, JobErrorRecord, JobTimestamps,
    OutputArtifactRecord, PersistentCloudJob, RetryCounters, SubmissionState, ValidationPolicy,
    CURRENT_CLOUD_JOB_SCHEMA_VERSION,
};
use super::provider::RemoteStatus;
use super::registry::ProviderRegistry;
use super::resolver::CloudProviderResolver;
use super::spec::{
    PreparedBackgroundRemoval, PreparedCharacterReplacement, PreparedProviderSubmission,
    ProviderTaskSpec,
};
use super::store::PersistentCloudJobStore;
use super::submission::CloudSubmissionGate;
use super::validator::CloudOutputValidator;
use crate::projects::ProjectManager;
use crate::system::StoragePaths;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::watch;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

// -----------------------------------------------------------------------------
// 1. Event Sink Trait & Implementations
// -----------------------------------------------------------------------------

pub trait EventSink: Send + Sync {
    fn emit_job_updated(&self, payload: &CloudJobEventPayload) -> Result<(), String>;
}

pub struct NoopEventSink;
impl EventSink for NoopEventSink {
    fn emit_job_updated(&self, _payload: &CloudJobEventPayload) -> Result<(), String> {
        Ok(())
    }
}

pub struct TauriEventSink {
    app_handle: tauri::AppHandle,
}

impl TauriEventSink {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }
}

impl EventSink for TauriEventSink {
    fn emit_job_updated(&self, payload: &CloudJobEventPayload) -> Result<(), String> {
        use tauri::Emitter;
        self.app_handle
            .emit("cloud-job://updated", payload)
            .map_err(|e| format!("Tauri event emission failed: {}", e))
    }
}

pub struct TestEventSink {
    pub events: Arc<RwLock<Vec<CloudJobEventPayload>>>,
}

impl TestEventSink {
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for TestEventSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink for TestEventSink {
    fn emit_job_updated(&self, payload: &CloudJobEventPayload) -> Result<(), String> {
        if let Ok(mut list) = self.events.write() {
            list.push(payload.clone());
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// 2. Lifecycle Timing Configuration
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct LifecycleTimingConfig {
    pub poll_interval_ms: u64,
    pub max_poll_duration_sec: u64,
    pub max_consecutive_poll_errors: u32,
    pub max_download_attempts: u32,
}

impl Default for LifecycleTimingConfig {
    fn default() -> Self {
        Self::production()
    }
}

impl LifecycleTimingConfig {
    pub fn production() -> Self {
        Self {
            poll_interval_ms: 1000,
            max_poll_duration_sec: 300,
            max_consecutive_poll_errors: 5,
            max_download_attempts: 3,
        }
    }

    pub fn fast_test() -> Self {
        Self {
            poll_interval_ms: 10,
            max_poll_duration_sec: 2,
            max_consecutive_poll_errors: 3,
            max_download_attempts: 2,
        }
    }
}

// -----------------------------------------------------------------------------
// 3. CloudJobLifecycleService
// -----------------------------------------------------------------------------

pub struct CloudJobLifecycleService {
    store: PersistentCloudJobStore,
    project_manager: ProjectManager,
    resolver: Arc<dyn CloudProviderResolver>,
    event_sink: Arc<dyn EventSink>,
    submission_gate: Arc<dyn CloudSubmissionGate>,
    timing_config: LifecycleTimingConfig,
    job_locks: Arc<RwLock<HashMap<String, Arc<TokioMutex<()>>>>>,
    cancellation_locks: Arc<RwLock<HashMap<String, Arc<TokioMutex<()>>>>>,
    cancellation_senders: Arc<RwLock<HashMap<String, watch::Sender<bool>>>>,
}

impl CloudJobLifecycleService {
    pub fn new(
        storage_paths: StoragePaths,
        resolver: Arc<dyn CloudProviderResolver>,
        event_sink: Arc<dyn EventSink>,
        submission_gate: Arc<dyn CloudSubmissionGate>,
        timing_config: LifecycleTimingConfig,
    ) -> Self {
        let store = PersistentCloudJobStore::new(storage_paths.clone());
        let project_manager = ProjectManager::new(storage_paths);
        Self {
            store,
            project_manager,
            resolver,
            event_sink,
            submission_gate,
            timing_config,
            job_locks: Arc::new(RwLock::new(HashMap::new())),
            cancellation_locks: Arc::new(RwLock::new(HashMap::new())),
            cancellation_senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn store(&self) -> &PersistentCloudJobStore {
        &self.store
    }

    pub fn project_manager(&self) -> &ProjectManager {
        &self.project_manager
    }

    pub fn resolver(&self) -> &Arc<dyn CloudProviderResolver> {
        &self.resolver
    }

    pub fn submission_gate(&self) -> &Arc<dyn CloudSubmissionGate> {
        &self.submission_gate
    }

    pub fn event_sink(&self) -> &Arc<dyn EventSink> {
        &self.event_sink
    }

    pub fn timing_config(&self) -> &LifecycleTimingConfig {
        &self.timing_config
    }

    // -------------------------------------------------------------------------
    // Lock Helpers
    // -------------------------------------------------------------------------

    fn get_job_lock(&self, internal_job_id: &str) -> Arc<TokioMutex<()>> {
        let mut locks = self.job_locks.write().unwrap();
        locks
            .entry(internal_job_id.to_string())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }

    fn get_cancellation_lock(&self, internal_job_id: &str) -> Arc<TokioMutex<()>> {
        let mut locks = self.cancellation_locks.write().unwrap();
        locks
            .entry(internal_job_id.to_string())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }

    fn get_request_lock(&self, project_id: &str, client_req_id: &str) -> Arc<TokioMutex<()>> {
        let key = format!("req-{}-{}", project_id, client_req_id);
        self.get_job_lock(&key)
    }

    fn signal_cancellation(&self, internal_job_id: &str) {
        if let Ok(senders) = self.cancellation_senders.read() {
            if let Some(tx) = senders.get(internal_job_id) {
                let _ = tx.send(true);
            }
        }
    }

    #[allow(dead_code)]
    fn remove_cancellation_sender(&self, internal_job_id: &str) {
        if let Ok(mut senders) = self.cancellation_senders.write() {
            senders.remove(internal_job_id);
        }
    }

    // -------------------------------------------------------------------------
    // Input Hashing & Configuration Identity
    // -------------------------------------------------------------------------

    fn compute_inputs(
        request: &CloudJobRequest,
        preserve_audio: bool,
    ) -> Result<(InputAssets, String), CloudProviderError> {
        let mut assets = InputAssets::default();
        let mut hasher = Sha256::new();

        hasher.update(request.task_type.as_bytes());
        hasher.update(request.prompt.as_bytes());
        if let Some(ref np) = request.negative_prompt {
            hasher.update(np.as_bytes());
        }
        hasher.update(&request.duration_seconds.to_le_bytes());
        hasher.update(&request.fps.to_le_bytes());
        hasher.update(&request.resolution.0.to_le_bytes());
        hasher.update(&request.resolution.1.to_le_bytes());
        hasher.update(&[if preserve_audio { 1u8 } else { 0u8 }]);

        if let Some(ref p) = request.source_video {
            assets.source_video_path = Some(p.clone());
            if p.exists() {
                let h = CloudOutputValidator::compute_file_sha256(p)?;
                hasher.update(h.as_bytes());
                assets.source_video_hash = Some(h);
            } else {
                return Err(CloudProviderError::RequestInvalid(format!(
                    "Source video not found at {}",
                    p.display()
                )));
            }
        }

        let ref_images = request.get_reference_images();
        for p in &ref_images {
            if p.exists() {
                let h = CloudOutputValidator::compute_file_sha256(p)?;
                hasher.update(h.as_bytes());
                assets.reference_image_paths.push(p.clone());
                assets.reference_image_hashes.push(h);
            } else {
                return Err(CloudProviderError::RequestInvalid(format!(
                    "Reference image not found at {}",
                    p.display()
                )));
            }
        }

        if let Some(first_p) = assets.reference_image_paths.first() {
            assets.reference_image_path = Some(first_p.clone());
        }
        if let Some(first_h) = assets.reference_image_hashes.first() {
            assets.reference_image_hash = Some(first_h.clone());
        }

        let config_hash = format!("{:x}", hasher.finalize());
        Ok((assets, config_hash))
    }

    // -------------------------------------------------------------------------
    // Submission Orchestration
    // -------------------------------------------------------------------------

    pub async fn start_cloud_generation(
        &self,
        request: CloudJobRequest,
        max_cost: Option<f64>,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        let client_req_id = request.job_id.clone();
        let project_id = match &request.project_id {
            Some(pid) => pid.clone(),
            None => {
                return Err(CloudProviderError::RequestInvalid(
                    "PROJECT_ID_REQUIRED: project_id is required for persistent cloud job"
                        .to_string(),
                ));
            }
        };

        // 1. Validate project exists in ProjectManager
        let project = self
            .project_manager
            .get_project(&project_id)
            .map_err(|e| CloudProviderError::RequestInvalid(format!("Project not found: {}", e)))?;

        // 2. Submission gate validation (Selects provider_id & model_id independently)
        let registry = ProviderRegistry::new();
        let plan = self
            .submission_gate
            .validate_and_prepare(&request, max_cost, &registry)?;

        // 3. Resolve runtime & check live execution / credentials
        let runtime = self
            .resolver
            .resolve_runtime(&plan.provider_key.provider_id, &plan.provider_key.model_id)?;

        // 4. Build trusted normalized ProviderTaskSpec
        let task_spec = ProviderTaskSpec::build(&request, &project, &plan)?;

        // 5. Acquire request lock to prevent duplicate concurrent client submissions
        let req_lock = self.get_request_lock(&project_id, &client_req_id);
        let _req_guard = req_lock.lock().await;

        // 6. Check if client request already produced an existing persistent job
        let mut job = match self
            .store
            .find_job_by_client_request_id(&project_id, &client_req_id)?
        {
            Some(existing) => match existing.submission_state {
                SubmissionState::InFlight => {
                    return Err(CloudProviderError::RequestInvalid(format!(
                        "DUPLICATE_SUBMISSION_PREVENTED: Job {} is currently in-flight",
                        existing.internal_job_id
                    )));
                }
                SubmissionState::Acknowledged => {
                    return Err(CloudProviderError::RequestInvalid(format!(
                        "DUPLICATE_SUBMISSION_PREVENTED: Job {} has already been submitted (remote_id: {:?})",
                        existing.internal_job_id, existing.remote_job_id
                    )));
                }
                SubmissionState::Ambiguous => {
                    return Err(CloudProviderError::RequestInvalid(format!(
                        "DUPLICATE_SUBMISSION_PREVENTED: Job {} is in ambiguous submission state. Automated re-submission is blocked.",
                        existing.internal_job_id
                    )));
                }
                SubmissionState::NeverAttempted => existing,
            },
            None => {
                let (descriptor, validation_policy, preserve_audio) = match &task_spec {
                    ProviderTaskSpec::CharacterReplacement(spec) => {
                        let desc = ArtifactDescriptor {
                            container: ArtifactContainer::Mp4,
                            video_codec: ArtifactVideoCodec::H264,
                            require_alpha: false,
                            require_audio: spec.save_audio,
                        };
                        let val = ValidationPolicy {
                            expected_duration_sec: if request.duration_seconds > 0.0 {
                                Some(request.duration_seconds)
                            } else {
                                None
                            },
                            expected_width: None,
                            expected_height: None,
                            expected_fps: None,
                            require_audio: spec.save_audio,
                            require_alpha: false,
                            expected_container: Some("mp4".to_string()),
                            expected_video_codec: Some("h264".to_string()),
                        };
                        (desc, val, spec.save_audio)
                    }
                    ProviderTaskSpec::BackgroundRemoval(spec) => {
                        let desc = ArtifactDescriptor {
                            container: ArtifactContainer::Webm,
                            video_codec: ArtifactVideoCodec::Vp9,
                            require_alpha: true,
                            require_audio: spec.preserve_audio,
                        };
                        let val = ValidationPolicy {
                            expected_duration_sec: Some(spec.source_facts.duration_sec),
                            expected_width: Some(spec.source_facts.width),
                            expected_height: Some(spec.source_facts.height),
                            expected_fps: Some(spec.source_facts.fps),
                            require_audio: spec.preserve_audio,
                            require_alpha: true,
                            expected_container: Some("webm".to_string()),
                            expected_video_codec: Some("vp9".to_string()),
                        };
                        (desc, val, spec.preserve_audio)
                    }
                };

                let (input_assets, config_hash) = Self::compute_inputs(&request, preserve_audio)?;
                let internal_job_id = format!("cjob-{}", Uuid::new_v4());

                let new_job = PersistentCloudJob {
                    schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
                    state_revision: 1,
                    job_id: client_req_id.clone(),
                    internal_job_id: internal_job_id.clone(),
                    project_id: project_id.clone(),
                    provider_id: plan.provider_key.provider_id.clone(),
                    model_id: plan.provider_key.model_id.clone(),
                    model_version: "official-current".to_string(),
                    task_type: request.task_type.clone(),
                    execution_class: plan.routing_decision.execution_class,
                    input_assets,
                    configuration_hash: config_hash,
                    submission_state: SubmissionState::NeverAttempted,
                    remote_job_id: None,
                    state: CloudJobState::Created,
                    cost: CostRecord {
                        estimate: Some(plan.routing_decision.estimated_cost.clone()),
                        budget_limit: plan.budget_limit,
                        confidence: super::cost::CostConfidence::Estimated,
                        reserved_budget: plan.routing_decision.estimated_cost.estimated_usd,
                        actual_cost: None,
                    },
                    output: OutputArtifactRecord::default(),
                    retry: RetryCounters::default(),
                    error: None,
                    timestamps: JobTimestamps::default(),
                    cancellation_requested: false,
                    progress_pct: None,
                    remote_status: None,
                    output_url: None,
                    validation_policy,
                    artifact_descriptor: Some(descriptor),
                };

                // Persist Created state first -> then emit
                self.store.save_job_atomic(&new_job)?;
                let _ = self
                    .event_sink
                    .emit_job_updated(&new_job.to_event_payload());
                new_job
            }
        };

        // 7. Transition to UPLOADING phase
        {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;

            let mut current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
            current.state = CloudJobState::Uploading;
            current.increment_revision();
            self.store.save_job_atomic(&current)?;
            let _ = self
                .event_sink
                .emit_job_updated(&current.to_event_payload());
            job = current;
        }

        // 8. Execute File Uploads (Outside Lock) & Build PreparedProviderSubmission
        let prepared = match task_spec {
            ProviderTaskSpec::CharacterReplacement(spec) => {
                let source_upload_res = runtime
                    .uploader
                    .upload_file(&spec.source_video, "video/mp4")
                    .await;

                let uploaded_source = match source_upload_res {
                    Ok(asset) => asset,
                    Err(e) => {
                        let lock = self.get_job_lock(&job.internal_job_id);
                        let _guard = lock.lock().await;

                        let mut current =
                            self.store.load_job(&job.project_id, &job.internal_job_id)?;
                        if current.cancellation_requested {
                            drop(_guard);
                            self.reconcile_cancellation(&mut current).await?;
                            return Ok(current);
                        }
                        current.state = CloudJobState::Failed;
                        current.submission_state = SubmissionState::NeverAttempted;
                        current.error = Some(JobErrorRecord {
                            code: "UPLOAD_FAILED".to_string(),
                            sanitized_message: format!("Failed to upload source video: {}", e),
                        });
                        current.increment_revision();
                        self.store.save_job_atomic(&current)?;
                        let _ = self
                            .event_sink
                            .emit_job_updated(&current.to_event_payload());
                        return Err(e);
                    }
                };

                let mut uploaded_references = Vec::new();
                for ref_path in &spec.reference_images {
                    match runtime.uploader.upload_file(ref_path, "image/jpeg").await {
                        Ok(asset) => uploaded_references.push(asset),
                        Err(e) => {
                            let lock = self.get_job_lock(&job.internal_job_id);
                            let _guard = lock.lock().await;

                            let mut current =
                                self.store.load_job(&job.project_id, &job.internal_job_id)?;
                            if current.cancellation_requested {
                                drop(_guard);
                                self.reconcile_cancellation(&mut current).await?;
                                return Ok(current);
                            }
                            current.state = CloudJobState::Failed;
                            current.submission_state = SubmissionState::NeverAttempted;
                            current.error = Some(JobErrorRecord {
                                code: "UPLOAD_FAILED".to_string(),
                                sanitized_message: format!(
                                    "Failed to upload reference image: {}",
                                    e
                                ),
                            });
                            current.increment_revision();
                            self.store.save_job_atomic(&current)?;
                            let _ = self
                                .event_sink
                                .emit_job_updated(&current.to_event_payload());
                            return Err(e);
                        }
                    }
                }

                PreparedProviderSubmission::CharacterReplacement(PreparedCharacterReplacement {
                    spec,
                    uploaded_source,
                    uploaded_references,
                })
            }
            ProviderTaskSpec::BackgroundRemoval(spec) => {
                let source_upload_res = runtime
                    .uploader
                    .upload_file(&spec.source_video, "video/mp4")
                    .await;

                let uploaded_source = match source_upload_res {
                    Ok(asset) => asset,
                    Err(e) => {
                        let lock = self.get_job_lock(&job.internal_job_id);
                        let _guard = lock.lock().await;

                        let mut current =
                            self.store.load_job(&job.project_id, &job.internal_job_id)?;
                        if current.cancellation_requested {
                            drop(_guard);
                            self.reconcile_cancellation(&mut current).await?;
                            return Ok(current);
                        }
                        current.state = CloudJobState::Failed;
                        current.submission_state = SubmissionState::NeverAttempted;
                        current.error = Some(JobErrorRecord {
                            code: "UPLOAD_FAILED".to_string(),
                            sanitized_message: format!("Failed to upload source video: {}", e),
                        });
                        current.increment_revision();
                        self.store.save_job_atomic(&current)?;
                        let _ = self
                            .event_sink
                            .emit_job_updated(&current.to_event_payload());
                        return Err(e);
                    }
                };

                PreparedProviderSubmission::BackgroundRemoval(PreparedBackgroundRemoval {
                    spec,
                    uploaded_source,
                })
            }
        };

        // 9. Re-acquire lock, reload authoritative state, verify !cancellation_requested before IN_FLIGHT
        {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;

            let mut current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
            if current.cancellation_requested {
                drop(_guard);
                self.reconcile_cancellation(&mut current).await?;
                return Ok(current);
            }

            current.state = CloudJobState::Submitted;
            current.submission_state = SubmissionState::InFlight;
            current.retry.submit_attempts = current.retry.submit_attempts.saturating_add(1);
            current.timestamps.submitted_at = Some(Utc::now().to_rfc3339());
            current.increment_revision();

            self.store.save_job_atomic(&current)?;
            let _ = self
                .event_sink
                .emit_job_updated(&current.to_event_payload());
            job = current;
        }

        let submit_res = runtime.provider.create_prediction(&prepared).await;

        // 11. After network await: acquire lock and reload authoritative state from disk
        let lock = self.get_job_lock(&job.internal_job_id);
        let _guard = lock.lock().await;

        let mut authoritative = self.store.load_job(&job.project_id, &job.internal_job_id)?;

        match submit_res {
            Ok(handle) => {
                authoritative.remote_job_id = Some(handle.remote_id.clone());
                if let Some(ver) = handle.model_version {
                    authoritative.model_version = ver;
                }
                authoritative.submission_state = SubmissionState::Acknowledged;

                if authoritative.cancellation_requested {
                    authoritative.increment_revision();
                    self.store.save_job_atomic(&authoritative)?;
                    let _ = self
                        .event_sink
                        .emit_job_updated(&authoritative.to_event_payload());

                    drop(_guard);
                    self.reconcile_cancellation(&mut authoritative).await?;
                    return Ok(authoritative);
                }

                authoritative.state = CloudJobState::Processing;
                authoritative.remote_status = Some("processing".to_string());
                authoritative.increment_revision();

                self.store.save_job_atomic(&authoritative)?;
                let _ = self
                    .event_sink
                    .emit_job_updated(&authoritative.to_event_payload());

                // Spawn non-blocking background polling task
                self.spawn_polling_task(authoritative.clone());

                Ok(authoritative)
            }
            Err(e) => {
                authoritative.submission_state = SubmissionState::Ambiguous;
                authoritative.state = CloudJobState::Blocked;
                authoritative.error = Some(JobErrorRecord {
                    code: "AMBIGUOUS_SUBMISSION".to_string(),
                    sanitized_message: format!(
                        "Submission failed without acknowledged remote ID: {}. Auto-resubmission is blocked to prevent double charges.",
                        e
                    ),
                });
                authoritative.increment_revision();

                self.store.save_job_atomic(&authoritative)?;
                let _ = self
                    .event_sink
                    .emit_job_updated(&authoritative.to_event_payload());

                Err(e)
            }
        }
    }

    // -------------------------------------------------------------------------
    // Status Query
    // -------------------------------------------------------------------------

    pub fn get_job_status(
        &self,
        project_id: &str,
        job_id: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        match self.store.load_job(project_id, job_id) {
            Ok(j) => Ok(j),
            Err(e) => {
                if let Ok(Some(j)) = self.store.find_job_by_client_request_id(project_id, job_id) {
                    Ok(j)
                } else {
                    Err(e)
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // Cancellation Reconciliation
    // -------------------------------------------------------------------------

    pub async fn cancel_cloud_generation(
        &self,
        project_id: &str,
        job_id: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        let initial_job = match self.store.load_job(project_id, job_id) {
            Ok(j) => j,
            Err(e) => {
                if let Ok(Some(j)) = self.store.find_job_by_client_request_id(project_id, job_id) {
                    j
                } else {
                    return Err(e);
                }
            }
        };

        let internal_job_id = initial_job.internal_job_id.clone();
        let cancel_lock = self.get_cancellation_lock(&internal_job_id);
        let _cancel_guard = cancel_lock.lock().await;

        let lock = self.get_job_lock(&internal_job_id);
        let _guard = lock.lock().await;

        let mut job = self.store.load_job(project_id, &internal_job_id)?;

        if job.state.is_terminal() {
            return Ok(job);
        }

        job.cancellation_requested = true;
        self.signal_cancellation(&job.internal_job_id);

        if job.state == CloudJobState::Created
            || job.state == CloudJobState::Validating
            || job.state == CloudJobState::CostApprovalRequired
            || job.state == CloudJobState::Uploading
        {
            job.state = CloudJobState::Cancelled;
            job.increment_revision();
            self.store.save_job_atomic(&job)?;
            let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
            return Ok(job);
        }

        job.increment_revision();
        self.store.save_job_atomic(&job)?;
        let _ = self.event_sink.emit_job_updated(&job.to_event_payload());

        drop(_guard);
        drop(_cancel_guard);

        self.reconcile_cancellation(&mut job).await?;
        Ok(job)
    }

    async fn reconcile_cancellation(
        &self,
        job: &mut PersistentCloudJob,
    ) -> Result<(), CloudProviderError> {
        let cancel_lock = self.get_cancellation_lock(&job.internal_job_id);
        let _cancel_guard = cancel_lock.lock().await;

        let (remote_id_opt, submission_state) = {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;
            let current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
            (current.remote_job_id.clone(), current.submission_state)
        };

        if submission_state == SubmissionState::InFlight && remote_id_opt.is_none() {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;

            let mut current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
            current.cancellation_requested = true;
            current.remote_status = Some("cancellation_pending_submission_ack".to_string());
            current.increment_revision();
            self.store.save_job_atomic(&current)?;
            let _ = self
                .event_sink
                .emit_job_updated(&current.to_event_payload());
            *job = current;
            return Ok(());
        }

        if let Some(ref remote_id) = remote_id_opt {
            match self
                .resolver
                .resolve_provider(&job.provider_id, &job.model_id)
            {
                Ok(provider) => match provider.cancel_job(remote_id).await {
                    Ok(()) => {
                        let lock = self.get_job_lock(&job.internal_job_id);
                        let _guard = lock.lock().await;

                        let mut current =
                            self.store.load_job(&job.project_id, &job.internal_job_id)?;
                        current.state = CloudJobState::Cancelled;
                        current.increment_revision();
                        self.store.save_job_atomic(&current)?;
                        let _ = self
                            .event_sink
                            .emit_job_updated(&current.to_event_payload());
                        *job = current;
                        Ok(())
                    }
                    Err(e) => {
                        let lock = self.get_job_lock(&job.internal_job_id);
                        let _guard = lock.lock().await;

                        let mut current =
                            self.store.load_job(&job.project_id, &job.internal_job_id)?;
                        current.state = CloudJobState::Blocked;
                        current.error = Some(JobErrorRecord {
                            code: "CANCELLATION_FAILED_REMOTE".to_string(),
                            sanitized_message: format!(
                                "Remote cancellation failed: {}. Cancellation intent is persisted.",
                                e
                            ),
                        });
                        current.increment_revision();
                        self.store.save_job_atomic(&current)?;
                        let _ = self
                            .event_sink
                            .emit_job_updated(&current.to_event_payload());
                        *job = current;
                        Err(e)
                    }
                },
                Err(e) => {
                    let lock = self.get_job_lock(&job.internal_job_id);
                    let _guard = lock.lock().await;

                    let mut current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
                    current.state = CloudJobState::Blocked;
                    current.error = Some(JobErrorRecord {
                        code: "MISSING_PROVIDER_CREDENTIALS".to_string(),
                        sanitized_message: format!("{}", e),
                    });
                    current.increment_revision();
                    self.store.save_job_atomic(&current)?;
                    let _ = self
                        .event_sink
                        .emit_job_updated(&current.to_event_payload());
                    *job = current;
                    Err(e)
                }
            }
        } else {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;

            let mut current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
            current.state = CloudJobState::Cancelled;
            current.increment_revision();
            self.store.save_job_atomic(&current)?;
            let _ = self
                .event_sink
                .emit_job_updated(&current.to_event_payload());
            *job = current;
            Ok(())
        }
    }

    // -------------------------------------------------------------------------
    // Manual Unblock and Resume
    // -------------------------------------------------------------------------

    pub async fn unblock_and_resume_job(
        &self,
        project_id: &str,
        job_id: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        let lock = self.get_job_lock(job_id);
        let _guard = lock.lock().await;

        let mut job = self.store.load_job(project_id, job_id)?;

        if job.state != CloudJobState::Blocked {
            return Err(CloudProviderError::RequestInvalid(format!(
                "Job {} is in state {:?}, not Blocked",
                job.internal_job_id, job.state
            )));
        }

        if job.cancellation_requested {
            drop(_guard);
            self.reconcile_cancellation(&mut job).await?;
            return Ok(job);
        }

        let _provider = self
            .resolver
            .resolve_provider(&job.provider_id, &job.model_id)?;

        if job.remote_job_id.is_some() {
            let mut current = self.store.load_job(project_id, job_id)?;
            current.state = CloudJobState::Processing;
            current.error = None;
            current.increment_revision();
            self.store.save_job_atomic(&current)?;
            let _ = self
                .event_sink
                .emit_job_updated(&current.to_event_payload());
            job = current.clone();

            self.spawn_polling_task(current);
            Ok(job)
        } else {
            Err(CloudProviderError::RequestInvalid(format!(
                "Job {} has no remote ID to resume",
                job.internal_job_id
            )))
        }
    }

    pub async fn resume_unblock_job(
        &self,
        project_id: &str,
        job_id: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        self.unblock_and_resume_job(project_id, job_id).await
    }

    // -------------------------------------------------------------------------
    // Startup Recovery
    // -------------------------------------------------------------------------

    pub async fn recover_startup_jobs(
        &self,
    ) -> Result<Vec<PersistentCloudJob>, CloudProviderError> {
        let active_jobs = self.store.list_all_active_jobs()?;
        let mut recovered = Vec::new();

        for mut job in active_jobs {
            if job.cancellation_requested {
                let _ = self.reconcile_cancellation(&mut job).await;
                recovered.push(job);
                continue;
            }

            match job.state {
                CloudJobState::Created
                | CloudJobState::Validating
                | CloudJobState::CostApprovalRequired => {
                    recovered.push(job);
                }
                CloudJobState::Uploading => {
                    let lock = self.get_job_lock(&job.internal_job_id);
                    let _guard = lock.lock().await;

                    let mut current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
                    current.state = CloudJobState::Created;
                    current.submission_state = SubmissionState::NeverAttempted;
                    current.increment_revision();
                    if self.store.save_job_atomic(&current).is_ok() {
                        let _ = self
                            .event_sink
                            .emit_job_updated(&current.to_event_payload());
                    }
                    recovered.push(current);
                }
                CloudJobState::Submitted | CloudJobState::Processing => {
                    if job.remote_job_id.is_some() {
                        match self
                            .resolver
                            .resolve_provider(&job.provider_id, &job.model_id)
                        {
                            Ok(_) => {
                                self.spawn_polling_task(job.clone());
                                recovered.push(job);
                            }
                            Err(e) => {
                                let lock = self.get_job_lock(&job.internal_job_id);
                                let _guard = lock.lock().await;

                                let mut current =
                                    self.store.load_job(&job.project_id, &job.internal_job_id)?;
                                current.state = CloudJobState::Blocked;
                                current.error = Some(JobErrorRecord {
                                    code: "MISSING_PROVIDER_CREDENTIALS".to_string(),
                                    sanitized_message: format!("{}", e),
                                });
                                current.increment_revision();
                                if self.store.save_job_atomic(&current).is_ok() {
                                    let _ = self
                                        .event_sink
                                        .emit_job_updated(&current.to_event_payload());
                                }
                                recovered.push(current);
                            }
                        }
                    } else {
                        let lock = self.get_job_lock(&job.internal_job_id);
                        let _guard = lock.lock().await;

                        let mut current =
                            self.store.load_job(&job.project_id, &job.internal_job_id)?;
                        current.state = CloudJobState::Blocked;
                        current.submission_state = SubmissionState::Ambiguous;
                        current.error = Some(JobErrorRecord {
                            code: "AMBIGUOUS_SUBMISSION".to_string(),
                            sanitized_message: "Process crashed during submission without acknowledged remote ID. Auto-resubmission is disabled.".to_string(),
                        });
                        current.increment_revision();
                        if self.store.save_job_atomic(&current).is_ok() {
                            let _ = self
                                .event_sink
                                .emit_job_updated(&current.to_event_payload());
                        }
                        recovered.push(current);
                    }
                }
                CloudJobState::Downloading => {
                    if job.retry.download_attempts >= self.timing_config.max_download_attempts {
                        let lock = self.get_job_lock(&job.internal_job_id);
                        let _guard = lock.lock().await;

                        let mut current =
                            self.store.load_job(&job.project_id, &job.internal_job_id)?;
                        current.state = CloudJobState::Failed;
                        current.error = Some(JobErrorRecord {
                            code: "DOWNLOAD_FAILED".to_string(),
                            sanitized_message: format!(
                                "Download attempts ({}) reached maximum allowed limit ({})",
                                current.retry.download_attempts,
                                self.timing_config.max_download_attempts
                            ),
                        });
                        current.increment_revision();
                        if self.store.save_job_atomic(&current).is_ok() {
                            let _ = self
                                .event_sink
                                .emit_job_updated(&current.to_event_payload());
                        }
                        recovered.push(current);
                    } else {
                        self.spawn_polling_task(job.clone());
                        recovered.push(job);
                    }
                }
                CloudJobState::ValidatingOutput => {
                    let partial_path = self
                        .store
                        .artifact_partial_path(&job.project_id, &job.internal_job_id);
                    let final_path = self.store.artifact_final_path_for_job(&job);

                    if let (Ok(partial), Ok(final_p)) = (partial_path, final_path) {
                        let validator = CloudOutputValidator::new();

                        // Case 1: Final artifact was already promoted before crash!
                        if final_p.is_file() {
                            let meta_res = validator
                                .validate_artifact_with_policy(&final_p, &job.validation_policy);

                            let lock = self.get_job_lock(&job.internal_job_id);
                            let _guard = lock.lock().await;

                            let mut current =
                                self.store.load_job(&job.project_id, &job.internal_job_id)?;
                            if current.cancellation_requested || current.state.is_terminal() {
                                drop(_guard);
                                let _ = self.reconcile_cancellation(&mut current).await;
                                recovered.push(current);
                                continue;
                            }

                            match meta_res {
                                Ok(meta) => {
                                    current.state = CloudJobState::Completed;
                                    current.output = OutputArtifactRecord {
                                        temporary_path: None,
                                        final_path: Some(final_p.clone()),
                                        artifact_hash: Some(meta.artifact_hash),
                                        width: Some(meta.width),
                                        height: Some(meta.height),
                                        duration_sec: Some(meta.duration_sec),
                                        fps: Some(meta.fps),
                                    };
                                    current.timestamps.completed_at = Some(Utc::now().to_rfc3339());
                                }
                                Err(e) => {
                                    current.state = CloudJobState::Failed;
                                    current.error = Some(JobErrorRecord {
                                        code: "VALIDATION_FAILED".to_string(),
                                        sanitized_message: format!(
                                            "Final artifact validation failed on recovery: {}",
                                            e
                                        ),
                                    });
                                }
                            }
                            current.increment_revision();
                            if self.store.save_job_atomic(&current).is_ok() {
                                let _ = self
                                    .event_sink
                                    .emit_job_updated(&current.to_event_payload());
                            }
                            recovered.push(current);
                            continue;
                        }

                        // Case 2: Partial artifact exists
                        if partial.is_file() {
                            let meta_res = validator
                                .validate_artifact_with_policy(&partial, &job.validation_policy);

                            let lock = self.get_job_lock(&job.internal_job_id);
                            let _guard = lock.lock().await;

                            let mut current =
                                self.store.load_job(&job.project_id, &job.internal_job_id)?;
                            if current.cancellation_requested || current.state.is_terminal() {
                                let _ = std::fs::remove_file(&partial);
                                drop(_guard);
                                let _ = self.reconcile_cancellation(&mut current).await;
                                recovered.push(current);
                                continue;
                            }

                            match meta_res {
                                Ok(meta) => {
                                    match CloudOutputValidator::promote_artifact(
                                        &partial, &final_p, &meta,
                                    ) {
                                        Ok(record) => {
                                            current.state = CloudJobState::Completed;
                                            current.output = record;
                                            current.timestamps.completed_at =
                                                Some(Utc::now().to_rfc3339());
                                        }
                                        Err(e) => {
                                            current.state = CloudJobState::Failed;
                                            current.error = Some(JobErrorRecord {
                                                code: "PROMOTION_FAILED".to_string(),
                                                sanitized_message: format!(
                                                    "Failed to promote artifact: {}",
                                                    e
                                                ),
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    current.state = CloudJobState::Failed;
                                    current.error = Some(JobErrorRecord {
                                        code: "VALIDATION_FAILED".to_string(),
                                        sanitized_message: format!(
                                            "Media validation failed on recovery: {}",
                                            e
                                        ),
                                    });
                                }
                            }
                            current.increment_revision();
                            if self.store.save_job_atomic(&current).is_ok() {
                                let _ = self
                                    .event_sink
                                    .emit_job_updated(&current.to_event_payload());
                            }
                            recovered.push(current);
                            continue;
                        }
                    }
                    self.spawn_polling_task(job.clone());
                    recovered.push(job);
                }
                CloudJobState::Blocked => {
                    recovered.push(job);
                }
                _ => {}
            }
        }

        Ok(recovered)
    }

    // -------------------------------------------------------------------------
    // Background Polling Worker
    // -------------------------------------------------------------------------

    fn spawn_polling_task(&self, initial_job: PersistentCloudJob) {
        let store = self.store.clone();
        let provider_resolver = self.resolver.clone();
        let event_sink = self.event_sink.clone();
        let timing = self.timing_config;
        let job_locks = self.job_locks.clone();
        let cancellation_senders = self.cancellation_senders.clone();

        let (cancel_tx, cancel_rx) = watch::channel(initial_job.cancellation_requested);
        {
            let mut senders = self.cancellation_senders.write().unwrap();
            senders.insert(initial_job.internal_job_id.clone(), cancel_tx);
        }

        let project_id = initial_job.project_id.clone();
        let internal_job_id = initial_job.internal_job_id.clone();
        let provider_id = initial_job.provider_id.clone();
        let model_id = initial_job.model_id.clone();

        tokio::spawn(async move {
            let get_lock = || {
                let mut locks = job_locks.write().unwrap();
                locks
                    .entry(internal_job_id.clone())
                    .or_insert_with(|| Arc::new(TokioMutex::new(())))
                    .clone()
            };

            let cleanup_sender = || {
                if let Ok(mut senders) = cancellation_senders.write() {
                    senders.remove(&internal_job_id);
                }
            };

            let provider = match provider_resolver.resolve_provider(&provider_id, &model_id) {
                Ok(p) => p,
                Err(e) => {
                    let lock = get_lock();
                    let _guard = lock.lock().await;

                    if let Ok(mut current) = store.load_job(&project_id, &internal_job_id) {
                        if !current.state.is_terminal() {
                            current.state = CloudJobState::Blocked;
                            current.error = Some(JobErrorRecord {
                                code: "MISSING_PROVIDER_CREDENTIALS".to_string(),
                                sanitized_message: format!("{}", e),
                            });
                            current.increment_revision();
                            if store.save_job_atomic(&current).is_ok() {
                                let _ = event_sink.emit_job_updated(&current.to_event_payload());
                            }
                        }
                    }
                    cleanup_sender();
                    return;
                }
            };

            let remote_id = match &initial_job.remote_job_id {
                Some(r) => r.clone(),
                None => {
                    cleanup_sender();
                    return;
                }
            };

            let start_time = std::time::Instant::now();
            let mut consecutive_errors = 0;

            // 1. Polling Phase
            loop {
                if *cancel_rx.borrow() {
                    cleanup_sender();
                    return;
                }

                {
                    let lock = get_lock();
                    let _guard = lock.lock().await;
                    if let Ok(current) = store.load_job(&project_id, &internal_job_id) {
                        if current.cancellation_requested || current.state.is_terminal() {
                            cleanup_sender();
                            return;
                        }
                        if current.state != CloudJobState::Processing
                            && current.state != CloudJobState::Submitted
                        {
                            break;
                        }
                    } else {
                        cleanup_sender();
                        return;
                    }
                }

                if start_time.elapsed() >= Duration::from_secs(timing.max_poll_duration_sec) {
                    let lock = get_lock();
                    let _guard = lock.lock().await;

                    if let Ok(mut current) = store.load_job(&project_id, &internal_job_id) {
                        if !current.cancellation_requested && !current.state.is_terminal() {
                            current.state = CloudJobState::Failed;
                            current.error = Some(JobErrorRecord {
                                code: "PROVIDER_TIMEOUT".to_string(),
                                sanitized_message: format!(
                                    "Polling exceeded maximum duration limit of {}s",
                                    timing.max_poll_duration_sec
                                ),
                            });
                            current.increment_revision();
                            if store.save_job_atomic(&current).is_ok() {
                                let _ = event_sink.emit_job_updated(&current.to_event_payload());
                            }
                        }
                    }
                    cleanup_sender();
                    return;
                }

                let poll_outcome = provider.poll_status(&remote_id).await;

                let lock = get_lock();
                let _guard = lock.lock().await;

                let mut current = match store.load_job(&project_id, &internal_job_id) {
                    Ok(c) => c,
                    Err(_) => {
                        cleanup_sender();
                        return;
                    }
                };

                if current.cancellation_requested || current.state.is_terminal() {
                    cleanup_sender();
                    return;
                }

                match poll_outcome {
                    Ok(poll_resp) => {
                        consecutive_errors = 0;
                        match poll_resp.status {
                            RemoteStatus::Starting => {
                                current.state = CloudJobState::Processing;
                                current.remote_status = Some("starting".to_string());
                                current.increment_revision();
                                if store.save_job_atomic(&current).is_ok() {
                                    let _ =
                                        event_sink.emit_job_updated(&current.to_event_payload());
                                }
                            }
                            RemoteStatus::Processing => {
                                current.state = CloudJobState::Processing;
                                current.remote_status = Some("processing".to_string());
                                current.increment_revision();
                                if store.save_job_atomic(&current).is_ok() {
                                    let _ =
                                        event_sink.emit_job_updated(&current.to_event_payload());
                                }
                            }
                            RemoteStatus::Succeeded => {
                                current.state = CloudJobState::Downloading;
                                current.remote_status = Some("succeeded".to_string());
                                current.output_url = poll_resp.output_url.clone();
                                current.increment_revision();
                                if store.save_job_atomic(&current).is_ok() {
                                    let _ =
                                        event_sink.emit_job_updated(&current.to_event_payload());
                                }
                                break;
                            }
                            RemoteStatus::Failed => {
                                current.state = CloudJobState::Failed;
                                current.remote_status = Some("failed".to_string());
                                current.error = Some(JobErrorRecord {
                                    code: "REMOTE_GENERATION_FAILED".to_string(),
                                    sanitized_message: poll_resp
                                        .error
                                        .unwrap_or_else(|| "Remote generation failed".to_string()),
                                });
                                current.increment_revision();
                                if store.save_job_atomic(&current).is_ok() {
                                    let _ =
                                        event_sink.emit_job_updated(&current.to_event_payload());
                                }
                                cleanup_sender();
                                return;
                            }
                            RemoteStatus::Canceled => {
                                current.state = CloudJobState::Cancelled;
                                current.remote_status = Some("canceled".to_string());
                                current.increment_revision();
                                if store.save_job_atomic(&current).is_ok() {
                                    let _ =
                                        event_sink.emit_job_updated(&current.to_event_payload());
                                }
                                cleanup_sender();
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors >= timing.max_consecutive_poll_errors {
                            current.state = CloudJobState::Blocked;
                            current.error = Some(JobErrorRecord {
                                code: "POLL_FAILED".to_string(),
                                sanitized_message: format!(
                                    "Consecutive poll errors ({}) reached threshold: {}",
                                    consecutive_errors, e
                                ),
                            });
                            current.increment_revision();
                            if store.save_job_atomic(&current).is_ok() {
                                let _ = event_sink.emit_job_updated(&current.to_event_payload());
                            }
                            cleanup_sender();
                            return;
                        }
                    }
                }

                drop(_guard);
                tokio::time::sleep(Duration::from_millis(timing.poll_interval_ms)).await;
            }

            // 2. Downloading Phase
            let output_url = {
                let lock = get_lock();
                let _guard = lock.lock().await;
                match store.load_job(&project_id, &internal_job_id) {
                    Ok(c) => match c.output_url {
                        Some(u) => u,
                        None => {
                            cleanup_sender();
                            return;
                        }
                    },
                    Err(_) => {
                        cleanup_sender();
                        return;
                    }
                }
            };

            let partial_path = match store.artifact_partial_path(&project_id, &internal_job_id) {
                Ok(p) => p,
                Err(_) => {
                    cleanup_sender();
                    return;
                }
            };

            let mut download_success = false;
            let current_attempts = {
                let lock = get_lock();
                let _guard = lock.lock().await;
                store
                    .load_job(&project_id, &internal_job_id)
                    .map(|c| c.retry.download_attempts)
                    .unwrap_or(0)
            };

            for attempt in current_attempts..timing.max_download_attempts {
                if *cancel_rx.borrow() {
                    let _ = std::fs::remove_file(&partial_path);
                    cleanup_sender();
                    return;
                }

                // Persist incremented retry counter BEFORE downloading
                {
                    let lock = get_lock();
                    let _guard = lock.lock().await;

                    let mut current = match store.load_job(&project_id, &internal_job_id) {
                        Ok(c) => c,
                        Err(_) => {
                            cleanup_sender();
                            return;
                        }
                    };

                    if current.cancellation_requested || current.state.is_terminal() {
                        let _ = std::fs::remove_file(&partial_path);
                        cleanup_sender();
                        return;
                    }

                    current.retry.download_attempts = attempt + 1;
                    current.increment_revision();
                    if store.save_job_atomic(&current).is_err() {
                        let _ = std::fs::remove_file(&partial_path);
                        cleanup_sender();
                        return;
                    }
                }

                match provider.download_result(&output_url, &partial_path).await {
                    Ok(_) => {
                        if *cancel_rx.borrow() {
                            let _ = std::fs::remove_file(&partial_path);
                            cleanup_sender();
                            return;
                        }
                        download_success = true;
                        break;
                    }
                    Err(_e) => {
                        let _ = std::fs::remove_file(&partial_path);
                        tokio::time::sleep(Duration::from_millis(50 * (attempt as u64))).await;
                    }
                }
            }

            // Persist download phase outcome
            {
                let lock = get_lock();
                let _guard = lock.lock().await;

                let mut current = match store.load_job(&project_id, &internal_job_id) {
                    Ok(c) => c,
                    Err(_) => {
                        let _ = std::fs::remove_file(&partial_path);
                        cleanup_sender();
                        return;
                    }
                };

                if current.cancellation_requested || current.state.is_terminal() {
                    let _ = std::fs::remove_file(&partial_path);
                    cleanup_sender();
                    return;
                }

                if !download_success {
                    let _ = std::fs::remove_file(&partial_path);
                    current.state = CloudJobState::Failed;
                    current.error = Some(JobErrorRecord {
                        code: "DOWNLOAD_FAILED".to_string(),
                        sanitized_message: format!(
                            "Failed to download output artifact after {} attempts",
                            current.retry.download_attempts
                        ),
                    });
                    current.increment_revision();
                    if store.save_job_atomic(&current).is_ok() {
                        let _ = event_sink.emit_job_updated(&current.to_event_payload());
                    }
                    cleanup_sender();
                    return;
                }

                current.state = CloudJobState::ValidatingOutput;
                current.increment_revision();
                if store.save_job_atomic(&current).is_ok() {
                    let _ = event_sink.emit_job_updated(&current.to_event_payload());
                }
            }

            // 3. Validation Phase
            if *cancel_rx.borrow() {
                let _ = std::fs::remove_file(&partial_path);
                cleanup_sender();
                return;
            }

            let (final_path, policy) = {
                let lock = get_lock();
                let _guard = lock.lock().await;
                match store.load_job(&project_id, &internal_job_id) {
                    Ok(c) => {
                        if c.cancellation_requested || c.state.is_terminal() {
                            let _ = std::fs::remove_file(&partial_path);
                            cleanup_sender();
                            return;
                        }
                        let fp = match store.artifact_final_path_for_job(&c) {
                            Ok(p) => p,
                            Err(_) => {
                                let _ = std::fs::remove_file(&partial_path);
                                cleanup_sender();
                                return;
                            }
                        };
                        (fp, c.validation_policy)
                    }
                    Err(_) => {
                        let _ = std::fs::remove_file(&partial_path);
                        cleanup_sender();
                        return;
                    }
                }
            };

            let validator = CloudOutputValidator::new();
            let validation_result = validator.validate_artifact_with_policy(&partial_path, &policy);

            // 4. Critical Decision: Atomic Promotion & Completion
            let lock = get_lock();
            let _guard = lock.lock().await;

            let mut current = match store.load_job(&project_id, &internal_job_id) {
                Ok(c) => c,
                Err(_) => {
                    let _ = std::fs::remove_file(&partial_path);
                    cleanup_sender();
                    return;
                }
            };

            if current.cancellation_requested || current.state.is_terminal() {
                let _ = std::fs::remove_file(&partial_path);
                cleanup_sender();
                return;
            }

            match validation_result {
                Ok(meta) => {
                    match CloudOutputValidator::promote_artifact(&partial_path, &final_path, &meta)
                    {
                        Ok(artifact_record) => {
                            current.state = CloudJobState::Completed;
                            current.output = artifact_record;
                            current.timestamps.completed_at = Some(Utc::now().to_rfc3339());
                        }
                        Err(e) => {
                            current.state = CloudJobState::Failed;
                            current.error = Some(JobErrorRecord {
                                code: "PROMOTION_FAILED".to_string(),
                                sanitized_message: format!(
                                    "Failed to promote output artifact: {}",
                                    e
                                ),
                            });
                        }
                    }
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&partial_path);
                    current.state = CloudJobState::Failed;
                    current.error = Some(JobErrorRecord {
                        code: "VALIDATION_FAILED".to_string(),
                        sanitized_message: format!("Media validation failed: {}", e),
                    });
                }
            }

            current.increment_revision();
            if store.save_job_atomic(&current).is_ok() {
                let _ = event_sink.emit_job_updated(&current.to_event_payload());
            }

            cleanup_sender();
        });
    }
}
