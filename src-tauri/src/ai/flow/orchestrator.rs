use super::capability::FlowCapabilityPolicy;
use super::manifest::{
    FlowChildSubmissionState, FlowFinalAudioPolicy, FlowGenerationManifest, FlowJobSnapshot,
    FlowJobState,
};
use super::output_validator::FlowOutputValidator;
use super::playwright_bridge::PlaywrightBridge;
use super::profile::FlowProfileManager;
use super::prompt_optimizer::{calculate_prompt_hash, PromptSource};
use super::segment::FlowVideoSegmenter;
use super::stitcher::FlowStitcher;
use super::store::FlowJobStore;
use crate::ai::cloud::job::JobErrorRecord;
use crate::ai::cloud::spec::SourceMediaProbe;
use crate::system::StoragePaths;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FlowOrchestrator {
    storage_paths: StoragePaths,
    store: FlowJobStore,
    profile_manager: FlowProfileManager,
    bridge: PlaywrightBridge,
    capability_policy: FlowCapabilityPolicy,
}

impl FlowOrchestrator {
    pub fn new(storage_paths: StoragePaths) -> Self {
        let store = FlowJobStore::new(storage_paths.clone());
        let profile_manager = FlowProfileManager::new(storage_paths.app_data_dir.clone());
        let bridge = PlaywrightBridge::new();
        let capability_policy = FlowCapabilityPolicy::for_edit_uploaded_video();

        Self {
            storage_paths,
            store,
            profile_manager,
            bridge,
            capability_policy,
        }
    }

    pub fn with_mock_bridge(storage_paths: StoragePaths, mock_url: String) -> Self {
        let store = FlowJobStore::new(storage_paths.clone());
        let profile_manager = FlowProfileManager::new(storage_paths.app_data_dir.clone());
        let bridge = PlaywrightBridge::with_mock_url(mock_url);
        let capability_policy = FlowCapabilityPolicy::for_edit_uploaded_video();

        Self {
            storage_paths,
            store,
            profile_manager,
            bridge,
            capability_policy,
        }
    }

    pub fn storage_paths(&self) -> &StoragePaths {
        &self.storage_paths
    }

    pub fn store(&self) -> &FlowJobStore {
        &self.store
    }

    pub fn profile_manager(&self) -> &FlowProfileManager {
        &self.profile_manager
    }

    pub fn capability_policy(&self) -> &FlowCapabilityPolicy {
        &self.capability_policy
    }

