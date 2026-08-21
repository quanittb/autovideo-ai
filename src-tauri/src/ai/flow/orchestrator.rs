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
        let capability_policy = FlowCapabilityPolicy::default();

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
        let capability_policy = FlowCapabilityPolicy::default();

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

        let source_bytes = std::fs::read(&source_video_path).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(&source_bytes);
        let source_content_hash = format!("{:x}", hasher.finalize());
        let source_file_name = source_video_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string());

        let credit_est = self.capability_policy.estimate_credits(plan.segments.len());
        let credit_record = super::capability::FlowCreditRecord {
            estimated_credits: credit_est,
            observed_credit_balance: None,
            completed_generations: 0,
        };

        let mut manifest = FlowGenerationManifest::new(
            parent_id.clone(),
            client_request_id,
            project_id.clone(),
            profile_id,
            format!("cfg_{}", prompt_hash),
            None,
            source_content_hash,
            source_file_name,
            submitted_prompt,
            prompt_hash,
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

        // Acquire profile lock
        let _guard = match self.profile_manager.try_lock_profile(&manifest.profile_id) {
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
        std::fs::create_dir_all(&outputs_dir).map_err(|e| format!("{}", e))?;

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

            if manifest.child_segments[i].state == FlowJobState::Completed
                || manifest.child_segments[i].submission_state
                    == FlowChildSubmissionState::ProvenCompleted
            {
                continue;
            }

            let seg_filename = manifest.child_segments[i].segment_file_name.clone();
            let seg_duration = manifest.child_segments[i].duration_sec;

            manifest.active_segment_index = i;
            manifest.state = FlowJobState::Submitting;
            manifest.child_segments[i].state = FlowJobState::Submitting;

            // Before click: Persist local submission attempt state FIRST!
            let attempt_id = format!("att_{}_{}", i, Utc::now().timestamp_millis());
            manifest.child_segments[i].local_submission_attempt_id = Some(attempt_id.clone());
            manifest.child_segments[i].submission_state =
                FlowChildSubmissionState::AttemptPersisted;
            self.store.save_manifest_atomic(&mut manifest)?;

            let seg_input_path = segments_dir.join(&seg_filename);

            // Execute browser submission via Playwright bridge
            let submission_evidence = match self
                .bridge
                .submit_generation(&frozen_prompt, Some(&seg_input_path), seg_duration)
                .await
            {
                Ok(ev) => ev,
                Err(e) => {
                    // Transition to GenerationAmbiguous if attempt was persisted but unconfirmed
                    manifest.state = FlowJobState::GenerationAmbiguous;
                    manifest.child_segments[i].state = FlowJobState::GenerationAmbiguous;
                    manifest.child_segments[i].submission_state =
                        FlowChildSubmissionState::Ambiguous;
                    manifest.error = Some(JobErrorRecord {
                        code: "SUBMISSION_UNCONFIRMED".to_string(),
                        sanitized_message: e,
                    });
                    self.store.save_manifest_atomic(&mut manifest)?;
                    return Ok(());
                }
            };

            // Proven submitted
            manifest.child_segments[i].submission_state = FlowChildSubmissionState::ProvenSubmitted;
            manifest.child_segments[i].submission_evidence = Some(submission_evidence.clone());
            manifest.state = FlowJobState::Generating;
            manifest.child_segments[i].state = FlowJobState::Generating;
            self.store.save_manifest_atomic(&mut manifest)?;

            // Poll until complete
            let poll_result = self.bridge.poll_generation(&submission_evidence).await?;
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
                            .unwrap_or_else(|| "Generation failed".to_string()),
                    });
                    self.store.save_manifest_atomic(&mut manifest)?;
                    return Ok(());
                }
                _ => {}
            }

            // Download segment output
            manifest.state = FlowJobState::Downloading;
            manifest.child_segments[i].state = FlowJobState::Downloading;
            self.store.save_manifest_atomic(&mut manifest)?;

            let out_seg_path = outputs_dir.join(format!("out_segment_{:03}.mp4", i));
            let download_url = poll_result
                .download_url
                .unwrap_or_else(|| format!("{}/download", self.bridge.target_url()));
            self.bridge
                .download_artifact(&download_url, &out_seg_path)
                .await?;

            // Validate segment output
            manifest.state = FlowJobState::ValidatingSegment;
            manifest.child_segments[i].state = FlowJobState::ValidatingSegment;
            self.store.save_manifest_atomic(&mut manifest)?;

            let validated_child =
                FlowOutputValidator::validate_child_artifact(&out_seg_path, seg_duration)?;
            manifest.child_segments[i].download_artifact_path = Some(out_seg_path);
            manifest.child_segments[i].download_artifact_sha = Some(validated_child.sha256);
            manifest.child_segments[i].submission_state = FlowChildSubmissionState::ProvenCompleted;
            manifest.child_segments[i].state = FlowJobState::Completed;
            manifest.child_segments[i].timestamps.completed_at = Some(Utc::now().to_rfc3339());
            manifest.credit_record.completed_generations += 1;
            self.store.save_manifest_atomic(&mut manifest)?;
        }

        // 3. Final Stitching & Audio Muxing Phase
        manifest.state = FlowJobState::Stitching;
        self.store.save_manifest_atomic(&mut manifest)?;

        let child_output_paths: Vec<PathBuf> = manifest
            .child_segments
            .iter()
            .filter_map(|c| c.download_artifact_path.clone())
            .collect();

        let final_output_file = flow_dir.join("final_flow_output.mp4");
        let final_record = FlowStitcher::stitch_flow_segments(
            &child_output_paths,
            Some(source_video_path),
            manifest.source_facts.duration_sec,
            &manifest.final_audio_policy,
            &final_output_file,
        )?;

        manifest.final_output = Some(final_record);
        manifest.state = FlowJobState::Completed;
        manifest.timestamps.completed_at = Some(Utc::now().to_rfc3339());
        self.store.save_manifest_atomic(&mut manifest)?;

        Ok(())
    }
}
