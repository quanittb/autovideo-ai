use super::error::CloudProviderError;
use super::job::{
    CloudJobEventPayload, CloudJobRequest, CloudJobState, CostRecord, InputAssets, JobErrorRecord,
    JobTimestamps, OutputArtifactRecord, PersistentCloudJob, RetryCounters, SubmissionState,
    ValidationPolicy, CURRENT_CLOUD_JOB_SCHEMA_VERSION,
};
use super::provider::RemoteStatus;
use super::registry::ProviderRegistry;
use super::resolver::{CloudProviderResolver, DefaultCloudProviderResolver};
use super::store::PersistentCloudJobStore;
use super::submission::{CloudSubmissionGate, DefaultCloudSubmissionGate};
use super::validator::CloudOutputValidator;
use super::ExecutionClass;
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
    provider_resolver: Arc<dyn CloudProviderResolver>,
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
        provider_resolver: Arc<dyn CloudProviderResolver>,
        event_sink: Arc<dyn EventSink>,
        submission_gate: Arc<dyn CloudSubmissionGate>,
        timing_config: LifecycleTimingConfig,
    ) -> Self {
        let store = PersistentCloudJobStore::new(storage_paths.clone());
        let project_manager = ProjectManager::new(storage_paths);
        Self {
            store,
            project_manager,
            provider_resolver,
            event_sink,
            submission_gate,
            timing_config,
            job_locks: Arc::new(RwLock::new(HashMap::new())),
            cancellation_locks: Arc::new(RwLock::new(HashMap::new())),
            cancellation_senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_defaults(storage_paths: StoragePaths) -> Self {
        Self::new(
            storage_paths,
            Arc::new(DefaultCloudProviderResolver::new()),
            Arc::new(NoopEventSink),
            Arc::new(DefaultCloudSubmissionGate::new()),
            LifecycleTimingConfig::production(),
        )
    }

    pub fn store(&self) -> &PersistentCloudJobStore {
        &self.store
    }

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

    fn get_request_lock(&self, project_id: &str, client_request_id: &str) -> Arc<TokioMutex<()>> {
        let key = format!("{}:{}", project_id, client_request_id);
        let mut locks = self.job_locks.write().unwrap();
        locks
            .entry(key)
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }

    fn signal_cancellation(&self, internal_job_id: &str) {
        if let Ok(senders) = self.cancellation_senders.read() {
            if let Some(tx) = senders.get(internal_job_id) {
                let _ = tx.send(true);
            }
        }
    }

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

        if let Some(ref p) = request.reference_image {
            assets.reference_image_path = Some(p.clone());
            if p.exists() {
                let h = CloudOutputValidator::compute_file_sha256(p)?;
                hasher.update(h.as_bytes());
                assets.reference_image_hash = Some(h);
            }
        }

        let config_hash = format!("{:x}", hasher.finalize());
        Ok((assets, config_hash))
    }

    // -------------------------------------------------------------------------
    // Submission Orchestration (with In-Flight Cancellation Race Handling)
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

        // Acquire request lock to prevent duplicate concurrent client submissions
        let req_lock = self.get_request_lock(&project_id, &client_req_id);
        let _req_guard = req_lock.lock().await;

        // 1. Check if client request already produced an existing persistent job (Fail-Closed on corrupt store)
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
                // 2. Validate project exists in ProjectManager
                let project = self.project_manager.get_project(&project_id).map_err(|e| {
                    CloudProviderError::RequestInvalid(format!("Project not found: {}", e))
                })?;

                // 3. Compute real audio validation policy from Project configuration
                let require_audio = project
                    .transformation_config
                    .preservation
                    .preserve_original_audio
                    && project
                        .source_media
                        .as_ref()
                        .map(|m| m.has_audio)
                        .unwrap_or(false);

                let validation_policy = ValidationPolicy {
                    expected_duration_sec: if request.duration_seconds > 0.0 {
                        Some(request.duration_seconds)
                    } else {
                        None
                    },
                    require_audio,
                };

                // 4. Compute stable input assets and configuration hash
                let (input_assets, config_hash) = Self::compute_inputs(&request)?;
                let internal_job_id = format!("cjob-{}", Uuid::new_v4());

                let new_job = PersistentCloudJob {
                    schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
                    state_revision: 1,
                    job_id: client_req_id.clone(),
                    internal_job_id: internal_job_id.clone(),
                    project_id: project_id.clone(),
                    provider_id: "replicate".to_string(),
                    model_id: "minimax/video-01".to_string(),
                    model_version: "minimax/video-01".to_string(),
                    task_type: request.task_type.clone(),
                    execution_class: ExecutionClass::SpecializedVideoTransformation,
                    input_assets,
                    configuration_hash: config_hash,
                    submission_state: SubmissionState::NeverAttempted,
                    remote_job_id: None,
                    state: CloudJobState::Created,
                    cost: CostRecord::default(),
                    output: OutputArtifactRecord::default(),
                    retry: RetryCounters::default(),
                    error: None,
                    timestamps: JobTimestamps::default(),
                    cancellation_requested: false,
                    progress_pct: None,
                    remote_status: None,
                    output_url: None,
                    validation_policy,
                };

                // Persist first -> only then emit
                self.store.save_job_atomic(&new_job)?;
                let _ = self
                    .event_sink
                    .emit_job_updated(&new_job.to_event_payload());
                new_job
            }
        };

        // 5. Resolve provider adapter from resolver
        let provider = self.provider_resolver.resolve_provider(&job.provider_id)?;
        let registry = ProviderRegistry::new();

        // 6. Submission gate validation (Production delegates to CostSaving router)
        let plan = self.submission_gate.validate_and_prepare(
            &request,
            max_cost,
            provider.as_ref(),
            &registry,
        )?;

        // 7. Critical section: transition to IN_FLIGHT and persist BEFORE calling provider.submit_job()
        {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;

            let mut current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
            current.state = CloudJobState::Submitted;
            current.submission_state = SubmissionState::InFlight;
            current.retry.submit_attempts = current.retry.submit_attempts.saturating_add(1);
            current.timestamps.submitted_at = Some(Utc::now().to_rfc3339());
            current.cost.estimate = Some(plan.routing_decision.estimated_cost);
            current.cost.budget_limit = plan.budget_limit;
            current.increment_revision();

            self.store.save_job_atomic(&current)?;
            let _ = self
                .event_sink
                .emit_job_updated(&current.to_event_payload());
            job = current;
        }

        // 8. Submit to provider (network call outside of lock)
        let submit_res = provider.submit_job(&request).await;

        // 9. After network await: acquire lock and reload authoritative state from disk (Eliminate stale writes)
        let lock = self.get_job_lock(&job.internal_job_id);
        let _guard = lock.lock().await;

        let mut authoritative = self.store.load_job(&job.project_id, &job.internal_job_id)?;

        match submit_res {
            Ok(handle) => {
                authoritative.remote_job_id = Some(handle.remote_id.clone());
                authoritative.submission_state = SubmissionState::Acknowledged;

                if authoritative.cancellation_requested {
                    // In-flight cancellation occurred while submit_job was awaiting response:
                    // Preserve remoteJobId, and reconcile remote cancellation immediately
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
        job_id_or_internal: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        if let Ok(job) = self.store.load_job(project_id, job_id_or_internal) {
            return Ok(job);
        }
        if let Some(job) = self
            .store
            .find_job_by_client_request_id(project_id, job_id_or_internal)?
        {
            return Ok(job);
        }
        Err(CloudProviderError::RequestInvalid(format!(
            "Job {} not found in project {}",
            job_id_or_internal, project_id
        )))
    }

    // -------------------------------------------------------------------------
    // Cancellation Orchestration & Reconciliation (Single Remote Cancel Owner)
    // -------------------------------------------------------------------------

    pub async fn cancel_cloud_generation(
        &self,
        project_id: &str,
        job_id_or_internal: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        let initial = self.get_job_status(project_id, job_id_or_internal)?;

        // 1. Short lock to persist cancellation intent and signal background task
        let mut job = {
            let lock = self.get_job_lock(&initial.internal_job_id);
            let _guard = lock.lock().await;

            let mut current = self
                .store
                .load_job(&initial.project_id, &initial.internal_job_id)?;
            if current.state.is_terminal() {
                return Ok(current);
            }

            current.cancellation_requested = true;
            current.increment_revision();
            self.store.save_job_atomic(&current)?;
            let _ = self
                .event_sink
                .emit_job_updated(&current.to_event_payload());
            current
        };

        // 2. Immediately signal background task to abort local operations
        self.signal_cancellation(&job.internal_job_id);

        // 3. Reconcile with remote provider (Single owner of provider.cancel_job protected by cancellation lock)
        self.reconcile_cancellation(&mut job).await?;
        self.remove_cancellation_sender(&job.internal_job_id);
        Ok(job)
    }

    async fn reconcile_cancellation(
        &self,
        job: &mut PersistentCloudJob,
    ) -> Result<(), CloudProviderError> {
        // Prevent concurrent cancellation commands from executing duplicate remote cancellations
        let cancel_lock = self.get_cancellation_lock(&job.internal_job_id);
        let _cancel_guard = cancel_lock.lock().await;

        // Reload authoritative state under short lock
        let (submission_state, remote_id_opt, is_already_terminal) = {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;
            let current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
            (
                current.submission_state,
                current.remote_job_id.clone(),
                current.state.is_terminal(),
            )
        };

        if is_already_terminal {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;
            *job = self.store.load_job(&job.project_id, &job.internal_job_id)?;
            return Ok(());
        }

        // Critical safety check: If submission is in flight without acknowledged remote ID,
        // do NOT false-cancel locally. Keep state as SUBMITTED with cancellation_pending_submission_ack.
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
            match self.provider_resolver.resolve_provider(&job.provider_id) {
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
                        sanitized_message: format!(
                            "Cannot cancel remote job: provider credentials unavailable: {}. Cancellation intent is preserved.",
                            e
                        ),
                    });
                    current.increment_revision();
                    self.store.save_job_atomic(&current)?;
                    let _ = self
                        .event_sink
                        .emit_job_updated(&current.to_event_payload());
                    *job = current;
                    Ok(())
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
    // Resume Blocked Job (Deadlock-Safe)
    // -------------------------------------------------------------------------

    pub async fn resume_unblock_job(
        &self,
        project_id: &str,
        job_id_or_internal: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        let mut job = self.get_job_status(project_id, job_id_or_internal)?;

        if job.state != CloudJobState::Blocked {
            return Err(CloudProviderError::RequestInvalid(format!(
                "Job {} is not blocked (current state: {:?})",
                job.internal_job_id, job.state
            )));
        }

        // Verify provider credentials now available
        let _provider = self.provider_resolver.resolve_provider(&job.provider_id)?;

        if job.cancellation_requested {
            self.reconcile_cancellation(&mut job).await?;
            return Ok(job);
        }

        if job.remote_job_id.is_some() {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;

            let mut current = self.store.load_job(&job.project_id, &job.internal_job_id)?;
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

    // -------------------------------------------------------------------------
    // Startup Recovery
    // -------------------------------------------------------------------------

    pub async fn recover_startup_jobs(
        &self,
    ) -> Result<Vec<PersistentCloudJob>, CloudProviderError> {
        let active_jobs = self.store.list_all_active_jobs()?;
        let mut recovered = Vec::new();

        for mut job in active_jobs {
            // Handle cancellation requested during recovery
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
                        match self.provider_resolver.resolve_provider(&job.provider_id) {
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
                    // ValidatingOutput recovery does NOT require provider credentials
                    let partial_path = self
                        .store
                        .artifact_partial_path(&job.project_id, &job.internal_job_id);
                    let final_path = self
                        .store
                        .artifact_final_path(&job.project_id, &job.internal_job_id);

                    if let (Ok(partial), Ok(final_p)) = (partial_path, final_path) {
                        if partial.exists() {
                            let validator = CloudOutputValidator::new();
                            let meta_res = validator.validate_artifact(
                                &partial,
                                job.validation_policy.expected_duration_sec,
                                job.validation_policy.require_audio,
                            );

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
    // Background Non-Blocking Polling & Download Worker
    // -------------------------------------------------------------------------

    fn spawn_polling_task(&self, initial_job: PersistentCloudJob) {
        let store = self.store.clone();
        let provider_resolver = self.provider_resolver.clone();
        let event_sink = self.event_sink.clone();
        let timing = self.timing_config;
        let job_locks = self.job_locks.clone();
        let cancellation_senders = self.cancellation_senders.clone();

        let (cancel_tx, mut cancel_rx) = watch::channel(initial_job.cancellation_requested);
        {
            let mut senders = self.cancellation_senders.write().unwrap();
            senders.insert(initial_job.internal_job_id.clone(), cancel_tx);
        }

        let project_id = initial_job.project_id.clone();
        let internal_job_id = initial_job.internal_job_id.clone();
        let provider_id = initial_job.provider_id.clone();

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

            let provider = match provider_resolver.resolve_provider(&provider_id) {
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

            // 1. Polling Phase: Never mutate stale snapshots across network await
            loop {
                // Check cancellation channel and disk state
                if *cancel_rx.borrow() {
                    cleanup_sender();
                    return;
                }

                // Check authoritative disk state
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

                // Poll network endpoint outside lock
                let poll_outcome = provider.poll_status(&remote_id).await;

                // Reload authoritative job from disk to apply poll result
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
                    Ok(resp) => {
                        consecutive_errors = 0;
                        current.retry.poll_attempts = current.retry.poll_attempts.saturating_add(1);
                        current.remote_status = Some(format!("{:?}", resp.status).to_lowercase());

                        match resp.status {
                            RemoteStatus::Starting | RemoteStatus::Processing => {
                                current.state = CloudJobState::Processing;
                                current.increment_revision();
                                if store.save_job_atomic(&current).is_ok() {
                                    let _ =
                                        event_sink.emit_job_updated(&current.to_event_payload());
                                }
                            }
                            RemoteStatus::Succeeded => {
                                current.state = CloudJobState::Downloading;
                                current.output_url = resp.output_url.clone();
                                current.increment_revision();
                                if store.save_job_atomic(&current).is_ok() {
                                    let _ =
                                        event_sink.emit_job_updated(&current.to_event_payload());
                                }
                                break; // Transition to Download phase
                            }
                            RemoteStatus::Failed => {
                                current.state = CloudJobState::Failed;
                                current.error = Some(JobErrorRecord {
                                    code: "PROVIDER_EXECUTION_FAILED".to_string(),
                                    sanitized_message: resp.error.unwrap_or_else(|| {
                                        "Remote execution failed without error detail".to_string()
                                    }),
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
                            current.state = CloudJobState::Failed;
                            current.error = Some(JobErrorRecord {
                                code: "POLL_CONSECUTIVE_ERRORS".to_string(),
                                sanitized_message: format!(
                                    "Failed after {} consecutive polling network errors: {}",
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

                // Sleep with immediate cancellation wakeup
                tokio::select! {
                    _ = cancel_rx.changed() => {
                        cleanup_sender();
                        return;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(timing.poll_interval_ms)) => {}
                }
            }

            // 2. Download Phase
            let (output_url, partial_path, starting_attempt) = {
                let lock = get_lock();
                let _guard = lock.lock().await;

                let current = match store.load_job(&project_id, &internal_job_id) {
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

                let url = match current.output_url {
                    Some(ref u) => u.clone(),
                    None => {
                        cleanup_sender();
                        return;
                    }
                };

                let partial = match store.artifact_partial_path(&project_id, &internal_job_id) {
                    Ok(p) => p,
                    Err(_) => {
                        cleanup_sender();
                        return;
                    }
                };

                (url, partial, current.retry.download_attempts)
            };

            let mut download_success = false;
            for attempt in (starting_attempt + 1)..=timing.max_download_attempts {
                // Check cancellation before each attempt
                if *cancel_rx.borrow() {
                    let _ = std::fs::remove_file(&partial_path);
                    cleanup_sender();
                    return;
                }

                // Retry budget persistence must FAIL CLOSED
                let save_ok = {
                    let lock = get_lock();
                    let _guard = lock.lock().await;
                    if let Ok(mut current) = store.load_job(&project_id, &internal_job_id) {
                        if current.cancellation_requested || current.state.is_terminal() {
                            let _ = std::fs::remove_file(&partial_path);
                            cleanup_sender();
                            return;
                        }
                        current.retry.download_attempts = attempt;
                        current.increment_revision();
                        store.save_job_atomic(&current).is_ok()
                    } else {
                        false
                    }
                };

                if !save_ok {
                    let _ = std::fs::remove_file(&partial_path);
                    cleanup_sender();
                    return;
                }

                match provider.download_result(&output_url, &partial_path).await {
                    Ok(_) => {
                        // Check cancellation immediately after download await
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

            // 3. Validation Phase (Separate from Atomic Promotion)
            let final_path = match store.artifact_final_path(&project_id, &internal_job_id) {
                Ok(p) => p,
                Err(_) => {
                    let _ = std::fs::remove_file(&partial_path);
                    cleanup_sender();
                    return;
                }
            };

            // Inspect cancellation before validation
            if *cancel_rx.borrow() {
                let _ = std::fs::remove_file(&partial_path);
                cleanup_sender();
                return;
            }

            let policy = {
                let lock = get_lock();
                let _guard = lock.lock().await;
                match store.load_job(&project_id, &internal_job_id) {
                    Ok(c) => {
                        if c.cancellation_requested || c.state.is_terminal() {
                            let _ = std::fs::remove_file(&partial_path);
                            cleanup_sender();
                            return;
                        }
                        c.validation_policy
                    }
                    Err(_) => {
                        let _ = std::fs::remove_file(&partial_path);
                        cleanup_sender();
                        return;
                    }
                }
            };

            let validator = CloudOutputValidator::new();
            let validation_result = validator.validate_artifact(
                &partial_path,
                policy.expected_duration_sec,
                policy.require_audio,
            );

            // 4. Critical Decision: Atomic Promotion & Completion under Short Lock
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
                // If cancellation occurred during validation, do NOT promote artifact or mark Completed!
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
                            current.increment_revision();
                            if store.save_job_atomic(&current).is_ok() {
                                let _ = event_sink.emit_job_updated(&current.to_event_payload());
                            }
                        }
                        Err(e) => {
                            let _ = std::fs::remove_file(&partial_path);
                            current.state = CloudJobState::Failed;
                            current.error = Some(JobErrorRecord {
                                code: "PROMOTION_FAILED".to_string(),
                                sanitized_message: format!("Failed to promote artifact: {}", e),
                            });
                            current.increment_revision();
                            if store.save_job_atomic(&current).is_ok() {
                                let _ = event_sink.emit_job_updated(&current.to_event_payload());
                            }
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
                    current.increment_revision();
                    if store.save_job_atomic(&current).is_ok() {
                        let _ = event_sink.emit_job_updated(&current.to_event_payload());
                    }
                }
            }

            cleanup_sender();
        });
    }
}
