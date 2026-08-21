use super::cache::SegmentCacheManager;
use super::cost::{CostGuard, DEFAULT_STANDARD_JOB_BUDGET_USD};
use super::error::CloudProviderError;
use super::job::{CloudJobRequest, CloudJobState, JobErrorRecord, OutputArtifactRecord};
use super::lifecycle::CloudJobLifecycleService;
use super::manifest::{SegmentedCloudJobManifest, SegmentedJobState};
use super::registry::ProviderRegistry;
use super::router::TaskClass;
use super::segment::{
    FinalAudioMuxer, SegmentPlanner, SegmentStitcher, DEFAULT_MAX_SEGMENT_DURATION_SEC,
};
use super::spec::{DetailedTimingFacts, SourceMediaFacts, SourceMediaProbe};
use super::store::SegmentedCloudJobStore;
use crate::system::StoragePaths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex as TokioMutex, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentedCloudSubmissionPreflight {
    pub task_class: TaskClass,
    pub segmentable: bool,
    pub estimated_segments: usize,
    pub source_facts: SourceMediaFacts,
    pub timing_facts: DetailedTimingFacts,
    pub provisional_cost_usd: f64,
    pub budget_limit: f64,
    pub budget_approved: bool,
    pub blocking_code: Option<String>,
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Clone)]
pub struct SegmentedCloudJobOrchestrator {
    lifecycle: Arc<CloudJobLifecycleService>,
    store: SegmentedCloudJobStore,
    paths: StoragePaths,
    registry: ProviderRegistry,
    parent_locks: Arc<RwLock<HashMap<String, Arc<TokioMutex<()>>>>>,
    app_handle: Option<tauri::AppHandle>,
}

impl SegmentedCloudJobOrchestrator {
    pub fn new(
        lifecycle: Arc<CloudJobLifecycleService>,
        store: SegmentedCloudJobStore,
        paths: StoragePaths,
        registry: ProviderRegistry,
        app_handle: Option<tauri::AppHandle>,
    ) -> Self {
        Self {
            lifecycle,
            store,
            paths,
            registry,
            parent_locks: Arc::new(RwLock::new(HashMap::new())),
            app_handle,
        }
    }

    async fn get_parent_request_lock(&self, lock_key: &str) -> Arc<TokioMutex<()>> {
        let mut map = self.parent_locks.write().await;
        map.entry(lock_key.to_string())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }

    fn emit_manifest_update(&self, manifest: &SegmentedCloudJobManifest) {
        if let Some(ref handle) = self.app_handle {
            use tauri::Emitter;
            let _ = handle.emit("segmented-cloud-job://updated", manifest);
        }
    }

    pub fn preflight_segmented_transformation(
        &self,
        request: &CloudJobRequest,
        max_cost: Option<f64>,
    ) -> Result<SegmentedCloudSubmissionPreflight, CloudProviderError> {
        let task_class = TaskClass::from_str_strict(&request.task_type)?;
        if task_class != TaskClass::BackgroundRemoval {
            return Err(CloudProviderError::RequestInvalid(format!(
                "UNSUPPORTED_SEGMENTED_TASK: Task {:?} does not support cloud segmentation",
                task_class
            )));
        }

        let source_path = request.source_video.as_ref().ok_or_else(|| {
            CloudProviderError::RequestInvalid(
                "SOURCE_VIDEO_REQUIRED: Source video is required for segmentation preflight"
                    .to_string(),
            )
        })?;

        let (source_facts, timing_facts) = SourceMediaProbe::probe_file_detailed(source_path)?;

        if timing_facts.is_vfr {
            return Err(CloudProviderError::RequestInvalid(
                "UNSUPPORTED_VFR_SEGMENTATION: Source video has variable frame rate (VFR) which is not supported for deterministic segmentation".to_string(),
            ));
        }

        let candidates = self
            .registry
            .find_candidates_for_task(TaskClass::BackgroundRemoval);
        let record = candidates.first().ok_or_else(|| {
            CloudProviderError::ProviderUnavailable(
                "No utility background removal provider registered".to_string(),
            )
        })?;

        let unit_rate = record.pricing_amount.unwrap_or(0.0042);
        let plan = SegmentPlanner::plan(
            &source_facts,
            &timing_facts,
            DEFAULT_MAX_SEGMENT_DURATION_SEC,
        )?;

        let provisional_cost_usd: f64 = plan
            .boundaries
            .iter()
            .map(|b| b.expected_duration_sec * unit_rate)
            .sum();

        let budget_limit = match max_cost {
            Some(val) => CostGuard::validate_budget(val)?,
            None => DEFAULT_STANDARD_JOB_BUDGET_USD,
        };

        let budget_approved = budget_limit >= provisional_cost_usd;
        let blocking_code = if !budget_approved {
            Some("COST_BUDGET_EXCEEDED".to_string())
        } else {
            None
        };

        Ok(SegmentedCloudSubmissionPreflight {
            task_class,
            segmentable: true,
            estimated_segments: plan.boundaries.len(),
            source_facts,
            timing_facts,
            provisional_cost_usd,
            budget_limit,
            budget_approved,
            blocking_code,
            provider_id: record.provider_id.clone(),
            model_id: record.model_id.clone(),
        })
    }

