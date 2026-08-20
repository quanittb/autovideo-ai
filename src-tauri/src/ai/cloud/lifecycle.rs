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

        // Acquire request lock to prevent duplicate concurrent client submissions
        let req_lock = self.get_request_lock(&project_id, &client_req_id);
        let _req_guard = req_lock.lock().await;

        // 1. Check if client request already produced an existing persistent job
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

            job.state = CloudJobState::Submitted;
            job.submission_state = SubmissionState::InFlight;
            job.retry.submit_attempts = job.retry.submit_attempts.saturating_add(1);
            job.timestamps.submitted_at = Some(Utc::now().to_rfc3339());
            job.cost.estimate = Some(plan.routing_decision.estimated_cost);
            job.cost.budget_limit = plan.budget_limit;
            job.increment_revision();

            self.store.save_job_atomic(&job)?;
            let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
        }

        // 8. Submit to provider (network call outside of lock)
        match provider.submit_job(&request).await {
            Ok(handle) => {
                let lock = self.get_job_lock(&job.internal_job_id);
                let _guard = lock.lock().await;

                job.remote_job_id = Some(handle.remote_id);
                job.submission_state = SubmissionState::Acknowledged;
                job.state = CloudJobState::Processing;
                job.remote_status = Some("processing".to_string());
                job.increment_revision();

                self.store.save_job_atomic(&job)?;
                let _ = self.event_sink.emit_job_updated(&job.to_event_payload());

                // Spawn non-blocking background polling task
                self.spawn_polling_task(job.clone());

                Ok(job)
            }
            Err(e) => {
                let lock = self.get_job_lock(&job.internal_job_id);
                let _guard = lock.lock().await;

                job.submission_state = SubmissionState::Ambiguous;
                job.state = CloudJobState::Blocked;
                job.error = Some(JobErrorRecord {
                    code: "AMBIGUOUS_SUBMISSION".to_string(),
                    sanitized_message: format!(
                        "Submission failed without acknowledged remote ID: {}. Auto-resubmission is blocked to prevent double charges.",
                        e
                    ),
                });
                job.increment_revision();

                self.store.save_job_atomic(&job)?;
                let _ = self.event_sink.emit_job_updated(&job.to_event_payload());

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
    // Cancellation Orchestration & Reconciliation
    // -------------------------------------------------------------------------

    pub async fn cancel_cloud_generation(
        &self,
        project_id: &str,
        job_id_or_internal: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        let mut job = self.get_job_status(project_id, job_id_or_internal)?;

        // 1. Short lock to persist cancellation intent and signal background task
        {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;

            if job.state.is_terminal() {
                return Ok(job);
            }

            job.cancellation_requested = true;
            job.increment_revision();
            self.store.save_job_atomic(&job)?;
            let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
        }

        // 2. Immediately signal polling task to break out of sleep/polling
        self.signal_cancellation(&job.internal_job_id);

        // 3. Reconcile with remote provider without holding long lock
        self.reconcile_cancellation(&mut job).await?;
        Ok(job)
    }

    async fn reconcile_cancellation(
        &self,
        job: &mut PersistentCloudJob,
    ) -> Result<(), CloudProviderError> {
        if let Some(ref remote_id) = job.remote_job_id {
            match self.provider_resolver.resolve_provider(&job.provider_id) {
                Ok(provider) => match provider.cancel_job(remote_id).await {
                    Ok(()) => {
                        let lock = self.get_job_lock(&job.internal_job_id);
                        let _guard = lock.lock().await;

                        job.state = CloudJobState::Cancelled;
                        job.increment_revision();
                        self.store.save_job_atomic(job)?;
                        let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
                        Ok(())
                    }
                    Err(e) => {
                        let lock = self.get_job_lock(&job.internal_job_id);
                        let _guard = lock.lock().await;

                        job.state = CloudJobState::Blocked;
                        job.error = Some(JobErrorRecord {
                            code: "CANCELLATION_FAILED_REMOTE".to_string(),
                            sanitized_message: format!(
                                "Remote cancellation failed: {}. Cancellation intent is persisted.",
                                e
                            ),
                        });
                        job.increment_revision();
                        self.store.save_job_atomic(job)?;
                        let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
                        Err(e)
                    }
                },
                Err(e) => {
                    let lock = self.get_job_lock(&job.internal_job_id);
                    let _guard = lock.lock().await;

                    job.state = CloudJobState::Blocked;
                    job.error = Some(JobErrorRecord {
                        code: "MISSING_PROVIDER_CREDENTIALS".to_string(),
                        sanitized_message: format!(
                            "Cannot cancel remote job: provider credentials unavailable: {}. Cancellation intent is preserved.",
                            e
                        ),
                    });
                    job.increment_revision();
                    self.store.save_job_atomic(job)?;
                    let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
                    Ok(())
                }
            }
        } else {
            let lock = self.get_job_lock(&job.internal_job_id);
            let _guard = lock.lock().await;

            job.state = CloudJobState::Cancelled;
            job.increment_revision();
            self.store.save_job_atomic(job)?;
            let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
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

            job.state = CloudJobState::Processing;
            job.error = None;
            job.increment_revision();
            self.store.save_job_atomic(&job)?;
            let _ = self.event_sink.emit_job_updated(&job.to_event_payload());

            self.spawn_polling_task(job.clone());
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

                    job.state = CloudJobState::Created;
                    job.increment_revision();
                    if self.store.save_job_atomic(&job).is_ok() {
                        let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
                    }
                    recovered.push(job);
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

                                job.state = CloudJobState::Blocked;
                                job.error = Some(JobErrorRecord {
                                    code: "MISSING_PROVIDER_CREDENTIALS".to_string(),
                                    sanitized_message: format!("{}", e),
                                });
                                job.increment_revision();
                                if self.store.save_job_atomic(&job).is_ok() {
                                    let _ =
                                        self.event_sink.emit_job_updated(&job.to_event_payload());
                                }
                                recovered.push(job);
                            }
                        }
                    } else {
                        let lock = self.get_job_lock(&job.internal_job_id);
                        let _guard = lock.lock().await;

                        job.state = CloudJobState::Blocked;
                        job.submission_state = SubmissionState::Ambiguous;
                        job.error = Some(JobErrorRecord {
                            code: "AMBIGUOUS_SUBMISSION".to_string(),
                            sanitized_message: "Process crashed during submission without acknowledged remote ID. Auto-resubmission is disabled.".to_string(),
                        });
                        job.increment_revision();
                        if self.store.save_job_atomic(&job).is_ok() {
                            let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
                        }
                        recovered.push(job);
                    }
                }
                CloudJobState::Downloading => {
                    self.spawn_polling_task(job.clone());
                    recovered.push(job);
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
                            let lock = self.get_job_lock(&job.internal_job_id);
                            let _guard = lock.lock().await;

                            let validator = CloudOutputValidator::new();
                            match validator.validate_and_promote_artifact(
                                &partial,
                                &final_p,
                                job.validation_policy.expected_duration_sec,
                                job.validation_policy.require_audio,
                            ) {
                                Ok(record) => {
                                    job.state = CloudJobState::Completed;
                                    job.output = record;
                                    job.timestamps.completed_at = Some(Utc::now().to_rfc3339());
                                }
                                Err(e) => {
                                    job.state = CloudJobState::Failed;
                                    job.error = Some(JobErrorRecord {
                                        code: "VALIDATION_FAILED".to_string(),
                                        sanitized_message: format!(
                                            "Media validation failed on recovery: {}",
                                            e
                                        ),
                                    });
                                }
                            }
                            job.increment_revision();
                            if self.store.save_job_atomic(&job).is_ok() {
                                let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
                            }
                            recovered.push(job);
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
    // Background Non-Blocking Polling & Download Loop
    // -------------------------------------------------------------------------

    fn spawn_polling_task(&self, mut job: PersistentCloudJob) {
        let store = self.store.clone();
        let provider_resolver = self.provider_resolver.clone();
        let event_sink = self.event_sink.clone();
        let timing = self.timing_config;
        let job_locks = self.job_locks.clone();

        let (cancel_tx, mut cancel_rx) = watch::channel(job.cancellation_requested);
        {
            let mut senders = self.cancellation_senders.write().unwrap();
            senders.insert(job.internal_job_id.clone(), cancel_tx);
        }

        tokio::spawn(async move {
            let provider = match provider_resolver.resolve_provider(&job.provider_id) {
                Ok(p) => p,
                Err(e) => {
                    let lock = {
                        let mut locks = job_locks.write().unwrap();
                        locks
                            .entry(job.internal_job_id.clone())
                            .or_insert_with(|| Arc::new(TokioMutex::new(())))
                            .clone()
                    };
                    let _guard = lock.lock().await;

                    job.state = CloudJobState::Blocked;
                    job.error = Some(JobErrorRecord {
                        code: "MISSING_PROVIDER_CREDENTIALS".to_string(),
                        sanitized_message: format!("{}", e),
                    });
                    job.increment_revision();
                    if store.save_job_atomic(&job).is_ok() {
                        let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    }
                    return;
                }
            };

            let remote_id = match &job.remote_job_id {
                Some(r) => r.clone(),
                None => return,
            };

            let start_time = std::time::Instant::now();
            let mut consecutive_errors = 0;

            // 1. Polling Phase without holding persistent lock
            while job.state == CloudJobState::Processing || job.state == CloudJobState::Submitted {
                // Check cancellation channel
                if *cancel_rx.borrow() || job.cancellation_requested {
                    let lock = {
                        let mut locks = job_locks.write().unwrap();
                        locks
                            .entry(job.internal_job_id.clone())
                            .or_insert_with(|| Arc::new(TokioMutex::new(())))
                            .clone()
                    };
                    let _guard = lock.lock().await;

                    match provider.cancel_job(&remote_id).await {
                        Ok(()) => {
                            job.state = CloudJobState::Cancelled;
                        }
                        Err(e) => {
                            job.state = CloudJobState::Blocked;
                            job.error = Some(JobErrorRecord {
                                code: "CANCELLATION_FAILED_REMOTE".to_string(),
                                sanitized_message: format!(
                                    "Remote cancellation failed: {}. Cancellation intent is persisted.",
                                    e
                                ),
                            });
                        }
                    }
                    job.increment_revision();
                    if store.save_job_atomic(&job).is_ok() {
                        let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    }
                    return;
                }

                if start_time.elapsed() >= Duration::from_secs(timing.max_poll_duration_sec) {
                    let lock = {
                        let mut locks = job_locks.write().unwrap();
                        locks
                            .entry(job.internal_job_id.clone())
                            .or_insert_with(|| Arc::new(TokioMutex::new(())))
                            .clone()
                    };
                    let _guard = lock.lock().await;

                    job.state = CloudJobState::Failed;
                    job.error = Some(JobErrorRecord {
                        code: "PROVIDER_TIMEOUT".to_string(),
                        sanitized_message: format!(
                            "Polling exceeded maximum duration limit of {}s",
                            timing.max_poll_duration_sec
                        ),
                    });
                    job.increment_revision();
                    if store.save_job_atomic(&job).is_ok() {
                        let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    }
                    return;
                }

                // Poll network endpoint outside lock
                match provider.poll_status(&remote_id).await {
                    Ok(resp) => {
                        consecutive_errors = 0;

                        let lock = {
                            let mut locks = job_locks.write().unwrap();
                            locks
                                .entry(job.internal_job_id.clone())
                                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                                .clone()
                        };
                        let _guard = lock.lock().await;

                        job.retry.poll_attempts = job.retry.poll_attempts.saturating_add(1);
                        job.remote_status = Some(format!("{:?}", resp.status).to_lowercase());

                        match resp.status {
                            RemoteStatus::Starting | RemoteStatus::Processing => {
                                job.state = CloudJobState::Processing;
                                job.increment_revision();
                                if store.save_job_atomic(&job).is_ok() {
                                    let _ = event_sink.emit_job_updated(&job.to_event_payload());
                                }
                            }
                            RemoteStatus::Succeeded => {
                                job.state = CloudJobState::Downloading;
                                job.output_url = resp.output_url.clone();
                                job.increment_revision();
                                if store.save_job_atomic(&job).is_ok() {
                                    let _ = event_sink.emit_job_updated(&job.to_event_payload());
                                }
                                break;
                            }
                            RemoteStatus::Failed => {
                                job.state = CloudJobState::Failed;
                                job.error = Some(JobErrorRecord {
                                    code: "PROVIDER_EXECUTION_FAILED".to_string(),
                                    sanitized_message: resp.error.unwrap_or_else(|| {
                                        "Remote execution failed without error detail".to_string()
                                    }),
                                });
                                job.increment_revision();
                                if store.save_job_atomic(&job).is_ok() {
                                    let _ = event_sink.emit_job_updated(&job.to_event_payload());
                                }
                                return;
                            }
                            RemoteStatus::Canceled => {
                                job.state = CloudJobState::Cancelled;
                                job.increment_revision();
                                if store.save_job_atomic(&job).is_ok() {
                                    let _ = event_sink.emit_job_updated(&job.to_event_payload());
                                }
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors >= timing.max_consecutive_poll_errors {
                            let lock = {
                                let mut locks = job_locks.write().unwrap();
                                locks
                                    .entry(job.internal_job_id.clone())
                                    .or_insert_with(|| Arc::new(TokioMutex::new(())))
                                    .clone()
                            };
                            let _guard = lock.lock().await;

                            job.state = CloudJobState::Failed;
                            job.error = Some(JobErrorRecord {
                                code: "POLL_CONSECUTIVE_ERRORS".to_string(),
                                sanitized_message: format!(
                                    "Failed after {} consecutive polling network errors: {}",
                                    consecutive_errors, e
                                ),
                            });
                            job.increment_revision();
                            if store.save_job_atomic(&job).is_ok() {
                                let _ = event_sink.emit_job_updated(&job.to_event_payload());
                            }
                            return;
                        }
                    }
                }

                // Sleep with immediate cancellation wakeup
                tokio::select! {
                    _ = cancel_rx.changed() => {
                        if *cancel_rx.borrow() {
                            job.cancellation_requested = true;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(timing.poll_interval_ms)) => {}
                }
            }

            // 2. Download Phase outside persistent lock
            if job.state == CloudJobState::Downloading {
                let output_url = match &job.output_url {
                    Some(u) => u.clone(),
                    None => {
                        let lock = {
                            let mut locks = job_locks.write().unwrap();
                            locks
                                .entry(job.internal_job_id.clone())
                                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                                .clone()
                        };
                        let _guard = lock.lock().await;

                        job.state = CloudJobState::Failed;
                        job.error = Some(JobErrorRecord {
                            code: "MISSING_OUTPUT_URL".to_string(),
                            sanitized_message:
                                "Provider reported success but output URL was missing".to_string(),
                        });
                        job.increment_revision();
                        if store.save_job_atomic(&job).is_ok() {
                            let _ = event_sink.emit_job_updated(&job.to_event_payload());
                        }
                        return;
                    }
                };

                let partial_path =
                    match store.artifact_partial_path(&job.project_id, &job.internal_job_id) {
                        Ok(p) => p,
                        Err(e) => {
                            let lock = {
                                let mut locks = job_locks.write().unwrap();
                                locks
                                    .entry(job.internal_job_id.clone())
                                    .or_insert_with(|| Arc::new(TokioMutex::new(())))
                                    .clone()
                            };
                            let _guard = lock.lock().await;

                            job.state = CloudJobState::Failed;
                            job.error = Some(JobErrorRecord {
                                code: "PATH_ERROR".to_string(),
                                sanitized_message: format!("{}", e),
                            });
                            job.increment_revision();
                            if store.save_job_atomic(&job).is_ok() {
                                let _ = event_sink.emit_job_updated(&job.to_event_payload());
                            }
                            return;
                        }
                    };

                let mut download_success = false;
                for attempt in 1..=timing.max_download_attempts {
                    job.retry.download_attempts = attempt;
                    match provider.download_result(&output_url, &partial_path).await {
                        Ok(_) => {
                            download_success = true;
                            break;
                        }
                        Err(_e) => {
                            let _ = std::fs::remove_file(&partial_path);
                            tokio::time::sleep(Duration::from_millis(50 * (attempt as u64))).await;
                        }
                    }
                }

                let lock = {
                    let mut locks = job_locks.write().unwrap();
                    locks
                        .entry(job.internal_job_id.clone())
                        .or_insert_with(|| Arc::new(TokioMutex::new(())))
                        .clone()
                };
                let _guard = lock.lock().await;

                if !download_success {
                    job.state = CloudJobState::Failed;
                    job.error = Some(JobErrorRecord {
                        code: "DOWNLOAD_FAILED".to_string(),
                        sanitized_message: format!(
                            "Failed to download output artifact after {} attempts",
                            job.retry.download_attempts
                        ),
                    });
                    job.increment_revision();
                    if store.save_job_atomic(&job).is_ok() {
                        let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    }
                    return;
                }

                job.state = CloudJobState::ValidatingOutput;
                job.increment_revision();
                if store.save_job_atomic(&job).is_ok() {
                    let _ = event_sink.emit_job_updated(&job.to_event_payload());
                }
            }

            // 3. Validation & Promotion Phase
            if job.state == CloudJobState::ValidatingOutput {
                let partial_path =
                    match store.artifact_partial_path(&job.project_id, &job.internal_job_id) {
                        Ok(p) => p,
                        Err(e) => {
                            let lock = {
                                let mut locks = job_locks.write().unwrap();
                                locks
                                    .entry(job.internal_job_id.clone())
                                    .or_insert_with(|| Arc::new(TokioMutex::new(())))
                                    .clone()
                            };
                            let _guard = lock.lock().await;

                            job.state = CloudJobState::Failed;
                            job.error = Some(JobErrorRecord {
                                code: "PATH_ERROR".to_string(),
                                sanitized_message: format!("{}", e),
                            });
                            job.increment_revision();
                            if store.save_job_atomic(&job).is_ok() {
                                let _ = event_sink.emit_job_updated(&job.to_event_payload());
                            }
                            return;
                        }
                    };

                let final_path =
                    match store.artifact_final_path(&job.project_id, &job.internal_job_id) {
                        Ok(p) => p,
                        Err(e) => {
                            let lock = {
                                let mut locks = job_locks.write().unwrap();
                                locks
                                    .entry(job.internal_job_id.clone())
                                    .or_insert_with(|| Arc::new(TokioMutex::new(())))
                                    .clone()
                            };
                            let _guard = lock.lock().await;

                            job.state = CloudJobState::Failed;
                            job.error = Some(JobErrorRecord {
                                code: "PATH_ERROR".to_string(),
                                sanitized_message: format!("{}", e),
                            });
                            job.increment_revision();
                            if store.save_job_atomic(&job).is_ok() {
                                let _ = event_sink.emit_job_updated(&job.to_event_payload());
                            }
                            return;
                        }
                    };

                let validator = CloudOutputValidator::new();
                let validation_result = validator.validate_and_promote_artifact(
                    &partial_path,
                    &final_path,
                    job.validation_policy.expected_duration_sec,
                    job.validation_policy.require_audio,
                );

                let lock = {
                    let mut locks = job_locks.write().unwrap();
                    locks
                        .entry(job.internal_job_id.clone())
                        .or_insert_with(|| Arc::new(TokioMutex::new(())))
                        .clone()
                };
                let _guard = lock.lock().await;

                match validation_result {
                    Ok(artifact_record) => {
                        job.state = CloudJobState::Completed;
                        job.output = artifact_record;
                        job.timestamps.completed_at = Some(Utc::now().to_rfc3339());
                        job.increment_revision();
                        if store.save_job_atomic(&job).is_ok() {
                            let _ = event_sink.emit_job_updated(&job.to_event_payload());
                        }
                    }
                    Err(e) => {
                        job.state = CloudJobState::Failed;
                        job.error = Some(JobErrorRecord {
                            code: "VALIDATION_FAILED".to_string(),
                            sanitized_message: format!("Media validation failed: {}", e),
                        });
                        job.increment_revision();
                        if store.save_job_atomic(&job).is_ok() {
                            let _ = event_sink.emit_job_updated(&job.to_event_payload());
                        }
                    }
                }
            }
        });
    }
}
