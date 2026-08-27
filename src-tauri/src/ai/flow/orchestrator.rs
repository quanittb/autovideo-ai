use super::continuity::FlowContinuityManager;
use super::manifest::{
    FlowCanonicalGeometry, FlowChildSubmissionState, FlowFinalAudioPolicy, FlowGenerationManifest,
    FlowIdentityContinuityStrategy, FlowJobKind, FlowJobSnapshot, FlowJobState,
    FlowNormalizedSegment, FlowParentLedger,
};
use super::output_validator::FlowOutputValidator;
use super::playwright_bridge::PlaywrightBridge;
use super::profile::FlowProfileManager;
use super::prompt_optimizer::{calculate_prompt_hash, PromptSource};
use super::segment::FlowVideoSegmenter;
use super::stitcher::{FlowStitcher, FlowVideoNormalizer};
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

pub use super::capability::{
    FlowCapabilityContext, FlowCapabilityObservation, FlowCapabilityObservationStore,
    FlowCapabilityPolicy, FlowCapabilitySource, FlowCreditRecord, FlowModelCapabilitiesSnapshot,
    FlowModelCapability,
};
pub use super::manifest::{FlowObservedGenerationConfig, FlowRequestedGenerationConfig};
pub use super::playwright_bridge::{FlowSubmissionOutcome, PreparedFlowSubmission};

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
    #[serde(default)]
    pub requested_config: Option<FlowRequestedGenerationConfig>,
    #[serde(default)]
    pub configuration_fingerprint: Option<String>,
    #[serde(default)]
    pub preflight_id: Option<String>,
}