    pub async fn start_segmented_transformation(
        &self,
        request: CloudJobRequest,
        max_cost: Option<f64>,
    ) -> Result<SegmentedCloudJobManifest, CloudProviderError> {
        let project_id_str = request
            .project_id
            .clone()
            .unwrap_or_else(|| "default_project".to_string());
        let lock_key = format!("{}:{}", project_id_str, request.job_id);
        let request_lock = self.get_parent_request_lock(&lock_key).await;
        let _guard = request_lock.lock().await;

        let task_class = TaskClass::from_str_strict(&request.task_type)?;
        if task_class != TaskClass::BackgroundRemoval {
            return Err(CloudProviderError::RequestInvalid(format!(
                "UNSUPPORTED_SEGMENTED_TASK: Task {:?} does not support cloud segmentation",
                task_class
            )));
        }

        let source_path = request.source_video.as_ref().ok_or_else(|| {
            CloudProviderError::RequestInvalid(
                "SOURCE_VIDEO_REQUIRED: Source video is required for segmentation".to_string(),
            )
        })?;

        let source_checksum = SegmentCacheManager::compute_file_sha256(source_path)?;
        let (source_facts, timing_facts) = SourceMediaProbe::probe_file_detailed(source_path)?;

        if timing_facts.is_vfr {
            return Err(CloudProviderError::RequestInvalid(
                "UNSUPPORTED_VFR_SEGMENTATION: Source video has variable frame rate (VFR) which is not supported for deterministic segmentation".to_string(),
            ));
        }

        let candidates = self
            .registry
            .find_candidates_for_task(TaskClass::BackgroundRemoval);
        let record = candidates.first().ok_or_else(|| {
            CloudProviderError::ProviderUnavailable(
                "No utility background removal provider registered".to_string(),
            )
        })?;

        let unit_rate = record.pricing_amount.unwrap_or(0.0042);
        let segment_plan = SegmentPlanner::plan(
            &source_facts,
            &timing_facts,
            DEFAULT_MAX_SEGMENT_DURATION_SEC,
        )?;

        let provisional_cost_usd: f64 = segment_plan
            .boundaries
            .iter()
            .map(|b| b.expected_duration_sec * unit_rate)
            .sum();

        let budget_limit = match max_cost {
            Some(val) => CostGuard::validate_budget(val)?,
            None => DEFAULT_STANDARD_JOB_BUDGET_USD,
        };

        // Compute canonical configuration hash
        let mut config_hasher = sha2::Sha256::default();
        use sha2::Digest;
        config_hasher.update(source_checksum.as_bytes());
        config_hasher.update(b":");
        config_hasher.update(record.provider_id.as_bytes());
        config_hasher.update(b":");
        config_hasher.update(record.model_id.as_bytes());
        config_hasher.update(b":");
        config_hasher.update(segment_plan.policy_version.to_string().as_bytes());
        let configuration_hash = format!("{:x}", config_hasher.finalize());

        // Deduplication check: existing parent by client_request_id
        if let Some(existing) = self
            .store
            .find_parent_by_client_request_id(&project_id_str, &request.job_id)?
        {
            if existing.configuration_hash == configuration_hash {
                return Ok(existing);
            } else {
                return Err(CloudProviderError::RequestInvalid(format!(
                    "REQUEST_ID_CONFLICT: Parent job with ID '{}' already exists with a different configuration",
                    request.job_id
                )));
            }
        }

        let parent_id = format!("seg-{}", uuid::Uuid::new_v4());
        let mut manifest = SegmentedCloudJobManifest::new(
            parent_id.clone(),
            request.job_id.clone(),
            project_id_str.clone(),
            request.task_type.clone(),
            record.provider_id.clone(),
            record.model_id.clone(),
            configuration_hash,
            source_facts.clone(),
            timing_facts.clone(),
            segment_plan.clone(),
            Some(budget_limit),
            provisional_cost_usd,
        );

        self.store.save_manifest_atomic(&manifest)?;
        self.emit_manifest_update(&manifest);

        // Transition to Splitting
        manifest
            .transition_to(SegmentedJobState::Splitting)
            .map_err(|e| CloudProviderError::JobFailed(e))?;
        manifest.recalculate_progress();
        self.store.save_manifest_atomic(&manifest)?;
        self.emit_manifest_update(&manifest);

        let project_dir = self.paths.projects_dir.join(&project_id_str);
        let mut actual_total_cost = 0.0f64;

        for (i, boundary) in segment_plan.boundaries.iter().enumerate() {
            let (split_path, split_facts) = SegmentCacheManager::get_or_create_split_segment(
                &project_dir,
                source_path,
                &source_checksum,
                &source_facts,
                boundary,
                timing_facts.r_frame_rate.to_f64(),
                DEFAULT_MAX_SEGMENT_DURATION_SEC,
            )?;

            manifest.child_jobs[i].input_segment_path = Some(split_path);
            manifest.child_jobs[i].duration_sec = split_facts.duration_sec;
            let seg_cost = split_facts.duration_sec * unit_rate;
            actual_total_cost += seg_cost;
        }

        manifest.actual_batch_base_estimate_usd = Some(actual_total_cost);

        // Stage B Budget Guard Check
        if actual_total_cost > budget_limit {
            manifest
                .transition_to(SegmentedJobState::CostApprovalRequired)
                .map_err(|e| CloudProviderError::JobFailed(e))?;
            manifest.error = Some(JobErrorRecord {
                code: "COST_APPROVAL_REQUIRED".to_string(),
                sanitized_message: format!(
                    "Actual batch base estimate ${:.4} exceeds approved budget ${:.4}",
                    actual_total_cost, budget_limit
                ),
            });
            self.store.save_manifest_atomic(&manifest)?;
            self.emit_manifest_update(&manifest);
            return Ok(manifest);
        }

        manifest
            .transition_to(SegmentedJobState::Ready)
            .map_err(|e| CloudProviderError::JobFailed(e))?;
        self.store.save_manifest_atomic(&manifest)?;
        self.emit_manifest_update(&manifest);

        // Spawn background sequential runner
        let orchestrator_clone = self.clone();
        let parent_id_clone = parent_id.clone();
        let project_id_clone = project_id_str.clone();
        let request_clone = request.clone();

        tokio::spawn(async move {
            let _ = orchestrator_clone
                .run_segmented_job_worker(project_id_clone, parent_id_clone, request_clone)
                .await;
        });

        Ok(manifest)
    }

