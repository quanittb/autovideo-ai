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
use crate::ai::transformation::{IdentityMode, TargetFaceSelection, TransformationIntent};
use crate::system::StoragePaths;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

// -----------------------------------------------------------------------------
// 1. Flow Cancellation Registry
// -----------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct FlowCancellationRegistry {
    cancelled_jobs: Arc<RwLock<HashSet<String>>>,
}

impl FlowCancellationRegistry {
    pub fn new() -> Self {
        Self {
            cancelled_jobs: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub async fn request_cancellation(&self, job_id: &str) {
        let mut guard = self.cancelled_jobs.write().await;
        guard.insert(job_id.to_string());
    }

    pub async fn is_cancelled(&self, job_id: &str) -> bool {
        let guard = self.cancelled_jobs.read().await;
        guard.contains(job_id)
    }

    pub async fn remove_cancellation(&self, job_id: &str) {
        let mut guard = self.cancelled_jobs.write().await;
        guard.remove(job_id);
    }
}

// -----------------------------------------------------------------------------
// 2. Production Flow Generation Request
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowGenerationRequest {
    pub project_id: String,
    #[serde(alias = "sourceVideoPath")]
    pub source_media_id: String,
    pub profile_id: String,
    #[serde(default)]
    pub transformation_intent: Option<TransformationIntent>,
    #[serde(default)]
    pub identity_mode: Option<IdentityMode>,
    pub prompt: String,
    #[serde(default)]
    pub prompt_source: Option<PromptSource>,
    #[serde(default)]
    pub target_face: Option<TargetFaceSelection>,
    #[serde(default)]
    pub max_credits: Option<u32>,
    #[serde(default)]
    pub preserve_original_audio: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowCostProvenance {
    UploadedVideoEdit,
    GenericComposerDiagnostic,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowGenerationPreflight {
    pub project_id: String,
    pub source_media_id: String,
    pub profile_id: String,

    pub transformation_intent: TransformationIntent,
    pub identity_mode: IdentityMode,

    pub resolved_prompt: String,
    pub prompt_source: PromptSource,
    pub prompt_hash: String,

    pub video_attached: bool,
    pub video_edit_active: bool,
    pub configuration_verified: bool,

    #[serde(default)]
    pub configured_model: Option<String>,
    #[serde(default)]
    pub configured_duration: Option<f64>,
    #[serde(default)]
    pub configured_orientation: Option<String>,
    pub output_count: u32,

    #[serde(default)]
    pub live_displayed_credit_cost: Option<u32>,
    #[serde(default)]
    pub live_credit_balance: Option<u32>,

    pub cost_provenance: FlowCostProvenance,

    #[serde(default)]
    pub diagnostic_composer_credit_cost: Option<u32>,

    #[serde(default)]
    pub observed_source_title: Option<String>,
    #[serde(default)]
    pub observed_source_duration: Option<f64>,
    #[serde(default)]
    pub observed_model: Option<String>,
    #[serde(default)]
    pub observed_orientation: Option<String>,
    #[serde(default)]
    pub observed_output_count: Option<u32>,
    #[serde(default)]
    pub observed_generation_length: Option<f64>,

    pub ready_for_paid_submission: bool,

    #[serde(default)]
    pub blocking_code: Option<String>,

    pub checked_at: String,
}

// -----------------------------------------------------------------------------
// 3. Flow Runtime Service (Application-Level Manager)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FlowRuntimeService {
    pub orchestrator: Arc<FlowOrchestrator>,
    pub cancellations: Arc<FlowCancellationRegistry>,
}

impl FlowRuntimeService {
    pub fn new(storage_paths: StoragePaths) -> Self {
        Self {
            orchestrator: Arc::new(FlowOrchestrator::new(storage_paths)),
            cancellations: Arc::new(FlowCancellationRegistry::new()),
        }
    }

    pub fn with_mock_bridge(storage_paths: StoragePaths, mock_url: String) -> Self {
        Self {
            orchestrator: Arc::new(FlowOrchestrator::with_mock_bridge(storage_paths, mock_url)),
            cancellations: Arc::new(FlowCancellationRegistry::new()),
        }
    }

    pub async fn preflight_flow_generation(
        &self,
        request: FlowGenerationRequest,
        canonical_source: PathBuf,
    ) -> Result<FlowGenerationPreflight, String> {
        self.orchestrator
            .preflight_flow_generation(request, canonical_source)
            .await
    }

    pub async fn start_flow_generation(
        &self,
        request: FlowGenerationRequest,
        canonical_source: PathBuf,
    ) -> Result<FlowJobSnapshot, String> {
        self.orchestrator
            .start_flow_generation_with_request(
                request,
                canonical_source,
                Some(self.cancellations.clone()),
            )
            .await
    }

    pub async fn cancel_flow_generation(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<FlowJobSnapshot, String> {
        self.cancellations.request_cancellation(parent_id).await;
        self.orchestrator.store().cancel_job(project_id, parent_id)
    }

    pub fn get_flow_job_status(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<FlowJobSnapshot, String> {
        let manifest = self
            .orchestrator
            .store()
            .load_manifest(project_id, parent_id)?;
        Ok(manifest.to_snapshot())
    }

    pub fn list_flow_jobs(&self, project_id: &str) -> Result<Vec<FlowJobSnapshot>, String> {
        let manifests = self.orchestrator.store().list_all_flow_jobs(project_id)?;
        Ok(manifests.into_iter().map(|m| m.to_snapshot()).collect())
    }
}

// -----------------------------------------------------------------------------
// 4. Flow Orchestrator Core Implementation
// -----------------------------------------------------------------------------

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
        let request = FlowGenerationRequest {
            project_id,
            source_media_id: source_video_path.to_string_lossy().to_string(),
            profile_id,
            transformation_intent: Some(TransformationIntent::FaceReplace),
            identity_mode: Some(IdentityMode::Generated),
            prompt,
            prompt_source,
            target_face: None,
            max_credits: None,
            preserve_original_audio: Some(true),
        };

        self.start_flow_generation_with_request(request, source_video_path, None)
            .await
    }

    pub async fn preflight_flow_generation(
        &self,
        request: FlowGenerationRequest,
        canonical_source_path: PathBuf,
    ) -> Result<FlowGenerationPreflight, String> {
        let intent = request
            .transformation_intent
            .unwrap_or(TransformationIntent::FaceReplace);
        let identity_mode = request.identity_mode.unwrap_or(IdentityMode::Generated);

        // Capability check
        match intent {
            TransformationIntent::BackgroundRemove => {
                return Err(
                    "FLOW_CAPABILITY_UNSUPPORTED: Background removal is not supported by Google Flow".to_string(),
                );
            }
            TransformationIntent::FaceReplace => {
                if identity_mode == IdentityMode::Reference {
                    return Err(
                        "FLOW_REFERENCE_IDENTITY_NOT_SUPPORTED: Face replacement with custom reference image is not supported by Google Flow".to_string(),
                    );
                }
            }
            _ => {}
        }

        let mut clean_prompt = request.prompt.trim().to_string();
        let resolved_prompt_source = if clean_prompt.is_empty() {
            if intent == TransformationIntent::FaceReplace
                && identity_mode == IdentityMode::Generated
            {
                clean_prompt = "Replace only the selected target person's facial identity with a new, temporally consistent synthetic identity. Strictly preserve: body, clothing, hair where practical, pose, expression dynamics, mouth movement, head movement, action, camera motion, background, lighting, composition, timing, and all non-target people.".to_string();
                PromptSource::SystemDefault
            } else {
                return Err("REQUEST_INVALID: Prompt cannot be empty for non-default transformation intents".to_string());
            }
        } else {
            request.prompt_source.unwrap_or(PromptSource::User)
        };

        if !canonical_source_path.exists() {
            return Err(format!(
                "FILE_NOT_FOUND: Source video does not exist: {:?}",
                canonical_source_path
            ));
        }

        // Verify profile exists
        let profile_dir = self.profile_manager.get_profile_dir(&request.profile_id)?;
        if !profile_dir.exists() {
            return Err(format!(
                "PROFILE_NOT_FOUND: Profile {} does not exist",
                request.profile_id
            ));
        }

        // Probe source video
        let facts = SourceMediaProbe::probe_file(&canonical_source_path)
            .map_err(|e| format!("PROBE_FAILED: {}", e))?;

        if facts.duration_sec <= 0.0 || facts.fps <= 0.0 {
            return Err("INVALID_MEDIA: Media facts have invalid duration or fps".to_string());
        }

        let prompt_hash = calculate_prompt_hash(&clean_prompt);

        // Perform browser preflight with sidecar
        let mut session = self.bridge.open_active_session(&profile_dir).await?;
        let preflight_res = session
            .dry_run_preflight(&clean_prompt, Some(&canonical_source_path))
            .await;
        session.close().await;

        let val = preflight_res?;
        let auth_status = val
            .get("authStatus")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN");

        if auth_status != "READY" {
            return Ok(FlowGenerationPreflight {
                project_id: request.project_id,
                source_media_id: request.source_media_id,
                profile_id: request.profile_id,
                transformation_intent: intent,
                identity_mode,
                resolved_prompt: clean_prompt,
                prompt_source: resolved_prompt_source,
                prompt_hash,
                video_attached: false,
                video_edit_active: false,
                configuration_verified: false,
                configured_model: None,
                configured_duration: None,
                configured_orientation: None,
                output_count: 1,
                live_displayed_credit_cost: None,
                live_credit_balance: None,
                cost_provenance: FlowCostProvenance::Unknown,
                diagnostic_composer_credit_cost: None,
                observed_source_title: None,
                observed_source_duration: None,
                observed_model: None,
                observed_orientation: None,
                observed_output_count: None,
                observed_generation_length: None,
                ready_for_paid_submission: false,
                blocking_code: Some(auth_status.to_string()),
                checked_at: Utc::now().to_rfc3339(),
            });
        }

        let edit_verif = val.get("videoEditVerification");
        let video_attached = edit_verif
            .and_then(|v| v.get("uploadedVideoAttached"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false);

        let video_edit_active = edit_verif
            .and_then(|v| v.get("uploadedVideoEditActive"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false);

        let observed_source_title = edit_verif
            .and_then(|v| v.get("sourceTitle"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let observed_source_duration = edit_verif
            .and_then(|v| v.get("inputSelectedDuration"))
            .and_then(|v| v.as_f64());
        let observed_model = edit_verif
            .and_then(|v| v.get("model"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let observed_orientation = edit_verif
            .and_then(|v| v.get("orientation"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let observed_output_count = edit_verif
            .and_then(|v| v.get("outputCount"))
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let observed_generation_length = edit_verif
            .and_then(|v| v.get("generationLengthSec"))
            .and_then(|v| v.as_f64());

        let model = observed_model.clone();
        let duration = observed_generation_length;
        let orientation = observed_orientation.clone();
        let output_count = observed_output_count.unwrap_or(1);

        let live_cost_raw = edit_verif
            .and_then(|v| v.get("creditEstimateNumber"))
            .and_then(|v| v.as_u64())
            .map(|c| c as u32);

        let diagnostic_composer_cost = val
            .get("diagnosticComposerCreditCost")
            .and_then(|v| v.as_u64())
            .map(|c| c as u32);

        let live_balance = val
            .get("liveCreditBalance")
            .and_then(|v| v.as_u64())
            .map(|c| c as u32);

        let configuration_verified = video_attached
            && video_edit_active
            && observed_model.as_deref() == Some("Omni Flash")
            && output_count == 1;

        let (cost_provenance, live_displayed_credit_cost, ready_for_paid_submission, blocking_code) =
            if video_attached && video_edit_active && configuration_verified {
                if let Some(cost) = live_cost_raw {
                    (
                        FlowCostProvenance::UploadedVideoEdit,
                        Some(cost),
                        true,
                        None,
                    )
                } else {
                    (
                        FlowCostProvenance::Unknown,
                        None,
                        false,
                        Some("FLOW_CONFIGURATION_UNVERIFIED".to_string()),
                    )
                }
            } else {
                let code = if !video_attached {
                    "FLOW_VIDEO_NOT_ATTACHED"
                } else if !video_edit_active {
                    "FLOW_VIDEO_EDIT_NOT_ACTIVE"
                } else {
                    "FLOW_CONFIGURATION_UNVERIFIED"
                };
                (
                    FlowCostProvenance::Unknown,
                    None,
                    false,
                    Some(code.to_string()),
                )
            };

        Ok(FlowGenerationPreflight {
            project_id: request.project_id,
            source_media_id: request.source_media_id,
            profile_id: request.profile_id,
            transformation_intent: intent,
            identity_mode,
            resolved_prompt: clean_prompt,
            prompt_source: resolved_prompt_source,
            prompt_hash,
            video_attached,
            video_edit_active,
            configuration_verified,
            configured_model: model,
            configured_duration: duration,
            configured_orientation: orientation,
            output_count,
            live_displayed_credit_cost,
            live_credit_balance: live_balance,
            cost_provenance,
            diagnostic_composer_credit_cost: diagnostic_composer_cost,
            observed_source_title,
            observed_source_duration,
            observed_model,
            observed_orientation,
            observed_output_count,
            observed_generation_length,
            ready_for_paid_submission,
            blocking_code,
            checked_at: Utc::now().to_rfc3339(),
        })
    }

    pub async fn start_flow_generation_with_request(
        &self,
        request: FlowGenerationRequest,
        canonical_source_path: PathBuf,
        cancellations: Option<Arc<FlowCancellationRegistry>>,
    ) -> Result<FlowJobSnapshot, String> {
        let intent = request
            .transformation_intent
            .unwrap_or(TransformationIntent::FaceReplace);
        let identity_mode = request.identity_mode.unwrap_or(IdentityMode::Generated);

        // Capability check
        match intent {
            TransformationIntent::BackgroundRemove => {
                return Err(
                    "FLOW_CAPABILITY_UNSUPPORTED: Background removal is not supported by Google Flow".to_string(),
                );
            }
            TransformationIntent::FaceReplace => {
                if identity_mode == IdentityMode::Reference {
                    return Err(
                        "FLOW_REFERENCE_IDENTITY_NOT_SUPPORTED: Face replacement with custom reference image is not supported by Google Flow".to_string(),
                    );
                }
            }
            _ => {}
        }

        let mut clean_prompt = request.prompt.trim().to_string();
        let resolved_prompt_source = if clean_prompt.is_empty() {
            if intent == TransformationIntent::FaceReplace
                && identity_mode == IdentityMode::Generated
            {
                clean_prompt = "Replace only the selected target person's facial identity with a new, temporally consistent synthetic identity. Strictly preserve: body, clothing, hair where practical, pose, expression dynamics, mouth movement, head movement, action, camera motion, background, lighting, composition, timing, and all non-target people.".to_string();
                PromptSource::SystemDefault
            } else {
                return Err("REQUEST_INVALID: Prompt cannot be empty for non-default transformation intents".to_string());
            }
        } else {
            request.prompt_source.unwrap_or(PromptSource::User)
        };

        if !canonical_source_path.exists() {
            return Err(format!(
                "FILE_NOT_FOUND: Source video does not exist: {:?}",
                canonical_source_path
            ));
        }

        // Verify profile exists
        let profile_dir = self.profile_manager.get_profile_dir(&request.profile_id)?;
        if !profile_dir.exists() {
            return Err(format!(
                "PROFILE_NOT_FOUND: Profile {} does not exist",
                request.profile_id
            ));
        }

        // Probe source video
        let facts = SourceMediaProbe::probe_file(&canonical_source_path)
            .map_err(|e| format!("PROBE_FAILED: {}", e))?;

        if facts.duration_sec <= 0.0 || facts.fps <= 0.0 {
            return Err("INVALID_MEDIA: Media facts have invalid duration or fps".to_string());
        }

        // Plan segments using largest legal boundary
        let plan = FlowVideoSegmenter::plan_segments(&facts, &self.capability_policy)?;

        let parent_id = format!("flow_{}", uuid::Uuid::new_v4());
        let client_request_id = format!("req_{}", Utc::now().timestamp_millis());
        let submitted_prompt = clean_prompt.clone();
        let prompt_hash = calculate_prompt_hash(&submitted_prompt);

        // Derive deterministic config hash
        let mut hasher = Sha256::new();
        hasher.update(parent_id.as_bytes());
        hasher.update(submitted_prompt.as_bytes());
        hasher.update(canonical_source_path.to_string_lossy().as_bytes());
        let config_hash = format!("{:x}", hasher.finalize());

        let mut credit_record = super::capability::FlowCreditRecord::default();
        let estimated = self.capability_policy.estimate_credits(plan.segments.len());
        credit_record.estimated_credits = estimated;
        credit_record.credit_budget_limit = request.max_credits;

        // Pre-check estimated budget if set
        if let Some(max_credits) = request.max_credits {
            if estimated > max_credits {
                return Err(format!(
                    "PRE_CLICK_REJECTED: Estimated credits ({}) exceed max credit budget limit ({})",
                    estimated, max_credits
                ));
            }
        }

        let audio_policy = FlowFinalAudioPolicy {
            preserve_original_audio: request.preserve_original_audio.unwrap_or(true),
            codec: "aac".to_string(),
        };

        let source_file_name = canonical_source_path
            .file_name()
            .and_then(|f| f.to_str())
            .map(|s| s.to_string());
        let source_media_id = if request.source_media_id.trim().is_empty() {
            None
        } else {
            Some(request.source_media_id.clone())
        };

        let mut manifest = FlowGenerationManifest::new(
            parent_id.clone(),
            client_request_id,
            request.project_id.clone(),
            request.profile_id,
            config_hash,
            source_media_id,
            prompt_hash.clone(),
            source_file_name,
            intent,
            identity_mode,
            request.target_face.clone(),
            submitted_prompt,
            prompt_hash,
            resolved_prompt_source,
            self.capability_policy.capability_policy_version,
            self.capability_policy.split_policy_version,
            facts.clone(),
            plan.clone(),
            credit_record,
            audio_policy,
        );

        manifest.state = FlowJobState::Ready;
        self.store.save_manifest_atomic(&mut manifest)?;

        let snapshot = manifest.to_snapshot();

        // Spawn sequential worker
        let orchestrator_clone = self.clone();
        let project_id_clone = request.project_id;
        let parent_id_clone = parent_id;
        let source_video_clone = canonical_source_path;

        tokio::spawn(async move {
            let _ = orchestrator_clone
                .run_flow_worker(
                    &project_id_clone,
                    &parent_id_clone,
                    &source_video_clone,
                    cancellations,
                )
                .await;
        });

        Ok(snapshot)
    }

    async fn check_cancelled(
        &self,
        project_id: &str,
        parent_id: &str,
        cancellations: Option<&Arc<FlowCancellationRegistry>>,
    ) -> bool {
        if let Some(reg) = cancellations {
            if reg.is_cancelled(parent_id).await {
                return true;
            }
        }
        if let Ok(m) = self.store.load_manifest(project_id, parent_id) {
            if m.cancellation_requested || m.state == FlowJobState::Cancelled {
                return true;
            }
        }
        false
    }

    pub async fn run_flow_worker(
        &self,
        project_id: &str,
        parent_id: &str,
        source_video_path: &Path,
        cancellations: Option<Arc<FlowCancellationRegistry>>,
    ) -> Result<(), String> {
        let mut manifest = self.store.load_manifest(project_id, parent_id)?;

        // CHECKPOINT 1: Before profile lock and split
        if self
            .check_cancelled(project_id, parent_id, cancellations.as_ref())
            .await
        {
            manifest.state = FlowJobState::Cancelled;
            self.store.save_manifest_atomic(&mut manifest)?;
            return Ok(());
        }

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

        // CHECKPOINT 2: Before Splitting Phase
        if self
            .check_cancelled(project_id, parent_id, cancellations.as_ref())
            .await
        {
            manifest.state = FlowJobState::Cancelled;
            self.store.save_manifest_atomic(&mut manifest)?;
            return Ok(());
        }

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

        // CHECKPOINT 3: After split, before browser session launch
        if self
            .check_cancelled(project_id, parent_id, cancellations.as_ref())
            .await
        {
            manifest.state = FlowJobState::Cancelled;
            self.store.save_manifest_atomic(&mut manifest)?;
            return Ok(());
        }

        // 2. Sequential Browser Generation Phase (Single Live Session)
        let frozen_prompt = manifest.submitted_prompt.clone();
        let total_segments = manifest.child_segments.len();

        let mut active_session = match self.bridge.open_active_session(&profile_dir).await {
            Ok(s) => Some(s),
            Err(e) => {
                manifest.state = FlowJobState::Failed;
                manifest.error = Some(JobErrorRecord {
                    code: "SESSION_SPAWN_FAILED".to_string(),
                    sanitized_message: e,
                });
                self.store.save_manifest_atomic(&mut manifest)?;
                return Ok(());
            }
        };

        // CHECKPOINT 4: After browser launch
        if self
            .check_cancelled(project_id, parent_id, cancellations.as_ref())
            .await
        {
            if let Some(s) = active_session.take() {
                s.close().await;
            }
            manifest.state = FlowJobState::Cancelled;
            self.store.save_manifest_atomic(&mut manifest)?;
            return Ok(());
        }

        for i in 0..total_segments {
            // CHECKPOINT 5: Before each segment iteration
            if self
                .check_cancelled(project_id, parent_id, cancellations.as_ref())
                .await
            {
                if let Some(s) = active_session.take() {
                    s.close().await;
                }
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
                    if let Some(s) = active_session.take() {
                        s.close().await;
                    }
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
                    let seg_input_path = segments_dir.join(&seg_filename);

                    // CHECKPOINT 6: Before credit preflight and budget enforcement
                    if self
                        .check_cancelled(project_id, parent_id, cancellations.as_ref())
                        .await
                    {
                        if let Some(s) = active_session.take() {
                            s.close().await;
                        }
                        manifest.state = FlowJobState::Cancelled;
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }

                    // Pre-Click Credit Budget Sequence:
                    let estimated_unit_cost = self.capability_policy.credits_per_generation;
                    if let Some(budget_limit) = manifest.credit_record.credit_budget_limit {
                        let projected =
                            manifest.credit_record.reserved_credits + estimated_unit_cost;
                        if projected > budget_limit {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::Blocked;
                            manifest.child_segments[i].state = FlowJobState::Blocked;
                            manifest.error = Some(JobErrorRecord {
                                code: "FLOW_CREDIT_BUDGET_EXCEEDED".to_string(),
                                sanitized_message: format!(
                                    "PRE_CLICK_REJECTED: Submitting segment #{} requires {} credits, which exceeds budget limit of {} credits (currently reserved: {})",
                                    i, estimated_unit_cost, budget_limit, manifest.credit_record.reserved_credits
                                ),
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                    }

                    manifest.active_segment_index = i;
                    manifest.state = FlowJobState::Submitting;
                    manifest.child_segments[i].state = FlowJobState::Submitting;

                    // CHECKPOINT 7: Immediately before attempt persistence & click
                    if self
                        .check_cancelled(project_id, parent_id, cancellations.as_ref())
                        .await
                    {
                        if let Some(s) = active_session.take() {
                            s.close().await;
                        }
                        manifest.state = FlowJobState::Cancelled;
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }

                    // Before click: Persist local submission attempt state FIRST!
                    let attempt_id = format!("att_{}_{}", i, Utc::now().timestamp_millis());
                    manifest.child_segments[i].local_submission_attempt_id =
                        Some(attempt_id.clone());
                    manifest.child_segments[i].submission_state =
                        FlowChildSubmissionState::AttemptPersisted;
                    manifest.credit_record.reserved_credits += estimated_unit_cost;
                    self.store.save_manifest_atomic(&mut manifest)?;

                    // Execute ONE browser submission via active session
                    let session_ref = active_session.as_mut().ok_or_else(|| {
                        "INTERNAL_ERROR: Missing active browser session".to_string()
                    })?;

                    match session_ref
                        .submit(
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
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
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

            // 3. Poll until complete (with timeout and sleep) on active session
            let poll_start = Utc::now();
            let poll_timeout = std::time::Duration::from_secs(600); // 10 minutes max for video generation
            let mut is_completed = false;

            while !is_completed {
                // CHECKPOINT 8: During polling loop
                if self
                    .check_cancelled(project_id, parent_id, cancellations.as_ref())
                    .await
                {
                    if let Some(s) = active_session.take() {
                        s.close().await;
                    }
                    manifest.state = FlowJobState::Cancelled;
                    self.store.save_manifest_atomic(&mut manifest)?;
                    return Ok(());
                }

                if Utc::now().signed_duration_since(poll_start).num_seconds()
                    > poll_timeout.as_secs() as i64
                {
                    if let Some(s) = active_session.take() {
                        s.close().await;
                    }
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

                let session_ref = active_session.as_mut().ok_or_else(|| {
                    "INTERNAL_ERROR: Missing active browser session during poll".to_string()
                })?;

                let poll_result = session_ref.poll(&submission_evidence).await?;

                match poll_result.status.as_str() {
                    "login_required" => {
                        if let Some(s) = active_session.take() {
                            s.close().await;
                        }
                        manifest.state = FlowJobState::LoginRequired;
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }
                    "credits_required" => {
                        if let Some(s) = active_session.take() {
                            s.close().await;
                        }
                        manifest.state = FlowJobState::CreditsRequired;
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }
                    "ui_changed" => {
                        if let Some(s) = active_session.take() {
                            s.close().await;
                        }
                        manifest.state = FlowJobState::FlowUiChanged;
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }
                    "failed" => {
                        if let Some(s) = active_session.take() {
                            s.close().await;
                        }
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
                        // CHECKPOINT 9: Before downloading artifact
                        if self
                            .check_cancelled(project_id, parent_id, cancellations.as_ref())
                            .await
                        {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::Cancelled;
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }

                        let seg_out_name = format!("child_out_{:03}.mp4", i);
                        let seg_out_path = outputs_dir.join(&seg_out_name);

                        manifest.state = FlowJobState::Downloading;
                        manifest.child_segments[i].state = FlowJobState::Downloading;
                        self.store.save_manifest_atomic(&mut manifest)?;

                        session_ref
                            .download(poll_result.download_url.as_deref(), &seg_out_path)
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

        // Close session upon generation phase completion
        if let Some(s) = active_session.take() {
            s.close().await;
        }

        // CHECKPOINT 10: Before final stitching
        if self
            .check_cancelled(project_id, parent_id, cancellations.as_ref())
            .await
        {
            manifest.state = FlowJobState::Cancelled;
            self.store.save_manifest_atomic(&mut manifest)?;
            return Ok(());
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

        // Cleanup cancellation flag if any
        if let Some(reg) = cancellations {
            reg.remove_cancellation(parent_id).await;
        }

        Ok(())
    }
}