    pub async fn start_flow_generation(
        &self,
        project_id: String,
        profile_id: String,
        prompt: String,
        prompt_source: Option<PromptSource>,
        source_video_path: PathBuf,
    ) -> Result<FlowJobSnapshot, String> {
        let clean_prompt = prompt.trim();
        if clean_prompt.is_empty() {
            return Err("REQUEST_INVALID: Prompt cannot be empty".to_string());
        }

        if !source_video_path.exists() {
            return Err(format!(
                "FILE_NOT_FOUND: Source video does not exist: {:?}",
                source_video_path
            ));
        }

        // Verify profile exists
        let profile_dir = self.profile_manager.get_profile_dir(&profile_id)?;
        if !profile_dir.exists() {
            return Err(format!(
                "PROFILE_NOT_FOUND: Profile {} does not exist",
                profile_id
            ));
        }

        // Probe source video
        let facts = SourceMediaProbe::probe_file(&source_video_path)
            .map_err(|e| format!("PROBE_FAILED: {}", e))?;

        if facts.duration_sec <= 0.0 || facts.fps <= 0.0 {
            return Err("INVALID_MEDIA: Media facts have invalid duration or fps".to_string());
        }

        // Plan segments using largest legal boundary
        let plan = FlowVideoSegmenter::plan_segments(&facts, &self.capability_policy)?;

        let parent_id = format!("flow_{}", uuid::Uuid::new_v4());
        let client_request_id = format!("req_{}", Utc::now().timestamp_millis());
        let submitted_prompt = clean_prompt.to_string();
        let prompt_hash = calculate_prompt_hash(&submitted_prompt);
        let source_provenance = prompt_source.unwrap_or(PromptSource::User);

        // Derive deterministic config hash
        let mut hasher = Sha256::new();
        hasher.update(parent_id.as_bytes());
        hasher.update(submitted_prompt.as_bytes());
        hasher.update(source_video_path.to_string_lossy().as_bytes());
        let config_hash = format!("{:x}", hasher.finalize());

        let mut credit_record = super::capability::FlowCreditRecord::default();
        credit_record.estimated_credits =
            self.capability_policy.estimate_credits(plan.segments.len());

        let mut manifest = FlowGenerationManifest::new(
            parent_id.clone(),
            client_request_id,
            project_id.clone(),
            profile_id,
            config_hash,
            None,
            prompt_hash,
            None,
            submitted_prompt,
            calculate_prompt_hash(clean_prompt),
            source_provenance,
            self.capability_policy.capability_policy_version,
            self.capability_policy.split_policy_version,
            facts.clone(),
            plan.clone(),
            credit_record,
            FlowFinalAudioPolicy::default(),
        );

        manifest.state = FlowJobState::Ready;
        self.store.save_manifest_atomic(&mut manifest)?;

        let snapshot = manifest.to_snapshot();

        // Spawn sequential worker
        let orchestrator_clone = self.clone();
        let project_id_clone = project_id;
        let parent_id_clone = parent_id;
        let source_video_clone = source_video_path;

        tokio::spawn(async move {
            let _ = orchestrator_clone
                .run_flow_worker(&project_id_clone, &parent_id_clone, &source_video_clone)
                .await;
        });

        Ok(snapshot)
    }