    pub async fn run_segmented_job_worker(
        &self,
        project_id: String,
        parent_id: String,
        request: CloudJobRequest,
    ) -> Result<(), CloudProviderError> {
        let mut manifest = self.store.load_manifest(&project_id, &parent_id)?;

        let artifacts_dir = self
            .paths
            .projects_dir
            .join(&project_id)
            .join("cloud-jobs")
            .join("artifacts");
        let final_dest_path = artifacts_dir.join(format!("{}.webm", parent_id));

        // Crash recovery check: if final artifact is already promoted on disk and decodable
        if final_dest_path.is_file() {
            if let Ok(output_facts) = SourceMediaProbe::probe_file(&final_dest_path) {
                if output_facts.width > 0
                    && output_facts.height > 0
                    && output_facts.duration_sec > 0.0
                {
                    if let Ok(_final_meta) = std::fs::metadata(&final_dest_path) {
                        let artifact_record = OutputArtifactRecord {
                            temporary_path: None,
                            final_path: Some(final_dest_path.clone()),
                            artifact_hash: None,
                            width: Some(output_facts.width),
                            height: Some(output_facts.height),
                            duration_sec: Some(output_facts.duration_sec),
                            fps: Some(output_facts.fps),
                        };
                        manifest.final_output = Some(artifact_record);
                        if manifest.state != SegmentedJobState::Completed {
                            let _ = manifest.transition_to(SegmentedJobState::Completed);
                            manifest.recalculate_progress();
                            let _ = self.store.save_manifest_atomic(&manifest);
                            self.emit_manifest_update(&manifest);
                        }
                        return Ok(());
                    }
                }
            }
        }

        if manifest.state == SegmentedJobState::Ready {
            let _ = manifest.transition_to(SegmentedJobState::Running);
            manifest.recalculate_progress();
            let _ = self.store.save_manifest_atomic(&manifest);
            self.emit_manifest_update(&manifest);
        }

        let candidates = self
            .registry
            .find_candidates_for_task(TaskClass::BackgroundRemoval);
        let record = candidates.first().ok_or_else(|| {
            CloudProviderError::ProviderUnavailable(
                "No utility background removal provider registered".to_string(),
            )
        })?;
        let unit_rate = record.pricing_amount.unwrap_or(0.0042);

        let total_children = manifest.child_jobs.len();
        let mut child_artifacts = Vec::with_capacity(total_children);

        for i in 0..total_children {
            // Check cancellation
            manifest = self.store.load_manifest(&project_id, &parent_id)?;
            if manifest.cancellation_requested {
                let _ = manifest.transition_to(SegmentedJobState::Cancelled);
                let _ = self.store.save_manifest_atomic(&manifest);
                self.emit_manifest_update(&manifest);
                return Ok(());
            }

            let child_record = &manifest.child_jobs[i];
            let child_client_id = child_record.client_job_id.clone();
            let input_segment_path = child_record.input_segment_path.clone().ok_or_else(|| {
                CloudProviderError::JobFailed(format!("Missing input segment path for child {}", i))
            })?;

            // Level A check: Check if child already completed in store
            let existing_child = self
                .lifecycle
                .store()
                .find_job_by_client_request_id(&project_id, &child_client_id)
                .unwrap_or(None);

            let (artifact_path, child_cost) = if let Some(ref cjob) = existing_child {
                if cjob.state == CloudJobState::Completed
                    && cjob
                        .output
                        .final_path
                        .as_ref()
                        .map(|p| p.exists())
                        .unwrap_or(false)
                {
                    let path = cjob.output.final_path.clone().unwrap();
                    let cost = cjob
                        .cost
                        .actual_cost
                        .unwrap_or(child_record.duration_sec * unit_rate);
                    (path, cost)
                } else {
                    self.dispatch_and_await_child(
                        &project_id,
                        &parent_id,
                        i,
                        &child_client_id,
                        &input_segment_path,
                        child_record.duration_sec,
                        &request,
                    )
                    .await?
                }
            } else {
                self.dispatch_and_await_child(
                    &project_id,
                    &parent_id,
                    i,
                    &child_client_id,
                    &input_segment_path,
                    child_record.duration_sec,
                    &request,
                )
                .await?
            };

            manifest = self.store.load_manifest(&project_id, &parent_id)?;
            manifest.child_jobs[i].state = Some(CloudJobState::Completed);
            manifest.child_jobs[i].output_artifact_path = Some(artifact_path.clone());
            manifest.child_jobs[i].cost_usd = Some(child_cost);
            manifest.child_jobs[i].updated_at = chrono::Utc::now().to_rfc3339();
            manifest.recalculate_progress();
            let _ = self.store.save_manifest_atomic(&manifest);
            self.emit_manifest_update(&manifest);

            child_artifacts.push(artifact_path);
        }

        // All children completed -> Transition to Stitching
        manifest = self.store.load_manifest(&project_id, &parent_id)?;
        manifest
            .transition_to(SegmentedJobState::Stitching)
            .map_err(|e| CloudProviderError::JobFailed(e))?;
        manifest.recalculate_progress();
        let _ = self.store.save_manifest_atomic(&manifest);
        self.emit_manifest_update(&manifest);

        let artifacts_dir = self
            .paths
            .projects_dir
            .join(&project_id)
            .join("cloud-jobs")
            .join("artifacts");
        let _ = std::fs::create_dir_all(&artifacts_dir);

        let temp_stitched_path = artifacts_dir.join(format!("{}_stitched_temp.webm", parent_id));
        SegmentStitcher::stitch_segments(&child_artifacts, &temp_stitched_path)?;

        // Check if final audio muxing is required
        let final_staged_path = if let Some(ref src_path) = request.source_video {
            if manifest.source_facts.has_audio {
                let temp_muxed_path = artifacts_dir.join(format!("{}_muxed_temp.webm", parent_id));
                FinalAudioMuxer::mux_original_audio(
                    &temp_stitched_path,
                    src_path,
                    &temp_muxed_path,
                )?;
                let _ = std::fs::remove_file(&temp_stitched_path);
                temp_muxed_path
            } else {
                temp_stitched_path
            }
        } else {
            temp_stitched_path
        };

        // Transition to ValidatingOutput
        manifest = self.store.load_manifest(&project_id, &parent_id)?;
        let _ = manifest.transition_to(SegmentedJobState::ValidatingOutput);
        manifest.recalculate_progress();
        let _ = self.store.save_manifest_atomic(&manifest);
        self.emit_manifest_update(&manifest);

        let final_dest_path = artifacts_dir.join(format!("{}.webm", parent_id));
        std::fs::rename(&final_staged_path, &final_dest_path).map_err(|e| {
            CloudProviderError::JobFailed(format!(
                "Failed to promote final segmented artifact to {}: {}",
                final_dest_path.display(),
                e
            ))
        })?;

        let output_facts = SourceMediaProbe::probe_file(&final_dest_path)?;

        let artifact_record = OutputArtifactRecord {
            temporary_path: None,
            final_path: Some(final_dest_path.clone()),
            artifact_hash: None,
            width: Some(output_facts.width),
            height: Some(output_facts.height),
            duration_sec: Some(output_facts.duration_sec),
            fps: Some(output_facts.fps),
        };

        manifest = self.store.load_manifest(&project_id, &parent_id)?;
        manifest.final_output = Some(artifact_record);
        let _ = manifest.transition_to(SegmentedJobState::Completed);
        manifest.recalculate_progress();
        let _ = self.store.save_manifest_atomic(&manifest);
        self.emit_manifest_update(&manifest);

        Ok(())
    }