impl FlowGenerationRequest {
    pub fn canonical_requested_config(&self) -> FlowRequestedGenerationConfig {
        self.requested_config.clone().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowPreflightTicket {
    pub preflight_id: String,
    pub configuration_fingerprint: String,
    pub profile_id: String,
    pub project_id: String,
    pub source_media_id: String,
    pub prompt_hash: String,
    pub requested_config: FlowRequestedGenerationConfig,
    pub live_displayed_credit_cost: Option<u32>,
    pub cost_provenance: FlowCostProvenance,
    pub checked_at: String,
    pub expires_at: String,
    pub ready_for_paid_submission: bool,
}

use std::collections::HashMap;
use std::sync::RwLock as StdRwLock;

#[derive(Debug, Clone, Default)]
pub struct FlowPreflightTicketStore {
    tickets: Arc<StdRwLock<HashMap<String, FlowPreflightTicket>>>,
}

impl FlowPreflightTicketStore {
    pub fn new() -> Self {
        Self {
            tickets: Arc::new(StdRwLock::new(HashMap::new())),
        }
    }

    pub fn insert_ticket(&self, ticket: FlowPreflightTicket) {
        if let Ok(mut guard) = self.tickets.write() {
            guard.insert(ticket.preflight_id.clone(), ticket);
        }
    }

    pub fn get_ticket(&self, preflight_id: &str) -> Option<FlowPreflightTicket> {
        if let Ok(guard) = self.tickets.read() {
            guard.get(preflight_id).cloned()
        } else {
            None
        }
    }

    pub fn consume_ticket(&self, preflight_id: &str) -> Option<FlowPreflightTicket> {
        if let Ok(mut guard) = self.tickets.write() {
            guard.remove(preflight_id)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowCostProvenance {
    UploadedVideoEdit,
    GenericComposerDiagnostic,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowCreditStatus {
    Ready,
    LoginRequired,
    FlowUiChanged,
    ProfileBusy,
    Unknown,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowCreditSource {
    LiveFlowUi,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowProfileCreditStatus {
    pub profile_id: String,
    #[serde(default)]
    pub balance: Option<u32>,
    pub status: FlowCreditStatus,
    pub checked_at: String,
    pub source: FlowCreditSource,
}

pub fn compute_configuration_fingerprint(
    profile_id: &str,
    source_media_id: &str,
    prompt_hash: &str,
    transformation_intent: TransformationIntent,
    identity_mode: IdentityMode,
    config: &FlowRequestedGenerationConfig,
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(profile_id.as_bytes());
    hasher.update(b":");
    hasher.update(source_media_id.as_bytes());
    hasher.update(b":");
    hasher.update(prompt_hash.as_bytes());
    hasher.update(b":");
    hasher.update(format!("{:?}", transformation_intent).as_bytes());
    hasher.update(b":");
    hasher.update(format!("{:?}", identity_mode).as_bytes());
    hasher.update(b":");
    hasher.update(config.model_id.as_deref().unwrap_or("").as_bytes());
    hasher.update(b":");
    hasher.update(config.resolution.as_deref().unwrap_or("").as_bytes());
    hasher.update(b":");
    hasher.update(config.duration_sec.unwrap_or(0).to_string().as_bytes());
    hasher.update(b":");
    hasher.update(config.orientation.as_deref().unwrap_or("").as_bytes());
    hasher.update(b":");
    hasher.update(config.output_count.to_string().as_bytes());
    format!("{:x}", hasher.finalize())
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

    pub requested_config: FlowRequestedGenerationConfig,
    pub observed_config: FlowObservedGenerationConfig,
    pub configuration_fingerprint: String,

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
    #[serde(default)]
    pub observed_resolution: Option<String>,

    pub ready_for_paid_submission: bool,

    #[serde(default)]
    pub blocking_code: Option<String>,

    pub checked_at: String,
    #[serde(default)]
    pub preflight_id: String,
    #[serde(default)]
    pub expires_at: String,
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

    pub async fn refresh_flow_credit_balance(
        &self,
        profile_id: &str,
    ) -> Result<FlowProfileCreditStatus, String> {
        self.orchestrator
            .refresh_flow_credit_balance(profile_id)
            .await
    }

    pub fn get_flow_model_capabilities(
        &self,
        profile_id: &str,
        operation_context: FlowCapabilityContext,
    ) -> FlowModelCapabilitiesSnapshot {
        self.orchestrator
            .get_flow_model_capabilities(profile_id, operation_context)
    }

    pub fn list_flow_jobs(&self, project_id: &str) -> Result<Vec<FlowJobSnapshot>, String> {
        let manifests = self.orchestrator.store().list_all_flow_jobs(project_id)?;
        Ok(manifests.into_iter().map(|m| m.to_snapshot()).collect())
    }

    pub fn use_flow_output_in_project(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<crate::projects::UseFlowOutputResult, String> {
        let manifest = self
            .orchestrator
            .store()
            .load_manifest(project_id, parent_id)?;
        let final_record = manifest.final_output.ok_or_else(|| {
            "ARTIFACT_NOT_READY: Flow generation output artifact has not been created".to_string()
        })?;

        let flow_job_dir = self
            .orchestrator
            .store()
            .parent_flow_job_dir(project_id, parent_id)?;
        let canonical_job_dir = flow_job_dir
            .canonicalize()
            .map_err(|e| format!("CANONICALIZE_FAILED: {}", e))?;
        let canonical_artifact = final_record
            .final_path
            .canonicalize()
            .map_err(|e| format!("OUTPUT_NOT_FOUND: {}", e))?;

        if !canonical_artifact.starts_with(&canonical_job_dir) {
            return Err(
                "SECURITY_VIOLATION: Output artifact is outside flow job directory".to_string(),
            );
        }

        let paths = self.orchestrator.storage_paths();
        let manager = crate::projects::ProjectManager::new(paths.clone());
        let mut project = manager
            .get_project(project_id)
            .map_err(|e| format!("{}", e))?;

        // Idempotency check: if project already has a derived asset with this provider job ID, return it
        if let Some(existing) = project
            .derived_media_assets
            .iter()
            .find(|d| d.provenance.provider == "FLOW" && d.provenance.provider_job_id == parent_id)
        {
            return Ok(crate::projects::UseFlowOutputResult {
                derived_asset: existing.clone(),
                project,
            });
        }

        let project_dir = paths.projects_dir.join(project_id);
        let derived_dir = project_dir.join("media").join("derived");
        std::fs::create_dir_all(&derived_dir)
            .map_err(|e| format!("Failed to create project derived media directory: {}", e))?;

        let new_asset_id = format!("media_flow_{}", uuid::Uuid::new_v4().simple());
        let derived_filename = format!("flow_{}_{}.mp4", parent_id, new_asset_id);
        let destination_path = derived_dir.join(&derived_filename);

        std::fs::copy(&canonical_artifact, &destination_path)
            .map_err(|e| format!("Failed to copy derived asset into project media: {}", e))?;

        // Probe the destination file to ensure independent metadata
        let media_service = crate::media::MediaService::new();
        let probed_metadata = media_service
            .probe(&destination_path)
            .map_err(|e| format!("Failed to probe copied derived media: {}", e))?;

        let source_media_id_used = manifest.source_media_id.clone().unwrap_or_else(|| {
            project
                .source_media
                .as_ref()
                .map(|s| s.media_id.clone())
                .unwrap_or_default()
        });

        let derived_source_media = crate::projects::SourceMedia {
            media_id: new_asset_id.clone(),
            original_file_name: derived_filename,
            source_path: destination_path,
            duration_ms: probed_metadata.duration_ms,
            width: probed_metadata.width,
            height: probed_metadata.height,
            fps: probed_metadata.fps,
            file_size_bytes: probed_metadata.file_size_bytes,
            container: probed_metadata.container,
            video_codec: probed_metadata.video_codec,
            audio_codec: probed_metadata.audio_codec,
            has_audio: probed_metadata.has_audio,
        };

        let provenance = crate::projects::DerivedMediaProvenance {
            provider: "FLOW".to_string(),
            provider_job_id: parent_id.to_string(),
            source_media_id: source_media_id_used,
            transformation_intent: manifest.transformation_intent,
            identity_mode: manifest.identity_mode,
            prompt_hash: manifest.prompt_hash.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let derived_asset = crate::projects::DerivedMediaAsset {
            media: derived_source_media,
            provenance,
        };

        project.derived_media_assets.push(derived_asset.clone());
        if let Some(ref mut ed) = project.editor_state {
            ed.active_media_id = Some(new_asset_id);
        } else {
            project.editor_state = Some(crate::projects::ProjectEditorState {
                active_media_id: Some(new_asset_id),
                ..Default::default()
            });
        }

        let updated_project = manager
            .update_project(&project)
            .map_err(|e| format!("{}", e))?;

        Ok(crate::projects::UseFlowOutputResult {
            derived_asset,
            project: updated_project,
        })
    }
}

// -----------------------------------------------------------------------------
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
    capability_observations: Arc<FlowCapabilityObservationStore>,
    preflight_tickets: Arc<FlowPreflightTicketStore>,
}

impl FlowOrchestrator {
    pub fn new(storage_paths: StoragePaths) -> Self {
        let store = FlowJobStore::new(storage_paths.clone());
        let profile_manager = FlowProfileManager::new(storage_paths.app_data_dir.clone());
        let bridge = PlaywrightBridge::new();
        let capability_policy = FlowCapabilityPolicy::for_edit_uploaded_video();
        let capability_observations = Arc::new(FlowCapabilityObservationStore::new());
        let preflight_tickets = Arc::new(FlowPreflightTicketStore::new());

        Self {
            storage_paths,
            store,
            profile_manager,
            bridge,
            capability_policy,
            capability_observations,
            preflight_tickets,
        }
    }

    pub fn with_mock_bridge(storage_paths: StoragePaths, mock_url: String) -> Self {
        let store = FlowJobStore::new(storage_paths.clone());
        let profile_manager = FlowProfileManager::new(storage_paths.app_data_dir.clone());
        let bridge = PlaywrightBridge::with_mock_url(mock_url);
        let capability_policy = FlowCapabilityPolicy::for_edit_uploaded_video();
        let capability_observations = Arc::new(FlowCapabilityObservationStore::new());
        let preflight_tickets = Arc::new(FlowPreflightTicketStore::new());

        Self {
            storage_paths,
            store,
            profile_manager,
            bridge,
            capability_policy,
            capability_observations,
            preflight_tickets,
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

    pub fn capability_observations(&self) -> &FlowCapabilityObservationStore {
        &self.capability_observations
    }

    pub fn preflight_tickets(&self) -> &FlowPreflightTicketStore {
        &self.preflight_tickets
    }

    pub async fn refresh_flow_credit_balance(
        &self,
        profile_id: &str,
    ) -> Result<FlowProfileCreditStatus, String> {
        let profile_dir = self.profile_manager.get_profile_dir(profile_id)?;
        if !profile_dir.exists() {
            return Ok(FlowProfileCreditStatus {
                profile_id: profile_id.to_string(),
                balance: None,
                status: FlowCreditStatus::Error,
                checked_at: Utc::now().to_rfc3339(),
                source: FlowCreditSource::Unknown,
            });
        }

        let lock_guard = match self.profile_manager.acquire_session_lock(profile_id) {
            Ok(g) => g,
            Err(_) => {
                return Ok(FlowProfileCreditStatus {
                    profile_id: profile_id.to_string(),
                    balance: None,
                    status: FlowCreditStatus::ProfileBusy,
                    checked_at: Utc::now().to_rfc3339(),
                    source: FlowCreditSource::Unknown,
                });
            }
        };

        let bridge = self.bridge.clone();
        let balance_val = match bridge.read_credit_balance(&profile_dir).await {
            Ok(v) => v,
            Err(e) => {
                drop(lock_guard);
                return Ok(FlowProfileCreditStatus {
                    profile_id: profile_id.to_string(),
                    balance: None,
                    status: if e.contains("LOGIN_REQUIRED") {
                        FlowCreditStatus::LoginRequired
                    } else if e.contains("FLOW_UI_CHANGED") {
                        FlowCreditStatus::FlowUiChanged
                    } else {
                        FlowCreditStatus::Error
                    },
                    checked_at: Utc::now().to_rfc3339(),
                    source: FlowCreditSource::Unknown,
                });
            }
        };

        drop(lock_guard);

        let status_str = balance_val
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("UNKNOWN");
        let status = match status_str {
            "READY" => FlowCreditStatus::Ready,
            "LOGIN_REQUIRED" => FlowCreditStatus::LoginRequired,
            "FLOW_UI_CHANGED" => FlowCreditStatus::FlowUiChanged,
            _ => FlowCreditStatus::Unknown,
        };

        let balance = balance_val
            .get("balance")
            .and_then(|b| b.as_u64())
            .map(|b| b as u32);
        let source_str = balance_val
            .get("source")
            .and_then(|s| s.as_str())
            .unwrap_or("UNKNOWN");
        let source = match source_str {
            "LIVE_FLOW_UI" => FlowCreditSource::LiveFlowUi,
            _ => FlowCreditSource::Unknown,
        };

        Ok(FlowProfileCreditStatus {
            profile_id: profile_id.to_string(),
            balance,
            status,
            checked_at: Utc::now().to_rfc3339(),
            source,
        })
    }

    pub fn get_flow_model_capabilities(
        &self,
        profile_id: &str,
        operation_context: FlowCapabilityContext,
    ) -> FlowModelCapabilitiesSnapshot {
        self.capability_observations
            .get_snapshot(profile_id, operation_context)
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
            requested_config: None,
            configuration_fingerprint: None,
            preflight_id: None,
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

        let requested_config = request.canonical_requested_config();
        let prompt_hash = calculate_prompt_hash(&clean_prompt);
        let configuration_fingerprint = compute_configuration_fingerprint(
            &request.profile_id,
            &request.source_media_id,
            &prompt_hash,
            intent,
            identity_mode,
            &requested_config,
        );

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
            let preflight_id = format!("pf_{}", uuid::Uuid::new_v4());
            let expires_at = (Utc::now() + chrono::Duration::seconds(300)).to_rfc3339();
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
                requested_config: requested_config.clone(),
                observed_config: FlowObservedGenerationConfig::default(),
                configuration_fingerprint,
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
                observed_resolution: None,
                ready_for_paid_submission: false,
                blocking_code: Some(auth_status.to_string()),
                checked_at: Utc::now().to_rfc3339(),
                preflight_id,
                expires_at,
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
        let observed_resolution = edit_verif
            .and_then(|v| v.get("resolution"))
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

        let observed_config = FlowObservedGenerationConfig {
            model_id: observed_model.clone(),
            resolution: observed_resolution.clone(),
            duration_sec: observed_generation_length.map(|d| d.round() as u32),
            orientation: observed_orientation.clone(),
            output_count: observed_output_count,
        };

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

        let model_matches = match (
            requested_config.model_id.as_deref(),
            observed_model.as_deref(),
        ) {
            (Some(req), Some(obs)) => {
                super::capability::normalize_canonical_model(req)
                    == super::capability::normalize_canonical_model(obs)
            }
            (None, Some(obs)) => super::capability::normalize_canonical_model(obs) == "omni flash",
            _ => false,
        };

        let resolution_matches = match (
            requested_config.resolution.as_deref(),
            observed_resolution.as_deref(),
        ) {
            (Some(req), Some(obs)) => {
                super::capability::normalize_canonical_resolution(req)
                    == super::capability::normalize_canonical_resolution(obs)
            }
            (Some(req), None) => super::capability::normalize_canonical_resolution(req) == "720p",
            (None, _) => true,
        };

        let expected_duration_sec = requested_config.duration_sec.unwrap_or(10);
        let duration_matches = match observed_generation_length {
            Some(obs) => (expected_duration_sec as f64 - obs).abs() < 0.5,
            None => false,
        };

        let expected_orientation = requested_config.orientation.as_deref().unwrap_or("9:16");
        let orientation_matches = match observed_orientation.as_deref() {
            Some(obs) => {
                let exp_norm =
                    super::capability::normalize_canonical_orientation(expected_orientation);
                let obs_norm = super::capability::normalize_canonical_orientation(obs);
                exp_norm != "UNKNOWN" && exp_norm == obs_norm
            }
            None => false,
        };

        let output_count_matches = output_count == requested_config.output_count;

        let configuration_verified = video_attached
            && video_edit_active
            && model_matches
            && resolution_matches
            && duration_matches
            && orientation_matches
            && output_count_matches;

        let (
            cost_provenance,
            live_displayed_credit_cost,
            mut ready_for_paid_submission,
            mut blocking_code,
        ) = if video_attached && video_edit_active && configuration_verified {
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

        // Check insufficient credit balance when both balance and cost are known
        if let (Some(bal), Some(cost)) = (live_balance, live_displayed_credit_cost) {
            if bal < cost {
                blocking_code = Some("FLOW_INSUFFICIENT_CREDITS".to_string());
                ready_for_paid_submission = false;
            }
        }

        let preflight_id = format!("pf_{}", uuid::Uuid::new_v4());
        let expires_at = (Utc::now() + chrono::Duration::seconds(300)).to_rfc3339();

        if ready_for_paid_submission && video_attached && video_edit_active {
            let obs_durations = observed_generation_length
                .map(|d| vec![d.round() as u32])
                .unwrap_or_default();
            let obs_orientations = observed_orientation
                .as_deref()
                .map(|o| {
                    let norm = super::capability::normalize_canonical_orientation(o);
                    if norm != "UNKNOWN" {
                        vec![norm.to_string()]
                    } else {
                        vec![o.to_string()]
                    }
                })
                .unwrap_or_default();

            self.capability_observations
                .record_observation(FlowCapabilityObservation {
                    profile_id: request.profile_id.clone(),
                    operation_context: FlowCapabilityContext::UploadedVideoEdit,
                    model_id: model.clone().unwrap_or_else(|| "Omni Flash".to_string()),
                    display_name: model.clone().unwrap_or_else(|| "Omni Flash".to_string()),
                    supported_resolutions: observed_resolution
                        .clone()
                        .map(|r| vec![r])
                        .unwrap_or_else(|| vec!["720p".to_string()]),
                    supported_durations_sec: obs_durations,
                    supported_orientations: obs_orientations,
                    supported_output_counts: vec![output_count],
                    supports_uploaded_video_edit: true,
                    observed_at: Utc::now().to_rfc3339(),
                    adapter_version: "flow-playwright-1.0".to_string(),
                });
        }

        let ticket = FlowPreflightTicket {
            preflight_id: preflight_id.clone(),
            configuration_fingerprint: configuration_fingerprint.clone(),
            profile_id: request.profile_id.clone(),
            project_id: request.project_id.clone(),
            source_media_id: request.source_media_id.clone(),
            prompt_hash: prompt_hash.clone(),
            requested_config: requested_config.clone(),
            live_displayed_credit_cost,
            cost_provenance,
            checked_at: Utc::now().to_rfc3339(),
            expires_at: expires_at.clone(),
            ready_for_paid_submission,
        };
        self.preflight_tickets.insert_ticket(ticket);

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
            requested_config,
            observed_config,
            configuration_fingerprint,
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
            observed_resolution,
            ready_for_paid_submission,
            blocking_code,
            checked_at: Utc::now().to_rfc3339(),
            preflight_id,
            expires_at,
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

        let is_long_video = facts.duration_sec > 10.000;

        let max_credits = request.max_credits.ok_or_else(|| {
            if is_long_video {
                "FLOW_TOTAL_CREDIT_BUDGET_REQUIRED: Explicit maxTotalCredits budget is required for multi-segment long video".to_string()
            } else {
                "FLOW_CREDIT_BUDGET_REQUIRED: Explicit maxCredits budget limit is mandatory for paid execution".to_string()
            }
        })?;

        let prompt_hash = calculate_prompt_hash(&clean_prompt);
        let requested_config = request.canonical_requested_config();

        if !is_long_video {
            let preflight_id = request.preflight_id.as_deref().ok_or_else(|| {
                "FLOW_PREFLIGHT_REQUIRED: Paid generation requires a valid preflightId from a successful preflight".to_string()
            })?;

            let req_fp = request
                .configuration_fingerprint
                .as_deref()
                .ok_or_else(|| {
                    "FLOW_PREFLIGHT_REQUIRED: Paid generation requires configurationFingerprint"
                        .to_string()
                })?;

            // Validate configuration fingerprint against expected
            let expected_fingerprint = compute_configuration_fingerprint(
                &request.profile_id,
                &request.source_media_id,
                &prompt_hash,
                intent,
                identity_mode,
                &requested_config,
            );

            if req_fp != &expected_fingerprint {
                return Err(
                    "FLOW_PREFLIGHT_STALE: Preflight configuration signature is invalid or stale"
                        .to_string(),
                );
            }

            // Validate preflight ticket from store
            let ticket = self
                .preflight_tickets
                .get_ticket(preflight_id)
                .ok_or_else(|| "FLOW_PREFLIGHT_REQUIRED: Preflight ticket not found".to_string())?;

            let expires_at_dt = chrono::DateTime::parse_from_rfc3339(&ticket.expires_at)
                .map_err(|_| {
                    "FLOW_PREFLIGHT_STALE: Invalid preflight expiration timestamp".to_string()
                })?
                .with_timezone(&Utc);
            if Utc::now() > expires_at_dt {
                return Err("FLOW_PREFLIGHT_STALE: Preflight ticket has expired".to_string());
            }

            if ticket.project_id != request.project_id
                || ticket.profile_id != request.profile_id
                || ticket.source_media_id != request.source_media_id
                || ticket.configuration_fingerprint != req_fp
            {
                return Err(
                    "FLOW_PREFLIGHT_STALE: Preflight configuration signature is invalid or stale"
                        .to_string(),
                );
            }

            if !ticket.ready_for_paid_submission {
                return Err(
                    "FLOW_PREFLIGHT_NOT_READY: Preflight was not authorized for paid submission"
                        .to_string(),
                );
            }

            let live_cost = ticket.live_displayed_credit_cost.ok_or_else(|| {
                "FLOW_PREFLIGHT_REQUIRED: Live displayed cost was not verified in preflight"
                    .to_string()
            })?;

            if live_cost > max_credits {
                return Err(format!(
                    "FLOW_CREDIT_BUDGET_EXCEEDED: Live displayed cost ({}) exceeds max budget ({})",
                    live_cost, max_credits
                ));
            }
        }

        // Plan segments using largest legal boundary
        let plan = FlowVideoSegmenter::plan_segments(&facts, &self.capability_policy)?;

        // Atomically consume preflight ticket immediately before job creation (for single-segment)
        if !is_long_video {
            if let Some(p_id) = request.preflight_id.as_deref() {
                let _consumed_ticket = self
                    .preflight_tickets
                    .consume_ticket(p_id)
                    .ok_or_else(|| {
                        "FLOW_PREFLIGHT_ALREADY_CONSUMED: Preflight ticket has already been used or consumed"
                            .to_string()
                    })?;
            }
        }

        let parent_id = format!("flow_{}", uuid::Uuid::new_v4());
        let client_request_id = format!("req_{}", Utc::now().timestamp_millis());
        let submitted_prompt = clean_prompt.clone();

        // Derive deterministic config hash
        let mut hasher = Sha256::new();
        hasher.update(parent_id.as_bytes());
        hasher.update(submitted_prompt.as_bytes());
        hasher.update(canonical_source_path.to_string_lossy().as_bytes());
        let config_hash = format!("{:x}", hasher.finalize());

        let mut credit_record = super::capability::FlowCreditRecord::default();
        let estimated = self.capability_policy.estimate_credits(plan.segments.len());
        credit_record.estimated_credits = estimated;
        credit_record.credit_budget_limit = Some(max_credits);

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
            source_media_id.clone(),
            prompt_hash.clone(),
            source_file_name,
            intent,
            identity_mode,
            request.target_face.clone(),
            requested_config.clone(),
            submitted_prompt,
            prompt_hash.clone(),
            resolved_prompt_source,
            self.capability_policy.capability_policy_version,
            self.capability_policy.split_policy_version,
            facts.clone(),
            plan.clone(),
            credit_record,
            audio_policy,
        );

        manifest.state = FlowJobState::Ready;

        let is_long_video = facts.duration_sec > 10.000;
        let flow_dir = self
            .store
            .parent_flow_job_dir(&request.project_id, &parent_id)?;

        if is_long_video {
            let long_video_plan = FlowVideoSegmenter::plan_long_video(
                &parent_id,
                &request.project_id,
                source_media_id.as_deref(),
                &canonical_source_path,
                &flow_dir,
                intent,
                identity_mode,
                requested_config.clone(),
                &clean_prompt,
                &prompt_hash,
                10.0,
            )?;

            let parent_ledger = FlowParentLedger {
                segment_count: long_video_plan.segment_count,
                planning_cost_estimate: (long_video_plan.segment_count * 20) as u32,
                authoritative_committed_credits: 0,
                reserved_credits: 0,
                completed_paid_segments: 0,
                dispatched_paid_clicks: 0,
                max_total_credits: Some(max_credits),
            };

            let canonical_geometry = FlowCanonicalGeometry {
                width: facts.width,
                height: facts.height,
                orientation: requested_config.orientation.clone().unwrap_or_else(|| {
                    if facts.height >= facts.width {
                        "PORTRAIT".to_string()
                    } else {
                        "LANDSCAPE".to_string()
                    }
                }),
                sar: "1:1".to_string(),
            };

            manifest.job_kind = FlowJobKind::LongVideoParent;
            manifest.parent_ledger = Some(parent_ledger);
            manifest.long_video_plan = Some(long_video_plan);
            manifest.canonical_geometry = Some(canonical_geometry);
            manifest.continuity_strategy = Some(FlowIdentityContinuityStrategy::SamePromptBaseline);
        } else {
            manifest.job_kind = FlowJobKind::SingleSegment;
        }

        self.store.save_manifest_atomic(&mut manifest)?;

        let snapshot = manifest.to_snapshot();

        // Spawn sequential worker
        let orchestrator_clone = self.clone();
        let project_id_clone = request.project_id;
        let parent_id_clone = parent_id;
        let source_video_clone = canonical_source_path;

        tokio::spawn(async move {
            if let Err(e) = orchestrator_clone
                .run_flow_worker(
                    &project_id_clone,
                    &parent_id_clone,
                    &source_video_clone,
                    cancellations,
                )
                .await
            {
                eprintln!("[FLOW WORKER ERROR] job {}: {}", parent_id_clone, e);
                if let Ok(mut m) = orchestrator_clone
                    .store
                    .load_manifest(&project_id_clone, &parent_id_clone)
                {
                    if m.state != FlowJobState::Failed && m.state != FlowJobState::Cancelled {
                        m.state = FlowJobState::Failed;
                        m.error = Some(JobErrorRecord {
                            code: "WORKER_EXECUTION_FAILED".to_string(),
                            sanitized_message: e,
                        });
                        let _ = orchestrator_clone.store.save_manifest_atomic(&mut m);
                    }
                }
            }
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

        if manifest.job_kind == FlowJobKind::LongVideoParent {
            return self
                .run_long_video_parent_worker(
                    project_id,
                    parent_id,
                    source_video_path,
                    cancellations,
                )
                .await;
        }

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
                    let session_ref = active_session.as_mut().ok_or_else(|| {
                        "INTERNAL_ERROR: Missing active browser session".to_string()
                    })?;

                    manifest.active_segment_index = i;
                    manifest.state = FlowJobState::Submitting;
                    manifest.child_segments[i].state = FlowJobState::Submitting;

                    let attempt_id = format!("att_{}_{}", i, Utc::now().timestamp_millis());

                    // 1. Prepare video edit submission
                    let prep = match session_ref
                        .prepare_video_edit(
                            &frozen_prompt,
                            Some(&seg_input_path),
                            Some(seg_duration),
                            Some(&manifest.requested_generation_config),
                            &attempt_id,
                        )
                        .await
                    {
                        Ok(p) => p,
                        Err(e) => {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            let is_ui_changed = e.contains("FLOW_UI_CHANGED");
                            let job_state = if is_ui_changed {
                                FlowJobState::FlowUiChanged
                            } else {
                                FlowJobState::Failed
                            };
                            manifest.state = job_state;
                            manifest.child_segments[i].state = job_state;
                            manifest.error = Some(JobErrorRecord {
                                code: if is_ui_changed {
                                    "FLOW_UI_CHANGED".to_string()
                                } else {
                                    "PREPARATION_FAILED".to_string()
                                },
                                sanitized_message: e,
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                    };

                    // Validate live cost from prepare - ZERO numeric fallback allowed
                    let unit_live_cost = match prep.live_displayed_credit_cost {
                        Some(cost) => cost,
                        None => {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::Blocked;
                            manifest.child_segments[i].state = FlowJobState::Blocked;
                            manifest.child_segments[i].submission_state =
                                FlowChildSubmissionState::NeverAttempted;
                            manifest.error = Some(JobErrorRecord {
                                code: "FLOW_LIVE_COST_UNVERIFIED".to_string(),
                                sanitized_message: "PRE_CLICK_REJECTED: Live displayed credit cost could not be verified on the Flow workspace".to_string(),
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                    };

                    if let Some(budget_limit) = manifest.credit_record.credit_budget_limit {
                        if manifest.credit_record.reserved_credits + unit_live_cost > budget_limit {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::Blocked;
                            manifest.child_segments[i].state = FlowJobState::Blocked;
                            manifest.child_segments[i].submission_state =
                                FlowChildSubmissionState::NeverAttempted;
                            manifest.error = Some(JobErrorRecord {
                                code: "FLOW_CREDIT_BUDGET_EXCEEDED".to_string(),
                                sanitized_message: format!(
                                    "PRE_CLICK_REJECTED: Submitting segment #{} requires {} credits, which exceeds budget limit of {} credits (currently reserved: {})",
                                    i, unit_live_cost, budget_limit, manifest.credit_record.reserved_credits
                                ),
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                    }

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

                    // Before click: Persist local submission attempt state & reserve LIVE cost
                    manifest.child_segments[i].local_submission_attempt_id =
                        Some(attempt_id.clone());
                    manifest.child_segments[i].submission_state =
                        FlowChildSubmissionState::AttemptPersisted;
                    manifest.credit_record.reserved_credits += unit_live_cost;
                    self.store.save_manifest_atomic(&mut manifest)?;

                    let max_budget = manifest.credit_record.credit_budget_limit.unwrap_or(99999);

                    // Execute ONE click with pre-click validation
                    let submit_outcome = session_ref
                        .submit_prepared(
                            &attempt_id,
                            unit_live_cost,
                            max_budget,
                            &prep.prepared_fingerprint,
                            Some(&manifest.requested_generation_config),
                            Some(&manifest.prompt_hash),
                            prep.source_identity
                                .as_deref()
                                .or(manifest.source_file_name.as_deref()),
                        )
                        .await;

                    match submit_outcome {
                        Ok(FlowSubmissionOutcome::ProvenSubmitted {
                            generation_evidence,
                            ..
                        }) => {
                            manifest.child_segments[i].submission_state =
                                FlowChildSubmissionState::ProvenSubmitted;
                            manifest.child_segments[i].submission_evidence =
                                Some(generation_evidence.clone());
                            manifest.state = FlowJobState::Generating;
                            manifest.child_segments[i].state = FlowJobState::Generating;
                            self.store.save_manifest_atomic(&mut manifest)?;
                            generation_evidence
                        }
                        Ok(FlowSubmissionOutcome::PreClickRejected { reason, .. }) => {
                            // Rollback reserved credits since click was NOT dispatched
                            manifest.credit_record.reserved_credits = manifest
                                .credit_record
                                .reserved_credits
                                .saturating_sub(unit_live_cost);
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            let reason_str =
                                reason.unwrap_or_else(|| "Pre-click validation failed".to_string());
                            let is_ui_changed = reason_str.contains("FLOW_UI_CHANGED");
                            let job_state = if is_ui_changed {
                                FlowJobState::FlowUiChanged
                            } else {
                                FlowJobState::Failed
                            };
                            manifest.state = job_state;
                            manifest.child_segments[i].state = job_state;
                            manifest.child_segments[i].submission_state =
                                FlowChildSubmissionState::NeverAttempted;
                            manifest.error = Some(JobErrorRecord {
                                code: if is_ui_changed {
                                    "FLOW_UI_CHANGED".to_string()
                                } else if reason_str.contains("FLOW_CREDIT_BUDGET_EXCEEDED") {
                                    "FLOW_CREDIT_BUDGET_EXCEEDED".to_string()
                                } else {
                                    "PRE_CLICK_REJECTED".to_string()
                                },
                                sanitized_message: reason_str,
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                        Ok(FlowSubmissionOutcome::PostClickAmbiguous { reason, .. }) => {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::GenerationAmbiguous;
                            manifest.child_segments[i].state = FlowJobState::GenerationAmbiguous;
                            manifest.child_segments[i].submission_state =
                                FlowChildSubmissionState::Ambiguous;
                            manifest.error = Some(JobErrorRecord {
                                code: "GENERATION_AMBIGUOUS".to_string(),
                                sanitized_message: reason.unwrap_or_else(|| {
                                    "Post-click transition ambiguous".to_string()
                                }),
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                        Err(e) => {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            let is_pre_click = e.contains("CLICK_NOT_DISPATCHED")
                                || e.contains("PRE_CLICK")
                                || e.contains("FLOW_CONFIGURATION")
                                || e.contains("FLOW_LIVE_COST")
                                || e.contains("FLOW_CREDIT_BUDGET")
                                || e.contains("CLICK_FAILED");

                            if is_pre_click {
                                // Rollback reserved credits since click was NOT dispatched
                                manifest.credit_record.reserved_credits = manifest
                                    .credit_record
                                    .reserved_credits
                                    .saturating_sub(unit_live_cost);
                                manifest.state = FlowJobState::Failed;
                                manifest.child_segments[i].state = FlowJobState::Failed;
                                manifest.child_segments[i].submission_state =
                                    FlowChildSubmissionState::NeverAttempted;
                                manifest.error = Some(JobErrorRecord {
                                    code: "PRE_CLICK_REJECTED".to_string(),
                                    sanitized_message: e,
                                });
                            } else {
                                manifest.state = FlowJobState::GenerationAmbiguous;
                                manifest.child_segments[i].state =
                                    FlowJobState::GenerationAmbiguous;
                                manifest.child_segments[i].submission_state =
                                    FlowChildSubmissionState::Ambiguous;
                                manifest.error = Some(JobErrorRecord {
                                    code: "GENERATION_AMBIGUOUS".to_string(),
                                    sanitized_message: e,
                                });
                            }
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

    pub async fn run_long_video_parent_worker(
        &self,
        project_id: &str,
        parent_id: &str,
        source_video_path: &Path,
        cancellations: Option<Arc<FlowCancellationRegistry>>,
    ) -> Result<(), String> {
        let mut manifest = self.store.load_manifest(project_id, parent_id)?;

        let flow_dir = self.store.parent_flow_job_dir(project_id, parent_id)?;
        let source_segments_dir = flow_dir.join("input_segments");
        let raw_children_dir = flow_dir.join("raw_children");
        let normalized_dir = flow_dir.join("normalized");
        let evidence_dir = flow_dir.join("continuity_evidence");
        let _ = std::fs::create_dir_all(&source_segments_dir);
        let _ = std::fs::create_dir_all(&raw_children_dir);
        let _ = std::fs::create_dir_all(&normalized_dir);
        let _ = std::fs::create_dir_all(&evidence_dir);

        let mut long_plan = match manifest.long_video_plan.clone() {
            Some(p) => p,
            None => {
                manifest.state = FlowJobState::Failed;
                manifest.error = Some(JobErrorRecord {
                    code: "LONG_VIDEO_PLAN_MISSING".to_string(),
                    sanitized_message: "Parent manifest missing long video plan".to_string(),
                });
                self.store.save_manifest_atomic(&mut manifest)?;
                return Ok(());
            }
        };

        // 1. Splitting Phase (Extract segments if not extracted)
        let needs_extract = long_plan.segments.iter().any(|s| {
            s.source_segment_path.as_os_str().is_empty() || !s.source_segment_path.exists()
        });

        if needs_extract {
            if self
                .check_cancelled(project_id, parent_id, cancellations.as_ref())
                .await
            {
                manifest.state = FlowJobState::Cancelled;
                self.store.save_manifest_atomic(&mut manifest)?;
                return Ok(());
            }

            manifest.state = FlowJobState::Splitting;
            self.store.save_manifest_atomic(&mut manifest)?;

            if let Err(e) = FlowVideoSegmenter::extract_long_video_segments(
                &mut long_plan,
                source_video_path,
                &source_segments_dir,
            ) {
                manifest.state = FlowJobState::Failed;
                manifest.error = Some(JobErrorRecord {
                    code: "SEGMENT_EXTRACTION_FAILED".to_string(),
                    sanitized_message: e,
                });
                self.store.save_manifest_atomic(&mut manifest)?;
                return Ok(());
            }

            manifest.long_video_plan = Some(long_plan.clone());
            manifest.state = FlowJobState::ReadyToSubmit;
            self.store.save_manifest_atomic(&mut manifest)?;
        }

        // 2. Child Lifecycle: Sequential normalization & execution
        let canonical_geom =
            manifest
                .canonical_geometry
                .clone()
                .unwrap_or_else(|| FlowCanonicalGeometry {
                    width: manifest.source_facts.width,
                    height: manifest.source_facts.height,
                    orientation: "PORTRAIT".to_string(),
                    sar: "1:1".to_string(),
                });
        let rational_fps = long_plan.get_rational_fps();
        let total_segs = long_plan.segments.len();

        let profile_dir = self.profile_manager.get_profile_dir(&manifest.profile_id)?;
        let _lock_guard = match self
            .profile_manager
            .acquire_session_lock(&manifest.profile_id)
        {
            Ok(g) => Some(g),
            Err(e) => {
                manifest.state = FlowJobState::Blocked;
                manifest.error = Some(JobErrorRecord {
                    code: "FLOW_PROFILE_LOCKED".to_string(),
                    sanitized_message: format!(
                        "Profile {} is currently locked or in use: {}",
                        manifest.profile_id, e
                    ),
                });
                self.store.save_manifest_atomic(&mut manifest)?;
                return Ok(());
            }
        };

        let is_mock =
            manifest.profile_id.starts_with("profile_mock") || manifest.profile_id == "mock";

        let mut active_session = if !is_mock {
            match self.bridge.open_active_session(&profile_dir).await {
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
            }
        } else {
            None
        };

        for i in 0..total_segs {
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

            let norm_path = normalized_dir.join(format!("segment_{:03}.mp4", i));

            // Rehydration check: if segment is already completed & normalized, reuse it
            if norm_path.exists() && long_plan.segments[i].state == FlowJobState::Completed {
                continue;
            }

            let raw_child = raw_children_dir.join(format!("raw_child_{:03}.mp4", i));

            let unit_cost = if !raw_child.exists() {
                if let Some(ref mut session_ref) = active_session {
                    let seg_duration = (long_plan.segments[i].planned_frame_count as f64
                        * rational_fps.denominator as f64)
                        / (rational_fps.numerator as f64);
                    let attempt_id =
                        format!("att_{}_{}_{}", parent_id, i, Utc::now().timestamp_millis());

                    manifest.state = FlowJobState::Submitting;
                    long_plan.segments[i].state = FlowJobState::Submitting;
                    long_plan.segments[i].local_submission_attempt_id = Some(attempt_id.clone());
                    manifest.long_video_plan = Some(long_plan.clone());
                    self.store.save_manifest_atomic(&mut manifest)?;

                    // Fresh preflight / prepare video edit for child segment
                    let prep = match session_ref
                        .prepare_video_edit(
                            &manifest.submitted_prompt,
                            Some(&long_plan.segments[i].source_segment_path),
                            Some(seg_duration),
                            Some(&manifest.requested_generation_config),
                            &attempt_id,
                        )
                        .await
                    {
                        Ok(p) => p,
                        Err(e) => {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::Failed;
                            manifest.error = Some(JobErrorRecord {
                                code: "PREPARATION_FAILED".to_string(),
                                sanitized_message: e,
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                    };

                    let seg_cost = match prep.live_displayed_credit_cost {
                        Some(c) => c,
                        None => {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::Blocked;
                            manifest.error = Some(JobErrorRecord {
                                code: "FLOW_LIVE_COST_UNVERIFIED".to_string(),
                                sanitized_message: "PRE_CLICK_REJECTED: Live displayed credit cost could not be verified on the Flow workspace".to_string(),
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                    };

                    // Cost gate: per-segment cost <= 20
                    if seg_cost > 20 {
                        if let Some(s) = active_session.take() {
                            s.close().await;
                        }
                        manifest.state = FlowJobState::Blocked;
                        manifest.error = Some(JobErrorRecord {
                            code: "FLOW_SEGMENT_CREDIT_BUDGET_EXCEEDED".to_string(),
                            sanitized_message: format!(
                                "PRE_CLICK_REJECTED: Submitting segment #{} requires {} credits, which exceeds per-segment limit of 20 credits",
                                i, seg_cost
                            ),
                        });
                        self.store.save_manifest_atomic(&mut manifest)?;
                        return Ok(());
                    }

                    // Budget check before click against parent ledger
                    if let Some(ref ledger) = manifest.parent_ledger {
                        if let Some(max_tot) = ledger.max_total_credits {
                            if ledger.authoritative_committed_credits + seg_cost > max_tot {
                                if let Some(s) = active_session.take() {
                                    s.close().await;
                                }
                                manifest.state = FlowJobState::Failed;
                                manifest.error = Some(JobErrorRecord {
                                    code: "FLOW_TOTAL_CREDIT_BUDGET_EXCEEDED".to_string(),
                                    sanitized_message: format!(
                                        "Committed credits ({}) + next segment cost ({}) exceeds maxTotalCredits ({})",
                                        ledger.authoritative_committed_credits, seg_cost, max_tot
                                    ),
                                });
                                self.store.save_manifest_atomic(&mut manifest)?;
                                return Ok(());
                            }
                        }
                    }

                    // Section 7 & 8: Revalidate active media and record uploaded_source_evidence
                    long_plan.segments[i].uploaded_source_evidence =
                        prep.uploaded_source_evidence.clone();
                    long_plan.segments[i].preclick_cost = Some(seg_cost);

                    let expected_stem = format!("segment_{:03}", i);
                    if let Some(ref obs_source) = prep.source_identity {
                        let obs_lower = obs_source.to_lowercase();
                        if !obs_lower.contains(&expected_stem) {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::Failed;
                            manifest.error = Some(JobErrorRecord {
                                code: "FLOW_ACTIVE_MEDIA_MISMATCH".to_string(),
                                sanitized_message: format!(
                                    "Observed active media ({}) does not match expected segment ({})",
                                    obs_source, expected_stem
                                ),
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                    }

                    // Pre-click checkpoint
                    long_plan.segments[i].submission_state =
                        FlowChildSubmissionState::AttemptPersisted;
                    manifest.long_video_plan = Some(long_plan.clone());
                    self.store.save_manifest_atomic(&mut manifest)?;

                    // Click Generate EXACTLY ONCE
                    let submit_outcome = session_ref
                        .submit_prepared(
                            &attempt_id,
                            seg_cost,
                            seg_cost, // Child cap = preflight cost
                            &prep.prepared_fingerprint,
                            Some(&manifest.requested_generation_config),
                            Some(&manifest.prompt_hash),
                            Some(&expected_stem),
                        )
                        .await;

                    let generation_evidence = match submit_outcome {
                        Ok(FlowSubmissionOutcome::ProvenSubmitted {
                            generation_evidence,
                            ..
                        }) => {
                            manifest.state = FlowJobState::Generating;
                            long_plan.segments[i].state = FlowJobState::Generating;
                            long_plan.segments[i].submission_state =
                                FlowChildSubmissionState::ProvenSubmitted;
                            long_plan.segments[i].submission_evidence =
                                Some(generation_evidence.clone());
                            long_plan.segments[i].click_dispatched = true;
                            if let Some(ref mut ledg) = manifest.parent_ledger {
                                ledg.dispatched_paid_clicks += 1;
                                ledg.authoritative_committed_credits += seg_cost;
                            }
                            manifest.long_video_plan = Some(long_plan.clone());
                            self.store.save_manifest_atomic(&mut manifest)?;
                            generation_evidence
                        }
                        Ok(FlowSubmissionOutcome::PreClickRejected { reason, .. }) => {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::Failed;
                            manifest.error = Some(JobErrorRecord {
                                code: "PRE_CLICK_REJECTED".to_string(),
                                sanitized_message: reason
                                    .unwrap_or_else(|| "Pre-click validation rejected".to_string()),
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                        Ok(FlowSubmissionOutcome::PostClickAmbiguous { reason, .. }) => {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::GenerationAmbiguous;
                            long_plan.segments[i].submission_state =
                                FlowChildSubmissionState::Ambiguous;
                            manifest.error = Some(JobErrorRecord {
                                code: "GENERATION_AMBIGUOUS".to_string(),
                                sanitized_message: reason.unwrap_or_else(|| {
                                    "Post-click transition ambiguous".to_string()
                                }),
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                        Err(e) => {
                            if let Some(s) = active_session.take() {
                                s.close().await;
                            }
                            manifest.state = FlowJobState::Failed;
                            manifest.error = Some(JobErrorRecord {
                                code: "SUBMISSION_FAILED".to_string(),
                                sanitized_message: e,
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }
                    };

                    // Poll until completion (up to 10 minutes)
                    let poll_start = Utc::now();
                    let poll_timeout = std::time::Duration::from_secs(600);
                    let mut is_completed = false;

                    while !is_completed {
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
                            manifest.error = Some(JobErrorRecord {
                                code: "GENERATION_TIMEOUT".to_string(),
                                sanitized_message:
                                    "Flow generation exceeded maximum polling duration of 10 minutes"
                                        .to_string(),
                            });
                            self.store.save_manifest_atomic(&mut manifest)?;
                            return Ok(());
                        }

                        let poll_res = session_ref.poll(&generation_evidence).await?;
                        match poll_res.status.as_str() {
                            "ready" => {
                                session_ref
                                    .download(poll_res.download_url.as_deref(), &raw_child)
                                    .await?;
                                is_completed = true;
                            }
                            "failed" => {
                                if let Some(s) = active_session.take() {
                                    s.close().await;
                                }
                                manifest.state = FlowJobState::Failed;
                                manifest.error = Some(JobErrorRecord {
                                    code: "GENERATION_FAILED".to_string(),
                                    sanitized_message: poll_res
                                        .error_message
                                        .unwrap_or_else(|| "Generation failed".to_string()),
                                });
                                self.store.save_manifest_atomic(&mut manifest)?;
                                return Ok(());
                            }
                            _ => {
                                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                            }
                        }
                    }

                    seg_cost
                } else {
                    // Mock mode
                    let unit_cost = 20u32;
                    if let Some(ref ledger) = manifest.parent_ledger {
                        if let Some(max_tot) = ledger.max_total_credits {
                            if ledger.authoritative_committed_credits + unit_cost > max_tot {
                                manifest.state = FlowJobState::Failed;
                                manifest.error = Some(JobErrorRecord {
                                    code: "FLOW_TOTAL_CREDIT_BUDGET_EXCEEDED".to_string(),
                                    sanitized_message: format!(
                                        "Committed credits ({}) + next segment cost ({}) exceeds maxTotalCredits ({})",
                                        ledger.authoritative_committed_credits, unit_cost, max_tot
                                    ),
                                });
                                self.store.save_manifest_atomic(&mut manifest)?;
                                return Ok(());
                            }
                        }
                    }

                    let _ = std::fs::copy(&long_plan.segments[i].source_segment_path, &raw_child);
                    unit_cost
                }
            } else {
                20u32
            };

            manifest.state = FlowJobState::ValidatingSegment;
            self.store.save_manifest_atomic(&mut manifest)?;

            let norm_res = FlowVideoNormalizer::normalize_child_segment(
                &raw_child,
                &long_plan.segments[i],
                &canonical_geom,
                rational_fps,
                &norm_path,
            );

            match norm_res {
                Ok(_) => {
                    long_plan.segments[i].state = FlowJobState::Completed;
                    long_plan.segments[i].submission_state =
                        FlowChildSubmissionState::ProvenCompleted;
                    if let Some(ref mut ledger) = manifest.parent_ledger {
                        ledger.completed_paid_segments += 1;
                        if active_session.is_none() {
                            ledger.dispatched_paid_clicks += 1;
                            ledger.authoritative_committed_credits += unit_cost;
                        }
                    }

                    // Continuity evidence for boundary with previous segment
                    if i > 0 {
                        let prev_norm = normalized_dir.join(format!("segment_{:03}.mp4", i - 1));
                        if prev_norm.exists() {
                            if let Ok(ev) = FlowContinuityManager::extract_boundary_evidence(
                                i - 1,
                                &prev_norm,
                                i - 1,
                                &norm_path,
                                i,
                                &evidence_dir,
                            ) {
                                manifest.continuity_evidence.push(ev);
                            }
                        }
                    }

                    manifest.long_video_plan = Some(long_plan.clone());
                    self.store.save_manifest_atomic(&mut manifest)?;
                }
                Err(e) => {
                    if let Some(s) = active_session.take() {
                        s.close().await;
                    }
                    manifest.state = FlowJobState::Failed;
                    manifest.error = Some(JobErrorRecord {
                        code: "CHILD_NORMALIZATION_FAILED".to_string(),
                        sanitized_message: e,
                    });
                    self.store.save_manifest_atomic(&mut manifest)?;
                    return Ok(());
                }
            }
        }

        if let Some(s) = active_session.take() {
            s.close().await;
        }

        // 3. Final Stitching Phase (Section 12)
        if self
            .check_cancelled(project_id, parent_id, cancellations.as_ref())
            .await
        {
            manifest.state = FlowJobState::Cancelled;
            self.store.save_manifest_atomic(&mut manifest)?;
            return Ok(());
        }

        manifest.state = FlowJobState::Stitching;
        self.store.save_manifest_atomic(&mut manifest)?;

        let mut norm_segs = Vec::new();
        for (idx, seg) in long_plan.segments.iter().enumerate() {
            let norm_p = normalized_dir.join(format!("segment_{:03}.mp4", idx));
            if !norm_p.exists() {
                manifest.state = FlowJobState::Failed;
                manifest.error = Some(JobErrorRecord {
                    code: "STITCH_INCOMPLETE".to_string(),
                    sanitized_message: format!("Normalized segment {} is missing", idx),
                });
                self.store.save_manifest_atomic(&mut manifest)?;
                return Ok(());
            }

            let facts = SourceMediaProbe::probe_file(&norm_p).map_err(|e| e.to_string())?;
            let frames = facts
                .timing
                .as_ref()
                .and_then(|t| t.nb_frames)
                .unwrap_or_else(|| (facts.duration_sec * rational_fps.to_f64()).round() as u64);

            norm_segs.push(FlowNormalizedSegment {
                segment_index: idx,
                path: norm_p,
                frame_count: frames,
                sha256: seg.source_segment_sha256.clone(),
            });
        }

        let total_frames: u64 = long_plan
            .segments
            .iter()
            .map(|s| s.planned_frame_count)
            .sum();
        let final_video_out = flow_dir.join("final_flow_output.mp4");

        let source_audio = if manifest.source_facts.has_audio
            && manifest.final_audio_policy.preserve_original_audio
        {
            Some(source_video_path)
        } else {
            None
        };

        let stitch_res = FlowStitcher::stitch_long_video_timeline(
            &norm_segs,
            source_audio,
            total_frames,
            rational_fps,
            &final_video_out,
        );

        match stitch_res {
            Ok((stitched_rec, audio_mode)) => {
                manifest.state = FlowJobState::ValidatingFinal;
                self.store.save_manifest_atomic(&mut manifest)?;

                manifest.final_output = Some(stitched_rec);
                manifest.audio_restoration_mode = Some(audio_mode);
                manifest.state = FlowJobState::Completed;
                self.store.save_manifest_atomic(&mut manifest)?;
            }
            Err(e) => {
                manifest.state = FlowJobState::Failed;
                manifest.error = Some(JobErrorRecord {
                    code: "FINAL_STITCH_FAILED".to_string(),
                    sanitized_message: e,
                });
                self.store.save_manifest_atomic(&mut manifest)?;
            }
        }

        if let Some(reg) = cancellations {
            reg.remove_cancellation(parent_id).await;
        }

        Ok(())
    }
}