    pub async fn run_flow_worker(
        &self,
        project_id: &str,
        parent_id: &str,
        source_video_path: &Path,
    ) -> Result<(), String> {
        let mut manifest = self.store.load_manifest(project_id, parent_id)?;

        let profile_dir = self.profile_manager.get_profile_dir(&manifest.profile_id)?;
        let _guard = match self
            .profile_manager
            .acquire_session_lock(&manifest.profile_id)
        {
            Ok(g) => g,
            Err(e) => {
                manifest.state = FlowJobState::Blocked;
                manifest.error = Some(JobErrorRecord {
                    code: "PROFILE_LOCK_FAILED".to_string(),
                    sanitized_message: e,
                });
                self.store.save_manifest_atomic(&mut manifest)?;
                return Ok(());
            }
        };

        let flow_dir = self.store.parent_flow_job_dir(project_id, parent_id)?;
        let segments_dir = flow_dir.join("input_segments");
        let outputs_dir = flow_dir.join("output_segments");
        let _ = std::fs::create_dir_all(&outputs_dir);

        // 1. Splitting Phase
        if manifest.child_segments.is_empty() {
            manifest.state = FlowJobState::Splitting;
            self.store.save_manifest_atomic(&mut manifest)?;

            let children = FlowVideoSegmenter::split_and_prepare_segments(
                source_video_path,
                &manifest.source_facts,
                &manifest.segment_plan,
                &segments_dir,
            )?;

            manifest.child_segments = children;
            manifest.state = FlowJobState::ReadyToSubmit;
            self.store.save_manifest_atomic(&mut manifest)?;
        }

        // 2. Sequential Browser Generation Phase
        let frozen_prompt = manifest.submitted_prompt.clone();
        let total_segments = manifest.child_segments.len();

        for i in 0..total_segments {
            if manifest.cancellation_requested {
                manifest.state = FlowJobState::Cancelled;
                self.store.save_manifest_atomic(&mut manifest)?;
                return Ok(());
            }

            let sub_state = manifest.child_segments[i].submission_state;
            if manifest.child_segments[i].state == FlowJobState::Completed
                || sub_state == FlowChildSubmissionState::ProvenCompleted
            {
                continue;
            }

            // CRASH RECOVERY POLICY CHECK:
            let submission_evidence = match sub_state {
                FlowChildSubmissionState::ProvenSubmitted => {
                    // ZERO submit! Resume polling directly using existing persisted evidence
                    manifest.child_segments[i]
                        .submission_evidence
                        .clone()
                        .unwrap_or_default()
                }
                FlowChildSubmissionState::AttemptPersisted
                | FlowChildSubmissionState::Ambiguous => {
                    // Crash happened in the Generate click window -> ZERO automatic resubmit!
                    manifest.state = FlowJobState::GenerationAmbiguous;
                    manifest.child_segments[i].state = FlowJobState::GenerationAmbiguous;
                    manifest.child_segments[i].submission_state =
                        FlowChildSubmissionState::Ambiguous;
                    manifest.error = Some(JobErrorRecord {
                        code: "GENERATION_AMBIGUOUS".to_string(),
                        sanitized_message: "Unconfirmed generation attempt requires user action or UI reconciliation".to_string(),
                    });
                    self.store.save_manifest_atomic(&mut manifest)?;
                    return Ok(());
                }
                FlowChildSubmissionState::NeverAttempted => {
                    let seg_filename = manifest.child_segments[i].segment_file_name.clone();
                    let seg_duration = manifest.child_segments[i].duration_sec;

                    manifest.active_segment_index = i;
                    manifest.state = FlowJobState::Submitting;
                    manifest.child_segments[i].state = FlowJobState::Submitting;

                    // Before click: Persist local submission attempt state FIRST!
                    let attempt_id = format!("att_{}_{}", i, Utc::now().timestamp_millis());
                    manifest.child_segments[i].local_submission_attempt_id =
                        Some(attempt_id.clone());
                    manifest.child_segments[i].submission_state =
                        FlowChildSubmissionState::AttemptPersisted;
                    self.store.save_manifest_atomic(&mut manifest)?;

                    let seg_input_path = segments_dir.join(&seg_filename);

                    // Execute ONE browser submission via Playwright bridge
                    match self
                        .bridge
                        .submit_generation(
                            &profile_dir,
                            &frozen_prompt,
                            Some(&seg_input_path),
                            seg_duration,
                            &attempt_id,
                        )
                        .await
                    {
                        Ok(ev) => {
                            manifest.child_segments[i].submission_state =
                                FlowChildSubmissionState::ProvenSubmitted;
                            manifest.child_segments[i].submission_evidence = Some(ev.clone());
                            manifest.state = FlowJobState::Generating;
                            manifest.child_segments[i].state = FlowJobState::Generating;
                            self.store.save_manifest_atomic(&mut manifest)?;
                            ev
                        }
                        Err(e) => {
                            manifest.state = FlowJobState::GenerationAmbiguous;
                            manifest.child_segments[i].state = FlowJobState::GenerationAmbiguous;
                            manifest.child_segments[i].submission_state =
                                FlowChildSubmissionState::Ambiguous;
                            manifest.error = Some(JobErrorRecord {
                                code: "SUBMISSION_FAILED".to_string(),
                                sanitized_message: e,
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                    }
                }
                FlowChildSubmissionState::ProvenCompleted => continue,
            };

            // 3. Poll until complete (with timeout and sleep)
            let poll_start = Utc::now();
            let poll_timeout = std::time::Duration::from_secs(600); // 10 minutes max for video generation
            let mut is_completed = false;

            while !is_completed {
                if manifest.cancellation_requested {
                    manifest.state = FlowJobState::Cancelled;
                    self.store.save_manifest_atomic(&mut manifest)?;
                    return Ok(());
                }

                if Utc::now().signed_duration_since(poll_start).num_seconds()
                    > poll_timeout.as_secs() as i64
                {
                    manifest.state = FlowJobState::Failed;
                    manifest.child_segments[i].state = FlowJobState::Failed;
                    manifest.error = Some(JobErrorRecord {
                        code: "GENERATION_TIMEOUT".to_string(),
                        sanitized_message:
                            "Flow generation exceeded maximum polling duration of 10 minutes"
                                .to_string(),
                    });
                    self.store.save_manifest_atomic(&mut manifest)?;
                    return Ok(());
                }

                let poll_result = self
                    .bridge
                    .poll_generation(&profile_dir, &submission_evidence)
                    .await?;

                match poll_result.status.as_str() {
                    "login_required" => {
                        manifest.state = FlowJobState::LoginRequired;
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }
                    "credits_required" => {
                        manifest.state = FlowJobState::CreditsRequired;
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }
                    "ui_changed" => {
                        manifest.state = FlowJobState::FlowUiChanged;
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }
                    "failed" => {
                        manifest.state = FlowJobState::Failed;
                        manifest.child_segments[i].state = FlowJobState::Failed;
                        manifest.error = Some(JobErrorRecord {
                            code: "GENERATION_FAILED".to_string(),
                            sanitized_message: poll_result
                                .error_message
                                .unwrap_or_else(|| "Flow generation failed".to_string()),
                        });
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }
                    "ready" => {
                        let download_url = poll_result
                            .download_url
                            .unwrap_or_else(|| "/download".to_string());
                        let seg_out_name = format!("child_out_{:03}.mp4", i);
                        let seg_out_path = outputs_dir.join(&seg_out_name);

                        manifest.state = FlowJobState::Downloading;
                        manifest.child_segments[i].state = FlowJobState::Downloading;
                        self.store.save_manifest_atomic(&mut manifest)?;

                        self.bridge
                            .download_artifact(&profile_dir, &download_url, &seg_out_path)
                            .await?;

                        // Validate segment
                        manifest.state = FlowJobState::ValidatingSegment;
                        manifest.child_segments[i].state = FlowJobState::ValidatingSegment;
                        self.store.save_manifest_atomic(&mut manifest)?;

                        let val_rec = FlowOutputValidator::validate_child_artifact(
                            &seg_out_path,
                            manifest.child_segments[i].duration_sec,
                        )?;

                        manifest.child_segments[i].download_artifact_path = Some(seg_out_path);
                        manifest.child_segments[i].download_artifact_sha = Some(val_rec.sha256);
                        manifest.child_segments[i].state = FlowJobState::Completed;
                        manifest.child_segments[i].submission_state =
                            FlowChildSubmissionState::ProvenCompleted;
                        manifest.credit_record.completed_generations += 1;
                        self.store.save_manifest_atomic(&mut manifest)?;
                        is_completed = true;
                    }
                    _ => {
                        // Generating/Queued -> Sleep briefly before next poll
                        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    }
                }
            }
        }

        // 4. Final Stitching Phase
        manifest.state = FlowJobState::Stitching;
        self.store.save_manifest_atomic(&mut manifest)?;

        let mut downloaded_segments = Vec::new();
        for child in &manifest.child_segments {
            if let Some(ref p) = child.download_artifact_path {
                downloaded_segments.push(p.clone());
            }
        }

        if downloaded_segments.len() != manifest.child_segments.len() {
            manifest.state = FlowJobState::Failed;
            manifest.error = Some(JobErrorRecord {
                code: "STITCH_INCOMPLETE".to_string(),
                sanitized_message: "Not all segment artifacts are downloaded".to_string(),
            });
            self.store.save_manifest_atomic(&mut manifest)?;
            return Ok(());
        }

        let final_video_out = flow_dir.join("final_flow_output.mp4");
        let stitched_record = FlowStitcher::stitch_flow_segments(
            &downloaded_segments,
            Some(source_video_path),
            manifest.source_facts.duration_sec,
            &manifest.final_audio_policy,
            &final_video_out,
        )?;

        // Validate final output
        manifest.state = FlowJobState::ValidatingFinal;
        self.store.save_manifest_atomic(&mut manifest)?;

        manifest.final_output = Some(stitched_record);

        manifest.state = FlowJobState::Completed;
        self.store.save_manifest_atomic(&mut manifest)?;

        Ok(())
    }
}
