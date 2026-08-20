use super::error::CloudProviderError;
use super::job::{
    CloudJobEventPayload, CloudJobRequest, CloudJobState, CostRecord, InputAssets, JobErrorRecord,
    PersistentCloudJob, SubmissionState,
};
use super::provider::RemoteStatus;
use super::registry::ProviderRegistry;
use super::resolver::{CloudProviderResolver, DefaultCloudProviderResolver};
use super::router::TaskClass;
use super::store::PersistentCloudJobStore;
use super::submission::validate_and_prepare_cloud_submission;
use super::validator::CloudOutputValidator;
use crate::projects::ProjectManager;
use crate::system::StoragePaths;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
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
        Self {
            poll_interval_ms: 1000,
            max_poll_duration_sec: 300,
            max_consecutive_poll_errors: 5,
            max_download_attempts: 3,
        }
    }
}

impl LifecycleTimingConfig {
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
    timing_config: LifecycleTimingConfig,
    job_locks: Arc<RwLock<HashMap<String, Arc<TokioMutex<()>>>>>,
}

impl CloudJobLifecycleService {
    pub fn new(
        storage_paths: StoragePaths,
        provider_resolver: Arc<dyn CloudProviderResolver>,
        event_sink: Arc<dyn EventSink>,
        timing_config: LifecycleTimingConfig,
    ) -> Self {
        let store = PersistentCloudJobStore::new(storage_paths.clone());
        let project_manager = ProjectManager::new(storage_paths);
        Self {
            store,
            project_manager,
            provider_resolver,
            event_sink,
            timing_config,
            job_locks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_defaults(storage_paths: StoragePaths) -> Self {
        Self::new(
            storage_paths,
            Arc::new(DefaultCloudProviderResolver::new()),
            Arc::new(NoopEventSink),
            LifecycleTimingConfig::default(),
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

    fn compute_configuration_hash(
        req: &CloudJobRequest,
        provider_id: &str,
        model_id: &str,
        model_version: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(provider_id.as_bytes());
        hasher.update(model_id.as_bytes());
        hasher.update(model_version.as_bytes());
        hasher.update(req.task_type.as_bytes());
        hasher.update(req.prompt.as_bytes());
        if let Some(np) = &req.negative_prompt {
            hasher.update(np.as_bytes());
        }
        hasher.update(&req.resolution.0.to_le_bytes());
        hasher.update(&req.resolution.1.to_le_bytes());
        hasher.update(&req.fps.to_le_bytes());
        hasher.update(&req.duration_seconds.to_le_bytes());
        format!("{:x}", hasher.finalize())
    }

    // -------------------------------------------------------------------------
    // Submit / Start Generation
    // -------------------------------------------------------------------------

    pub async fn start_cloud_generation(
        &self,
        request: CloudJobRequest,
        max_cost: Option<f64>,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        // 1. Strict Project ID validation (no _standalone in production)
        let project_id = match &request.project_id {
            Some(pid) if !pid.trim().is_empty() => pid.trim().to_string(),
            _ => {
                return Err(CloudProviderError::RequestInvalid(
                    "PROJECT_ID_REQUIRED: A valid projectId is required for cloud generation"
                        .to_string(),
                ))
            }
        };

        // 2. Validate project exists in ProjectManager
        self.project_manager
            .get_project(&project_id)
            .map_err(|e| CloudProviderError::RequestInvalid(format!("Project not found: {}", e)))?;

        // 3. Resolve internal safe job ID
        let internal_job_id = if request.job_id.starts_with("cjob-")
            && !request.job_id.contains('/')
            && !request.job_id.contains('\\')
        {
            request.job_id.clone()
        } else {
            format!("cjob-{}", Uuid::new_v4())
        };

        let lock = self.get_job_lock(&internal_job_id);
        let _guard = lock.lock().await;

        // 4. Check if job already exists on disk
        let mut job = match self.store.load_job(&project_id, &internal_job_id) {
            Ok(existing) => {
                // If already submitted or in flight, prevent duplicate paid submission!
                if existing.submission_state != SubmissionState::NeverAttempted {
                    return Err(CloudProviderError::RequestInvalid(format!(
                        "DUPLICATE_SUBMISSION_PREVENTED: Job {} has already been submitted (state: {:?})",
                        internal_job_id, existing.submission_state
                    )));
                }
                existing
            }
            Err(_) => {
                // Read source media & reference image hashes (Read-Only)
                let source_hash = match &request.source_video {
                    Some(path) if path.exists() => {
                        Some(CloudOutputValidator::compute_file_sha256(path)?)
                    }
                    _ => None,
                };
                let ref_hash = match &request.reference_image {
                    Some(path) if path.exists() => {
                        Some(CloudOutputValidator::compute_file_sha256(path)?)
                    }
                    _ => None,
                };

                let input_assets = InputAssets {
                    source_video_path: request.source_video.clone(),
                    source_video_hash: source_hash,
                    reference_image_path: request.reference_image.clone(),
                    reference_image_hash: ref_hash,
                };

                let config_hash = Self::compute_configuration_hash(
                    &request,
                    "replicate",
                    "minimax/video-01",
                    "minimax/video-01",
                );

                let task_class = TaskClass::from_str_or_default(&request.task_type);

                let cost_record = CostRecord {
                    estimate: None,
                    confidence: super::cost::CostConfidence::Estimated,
                    budget_limit: max_cost.unwrap_or(super::cost::DEFAULT_STANDARD_JOB_BUDGET_USD),
                    reserved_budget: None,
                    actual_cost: None,
                };

                let new_job = PersistentCloudJob::new(
                    request.job_id.clone(),
                    internal_job_id.clone(),
                    project_id.clone(),
                    "replicate".to_string(),
                    "minimax/video-01".to_string(),
                    "minimax/video-01".to_string(),
                    request.task_type.clone(),
                    task_class.execution_class(),
                    input_assets,
                    config_hash,
                    cost_record,
                );

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

        // 6. Authoritative Phase 14 routing & budget validation
        let plan = validate_and_prepare_cloud_submission(
            &request,
            max_cost,
            provider.as_ref(),
            &registry,
        )?;

        // 7. Transition to IN_FLIGHT and persist BEFORE calling provider.submit_job()
        job.state = CloudJobState::Submitted;
        job.submission_state = SubmissionState::InFlight;
        job.retry.submit_attempts = job.retry.submit_attempts.saturating_add(1);
        job.timestamps.submitted_at = Some(Utc::now().to_rfc3339());
        job.cost.estimate = Some(plan.routing_decision.estimated_cost);
        job.cost.budget_limit = plan.budget_limit;
        job.increment_revision();

        self.store.save_job_atomic(&job)?;
        let _ = self.event_sink.emit_job_updated(&job.to_event_payload());

        // 8. Submit to provider
        match provider.submit_job(&request).await {
            Ok(handle) => {
                // Acknowledged submission
                job.remote_job_id = Some(handle.remote_id);
                job.submission_state = SubmissionState::Acknowledged;
                job.state = CloudJobState::Processing;
                job.remote_status = Some("processing".to_string());
                job.increment_revision();

                self.store.save_job_atomic(&job)?;
                let _ = self.event_sink.emit_job_updated(&job.to_event_payload());

                // Spawn background polling task
                self.spawn_polling_task(job.clone());

                Ok(job)
            }
            Err(e) => {
                // Ambiguous submission or provider error
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
        internal_job_id: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        self.store.load_job(project_id, internal_job_id)
    }

    // -------------------------------------------------------------------------
    // Cancellation
    // -------------------------------------------------------------------------

    pub async fn cancel_cloud_generation(
        &self,
        project_id: &str,
        internal_job_id: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        let lock = self.get_job_lock(internal_job_id);
        let _guard = lock.lock().await;

        let mut job = self.store.load_job(project_id, internal_job_id)?;
        if job.state.is_terminal() {
            return Ok(job);
        }

        job.cancellation_requested = true;
        job.increment_revision();
        self.store.save_job_atomic(&job)?;
        let _ = self.event_sink.emit_job_updated(&job.to_event_payload());

        // Reconcile with remote provider if remote_job_id is known
        if let Some(r_id) = &job.remote_job_id {
            if let Ok(provider) = self.provider_resolver.resolve_provider(&job.provider_id) {
                let _ = provider.cancel_job(r_id).await;
            }
        }

        job.state = CloudJobState::Cancelled;
        job.remote_status = Some("canceled".to_string());
        job.increment_revision();
        self.store.save_job_atomic(&job)?;
        let _ = self.event_sink.emit_job_updated(&job.to_event_payload());

        Ok(job)
    }

    // -------------------------------------------------------------------------
    // Unblock / Resume Safe Polling on Resolved Credentials
    // -------------------------------------------------------------------------

    pub async fn resume_unblock_job(
        &self,
        project_id: &str,
        internal_job_id: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        let lock = self.get_job_lock(internal_job_id);
        let _guard = lock.lock().await;

        let mut job = self.store.load_job(project_id, internal_job_id)?;
        if job.remote_job_id.is_none() {
            return Err(CloudProviderError::RequestInvalid(
                "Cannot resume job without remoteJobId".to_string(),
            ));
        }

        // Verify provider is now resolvable
        let _ = self.provider_resolver.resolve_provider(&job.provider_id)?;

        job.state = CloudJobState::Processing;
        job.error = None;
        job.increment_revision();
        self.store.save_job_atomic(&job)?;
        let _ = self.event_sink.emit_job_updated(&job.to_event_payload());

        self.spawn_polling_task(job.clone());

        Ok(job)
    }

    // -------------------------------------------------------------------------
    // Startup Recovery
    // -------------------------------------------------------------------------

    pub fn recover_startup_jobs(&self) -> Result<Vec<PersistentCloudJob>, CloudProviderError> {
        let active_jobs = self.store.list_all_active_jobs()?;
        let mut recovered = Vec::new();

        for mut job in active_jobs {
            if job.cancellation_requested {
                job.state = CloudJobState::Cancelled;
                job.increment_revision();
                let _ = self.store.save_job_atomic(&job);
                let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
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
                    job.state = CloudJobState::Blocked;
                    job.error = Some(JobErrorRecord {
                        code: "UPLOAD_INTERRUPTED".to_string(),
                        sanitized_message: "Process restarted during upload phase".to_string(),
                    });
                    job.increment_revision();
                    let _ = self.store.save_job_atomic(&job);
                    let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
                    recovered.push(job);
                }
                CloudJobState::Submitted | CloudJobState::Processing => {
                    if let Some(_r_id) = &job.remote_job_id {
                        match self.provider_resolver.resolve_provider(&job.provider_id) {
                            Ok(_) => {
                                job.state = CloudJobState::Processing;
                                job.increment_revision();
                                let _ = self.store.save_job_atomic(&job);
                                self.spawn_polling_task(job.clone());
                                recovered.push(job);
                            }
                            Err(_) => {
                                // Missing provider credentials on restart -> safely block without deleting or resubmitting!
                                job.state = CloudJobState::Blocked;
                                job.error = Some(JobErrorRecord {
                                    code: "MISSING_PROVIDER_CREDENTIALS".to_string(),
                                    sanitized_message: format!(
                                        "Provider '{}' credentials unavailable upon restart. Polling paused safely.",
                                        job.provider_id
                                    ),
                                });
                                job.increment_revision();
                                let _ = self.store.save_job_atomic(&job);
                                let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
                                recovered.push(job);
                            }
                        }
                    } else {
                        // Submitted without remote ID -> Ambiguous
                        job.state = CloudJobState::Blocked;
                        job.submission_state = SubmissionState::Ambiguous;
                        job.error = Some(JobErrorRecord {
                            code: "AMBIGUOUS_SUBMISSION".to_string(),
                            sanitized_message: "Process crashed during submission without acknowledged remote ID. Auto-resubmission is disabled.".to_string(),
                        });
                        job.increment_revision();
                        let _ = self.store.save_job_atomic(&job);
                        let _ = self.event_sink.emit_job_updated(&job.to_event_payload());
                        recovered.push(job);
                    }
                }
                CloudJobState::Downloading => {
                    self.spawn_polling_task(job.clone());
                    recovered.push(job);
                }
                CloudJobState::ValidatingOutput => {
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
    // Background Polling & Output Promotion Loop
    // -------------------------------------------------------------------------

    fn spawn_polling_task(&self, mut job: PersistentCloudJob) {
        let store = self.store.clone();
        let provider_resolver = self.provider_resolver.clone();
        let event_sink = self.event_sink.clone();
        let timing = self.timing_config;
        let lock = self.get_job_lock(&job.internal_job_id);

        tokio::spawn(async move {
            let _guard = lock.lock().await;
            let provider = match provider_resolver.resolve_provider(&job.provider_id) {
                Ok(p) => p,
                Err(e) => {
                    job.state = CloudJobState::Blocked;
                    job.error = Some(JobErrorRecord {
                        code: "MISSING_PROVIDER_CREDENTIALS".to_string(),
                        sanitized_message: format!("{}", e),
                    });
                    job.increment_revision();
                    let _ = store.save_job_atomic(&job);
                    let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    return;
                }
            };

            let remote_id = match &job.remote_job_id {
                Some(r) => r.clone(),
                None => return,
            };

            let start_time = std::time::Instant::now();
            let mut consecutive_errors = 0;

            // 1. Polling Phase
            while job.state == CloudJobState::Processing || job.state == CloudJobState::Submitted {
                if job.cancellation_requested {
                    job.state = CloudJobState::Cancelled;
                    job.increment_revision();
                    let _ = store.save_job_atomic(&job);
                    let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    return;
                }

                if start_time.elapsed() >= Duration::from_secs(timing.max_poll_duration_sec) {
                    job.state = CloudJobState::Failed;
                    job.error = Some(JobErrorRecord {
                        code: "PROVIDER_TIMEOUT".to_string(),
                        sanitized_message: format!(
                            "Polling exceeded maximum duration limit of {}s",
                            timing.max_poll_duration_sec
                        ),
                    });
                    job.increment_revision();
                    let _ = store.save_job_atomic(&job);
                    let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    return;
                }

                job.retry.poll_attempts = job.retry.poll_attempts.saturating_add(1);

                match provider.poll_status(&remote_id).await {
                    Ok(poll_resp) => {
                        consecutive_errors = 0;
                        job.remote_status = Some(format!("{:?}", poll_resp.status));
                        if let Some(url) = poll_resp.output_url {
                            job.output_url = Some(url);
                        }

                        match poll_resp.status {
                            RemoteStatus::Starting | RemoteStatus::Processing => {
                                job.state = CloudJobState::Processing;
                                job.increment_revision();
                                let _ = store.save_job_atomic(&job);
                                let _ = event_sink.emit_job_updated(&job.to_event_payload());
                            }
                            RemoteStatus::Succeeded => {
                                job.state = CloudJobState::Downloading;
                                job.increment_revision();
                                let _ = store.save_job_atomic(&job);
                                let _ = event_sink.emit_job_updated(&job.to_event_payload());
                                break;
                            }
                            RemoteStatus::Failed => {
                                job.state = CloudJobState::Failed;
                                job.error = Some(JobErrorRecord {
                                    code: "PROVIDER_EXECUTION_FAILED".to_string(),
                                    sanitized_message: poll_resp
                                        .error
                                        .unwrap_or_else(|| "Provider job failed".to_string()),
                                });
                                job.increment_revision();
                                let _ = store.save_job_atomic(&job);
                                let _ = event_sink.emit_job_updated(&job.to_event_payload());
                                return;
                            }
                            RemoteStatus::Canceled => {
                                job.state = CloudJobState::Cancelled;
                                job.increment_revision();
                                let _ = store.save_job_atomic(&job);
                                let _ = event_sink.emit_job_updated(&job.to_event_payload());
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        if consecutive_errors >= timing.max_consecutive_poll_errors {
                            job.state = CloudJobState::Failed;
                            job.error = Some(JobErrorRecord {
                                code: "POLL_ERROR_LIMIT".to_string(),
                                sanitized_message: format!("Consecutive polling errors: {}", e),
                            });
                            job.increment_revision();
                            let _ = store.save_job_atomic(&job);
                            let _ = event_sink.emit_job_updated(&job.to_event_payload());
                            return;
                        }
                    }
                }

                tokio::time::sleep(Duration::from_millis(timing.poll_interval_ms)).await;
            }

            // 2. Download Phase
            if job.state == CloudJobState::Downloading {
                let partial_path =
                    match store.artifact_partial_path(&job.project_id, &job.internal_job_id) {
                        Ok(p) => p,
                        Err(e) => {
                            job.state = CloudJobState::Failed;
                            job.error = Some(JobErrorRecord {
                                code: "PATH_ERROR".to_string(),
                                sanitized_message: format!("{}", e),
                            });
                            job.increment_revision();
                            let _ = store.save_job_atomic(&job);
                            let _ = event_sink.emit_job_updated(&job.to_event_payload());
                            return;
                        }
                    };

                let output_url = job.output_url.clone().unwrap_or_default();
                let mut download_success = false;

                while job.retry.download_attempts < timing.max_download_attempts {
                    job.retry.download_attempts = job.retry.download_attempts.saturating_add(1);
                    job.increment_revision();
                    let _ = store.save_job_atomic(&job);

                    match provider.download_result(&output_url, &partial_path).await {
                        Ok(_) => {
                            download_success = true;
                            break;
                        }
                        Err(_) => {
                            tokio::time::sleep(Duration::from_millis(timing.poll_interval_ms))
                                .await;
                        }
                    }
                }

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
                    let _ = store.save_job_atomic(&job);
                    let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    return;
                }

                job.state = CloudJobState::ValidatingOutput;
                job.increment_revision();
                let _ = store.save_job_atomic(&job);
                let _ = event_sink.emit_job_updated(&job.to_event_payload());
            }

            // 3. Validation & Promotion Phase
            if job.state == CloudJobState::ValidatingOutput {
                let partial_path =
                    match store.artifact_partial_path(&job.project_id, &job.internal_job_id) {
                        Ok(p) => p,
                        Err(e) => {
                            job.state = CloudJobState::Failed;
                            job.error = Some(JobErrorRecord {
                                code: "PATH_ERROR".to_string(),
                                sanitized_message: format!("{}", e),
                            });
                            job.increment_revision();
                            let _ = store.save_job_atomic(&job);
                            let _ = event_sink.emit_job_updated(&job.to_event_payload());
                            return;
                        }
                    };

                let final_path =
                    match store.artifact_final_path(&job.project_id, &job.internal_job_id) {
                        Ok(p) => p,
                        Err(e) => {
                            job.state = CloudJobState::Failed;
                            job.error = Some(JobErrorRecord {
                                code: "PATH_ERROR".to_string(),
                                sanitized_message: format!("{}", e),
                            });
                            job.increment_revision();
                            let _ = store.save_job_atomic(&job);
                            let _ = event_sink.emit_job_updated(&job.to_event_payload());
                            return;
                        }
                    };

                let validator = CloudOutputValidator::new();
                match validator.validate_and_promote_artifact(
                    &partial_path,
                    &final_path,
                    None,
                    false,
                ) {
                    Ok(artifact_record) => {
                        job.output = artifact_record;
                        job.state = CloudJobState::Completed;
                        job.timestamps.completed_at = Some(Utc::now().to_rfc3339());
                        job.increment_revision();
                        let _ = store.save_job_atomic(&job);
                        let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    }
                    Err(e) => {
                        job.state = CloudJobState::Failed;
                        job.error = Some(JobErrorRecord {
                            code: "VALIDATION_FAILED".to_string(),
                            sanitized_message: format!("Media validation failed: {}", e),
                        });
                        job.increment_revision();
                        let _ = store.save_job_atomic(&job);
                        let _ = event_sink.emit_job_updated(&job.to_event_payload());
                    }
                }
            }
        });
    }
}