    async fn dispatch_and_await_child(
        &self,
        project_id: &str,
        parent_id: &str,
        segment_index: usize,
        child_client_id: &str,
        input_segment_path: &Path,
        segment_duration_sec: f64,
        parent_request: &CloudJobRequest,
    ) -> Result<(PathBuf, f64), CloudProviderError> {
        let child_req = CloudJobRequest {
            job_id: child_client_id.to_string(),
            project_id: Some(project_id.to_string()),
            task_type: "BACKGROUND_REMOVAL".to_string(),
            prompt: String::new(),
            negative_prompt: None,
            source_video: Some(input_segment_path.to_path_buf()),
            duration_seconds: segment_duration_sec,
            resolution: parent_request.resolution,
            fps: parent_request.fps,
            reference_image: None,
            reference_images: None,
        };

        let child_job = self
            .lifecycle
            .start_cloud_generation(child_req, Some(3.0))
            .await?;

        // Update internal job id in parent manifest
        let mut manifest = self.store.load_manifest(project_id, parent_id)?;
        manifest.child_jobs[segment_index].internal_job_id =
            Some(child_job.internal_job_id.clone());
        manifest.child_jobs[segment_index].state = Some(child_job.state);
        let _ = self.store.save_manifest_atomic(&manifest);
        self.emit_manifest_update(&manifest);

        // Await child completion in loop
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            let current_child = self
                .lifecycle
                .get_job_status(project_id, &child_job.internal_job_id)?;

            match current_child.state {
                CloudJobState::Completed => {
                    let final_path = current_child.output.final_path.ok_or_else(|| {
                        CloudProviderError::JobFailed(
                            "Child completed but missing output final_path".to_string(),
                        )
                    })?;
                    let cost = current_child
                        .cost
                        .actual_cost
                        .unwrap_or(segment_duration_sec * 0.0042);
                    return Ok((final_path, cost));
                }
                CloudJobState::Failed => {
                    let err_msg = current_child
                        .error
                        .map(|e| e.sanitized_message)
                        .unwrap_or_else(|| "Child job failed".to_string());
                    let mut m = self.store.load_manifest(project_id, parent_id)?;
                    m.error = Some(JobErrorRecord {
                        code: "CHILD_SEGMENT_FAILED".to_string(),
                        sanitized_message: format!("Segment {} failed: {}", segment_index, err_msg),
                    });
                    let _ = m.transition_to(SegmentedJobState::Failed);
                    let _ = self.store.save_manifest_atomic(&m);
                    self.emit_manifest_update(&m);
                    return Err(CloudProviderError::JobFailed(format!(
                        "CHILD_SEGMENT_FAILED: Segment {} failed: {}",
                        segment_index, err_msg
                    )));
                }
                CloudJobState::Blocked => {
                    let err_msg = current_child
                        .error
                        .map(|e| e.sanitized_message)
                        .unwrap_or_else(|| "Child job blocked".to_string());
                    let mut m = self.store.load_manifest(project_id, parent_id)?;
                    m.error = Some(JobErrorRecord {
                        code: "CHILD_SEGMENT_BLOCKED".to_string(),
                        sanitized_message: format!(
                            "Segment {} blocked: {}",
                            segment_index, err_msg
                        ),
                    });
                    let _ = m.transition_to(SegmentedJobState::Blocked);
                    let _ = self.store.save_manifest_atomic(&m);
                    self.emit_manifest_update(&m);
                    return Err(CloudProviderError::ProviderUnavailable(format!(
                        "CHILD_SEGMENT_BLOCKED: Segment {} blocked: {}",
                        segment_index, err_msg
                    )));
                }
                CloudJobState::Cancelled => {
                    let mut m = self.store.load_manifest(project_id, parent_id)?;
                    let _ = m.transition_to(SegmentedJobState::Cancelled);
                    let _ = self.store.save_manifest_atomic(&m);
                    self.emit_manifest_update(&m);
                    return Err(CloudProviderError::JobFailed(
                        "CHILD_SEGMENT_CANCELLED".to_string(),
                    ));
                }
                _ => {}
            }
        }
    }

    pub async fn approve_segmented_budget(
        &self,
        project_id: &str,
        parent_id: &str,
        new_max_cost: f64,
    ) -> Result<SegmentedCloudJobManifest, CloudProviderError> {
        let lock_key = format!("{}:{}", project_id, parent_id);
        let parent_lock = self.get_parent_request_lock(&lock_key).await;
        let _guard = parent_lock.lock().await;

        let mut manifest = self.store.load_manifest(project_id, parent_id)?;
        if manifest.state != SegmentedJobState::CostApprovalRequired {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_STATE: Parent job is in {:?}, not CostApprovalRequired",
                manifest.state
            )));
        }

        let approved_budget = CostGuard::validate_budget(new_max_cost)?;
        if let Some(actual) = manifest.actual_batch_base_estimate_usd {
            if approved_budget < actual {
                return Err(CloudProviderError::RequestInvalid(format!(
                    "BUDGET_TOO_LOW: Approved budget ${:.4} is lower than required batch estimate ${:.4}",
                    approved_budget, actual
                )));
            }
        }

        manifest.budget_limit = Some(approved_budget);
        manifest.error = None;
        manifest
            .transition_to(SegmentedJobState::Ready)
            .map_err(|e| CloudProviderError::JobFailed(e))?;
        manifest.recalculate_progress();
        self.store.save_manifest_atomic(&manifest)?;
        self.emit_manifest_update(&manifest);

        let orchestrator_clone = self.clone();
        let parent_id_clone = parent_id.to_string();
        let project_id_clone = project_id.to_string();
        let request = CloudJobRequest {
            job_id: manifest.client_request_id.clone(),
            project_id: Some(project_id.to_string()),
            task_type: manifest.task_type.clone(),
            prompt: String::new(),
            negative_prompt: None,
            source_video: None,
            duration_seconds: manifest.source_facts.duration_sec,
            resolution: (manifest.source_facts.width, manifest.source_facts.height),
            fps: manifest.source_facts.fps,
            reference_image: None,
            reference_images: None,
        };

        tokio::spawn(async move {
            let _ = orchestrator_clone
                .run_segmented_job_worker(project_id_clone, parent_id_clone, request)
                .await;
        });

        Ok(manifest)
    }

    pub async fn cancel_segmented_transformation(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<SegmentedCloudJobManifest, CloudProviderError> {
        let lock_key = format!("{}:{}", project_id, parent_id);
        let parent_lock = self.get_parent_request_lock(&lock_key).await;
        let _guard = parent_lock.lock().await;

        let mut manifest = self.store.load_manifest(project_id, parent_id)?;
        if manifest.state.is_terminal() {
            return Ok(manifest);
        }

        manifest.cancellation_requested = true;

        // Cancel active child job if any
        for child in &manifest.child_jobs {
            if let Some(ref internal_id) = child.internal_job_id {
                if child.state.map(|s| !s.is_terminal()).unwrap_or(true) {
                    let _ = self
                        .lifecycle
                        .cancel_cloud_generation(project_id, internal_id)
                        .await;
                }
            }
        }

        let _ = manifest.transition_to(SegmentedJobState::Cancelled);
        manifest.recalculate_progress();
        self.store.save_manifest_atomic(&manifest)?;
        self.emit_manifest_update(&manifest);

        Ok(manifest)
    }

    pub fn list_segmented_jobs_in_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<SegmentedCloudJobManifest>, CloudProviderError> {
        self.store.list_segmented_jobs(project_id)
    }

    pub async fn recover_startup_segmented_jobs(&self) -> Result<(), CloudProviderError> {
        let projects_dir = &self.paths.projects_dir;
        if !projects_dir.exists() {
            return Ok(());
        }

        let entries = match std::fs::read_dir(projects_dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let project_id = entry.file_name().to_string_lossy().to_string();
                if let Ok(manifests) = self.list_segmented_jobs_in_project(&project_id) {
                    for m in manifests {
                        if !m.state.is_terminal()
                            && m.state != SegmentedJobState::CostApprovalRequired
                        {
                            let request = CloudJobRequest {
                                job_id: m.client_request_id.clone(),
                                project_id: Some(project_id.clone()),
                                task_type: m.task_type.clone(),
                                prompt: String::new(),
                                negative_prompt: None,
                                source_video: None,
                                duration_seconds: m.source_facts.duration_sec,
                                resolution: (m.source_facts.width, m.source_facts.height),
                                fps: m.source_facts.fps,
                                reference_image: None,
                                reference_images: None,
                            };
                            let orchestrator_clone = self.clone();
                            let pid = project_id.clone();
                            let parent_id = m.parent_id.clone();
                            tokio::spawn(async move {
                                let _ = orchestrator_clone
                                    .run_segmented_job_worker(pid, parent_id, request)
                                    .await;
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
