use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowPollResult {
    pub status: String, // "queued" | "generating" | "ready" | "failed" | "login_required" | "credits_required" | "ui_changed"
    pub progress_pct: f64,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRecoveryResult {
    pub status: String,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub saved_path: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub correlated_output_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowGenerationSettings {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub generation_length_sec: Option<u32>,
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default)]
    pub output_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoEditModeVerification {
    pub uploaded_video_attached: bool,
    pub video_visible_in_active_edit: bool,
    pub uploaded_video_edit_active: bool,
    pub active_composer_mode: String,
    #[serde(default)]
    pub source_title: Option<String>,
    pub input_trim_start: f64,
    pub input_trim_end: f64,
    pub input_selected_duration: f64,
    pub model: String,
    pub generation_length_sec: u32,
    pub orientation: String,
    pub output_count: u32,
    pub resolution: String,
    #[serde(default)]
    pub credit_readback1: Option<String>,
    #[serde(default)]
    pub credit_readback2: Option<String>,
    #[serde(default)]
    pub credit_estimate_number: Option<u32>,
    pub credit_stable: bool,
    pub cost_classification: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowSettingsReadback {
    pub model: Option<String>,
    pub generation_length_sec: Option<u32>,
    pub orientation: Option<String>,
    pub output_count: Option<u32>,
    #[serde(default)]
    pub credit_estimate_text: Option<String>,
    #[serde(default)]
    pub credit_estimate_number: Option<u32>,
    #[serde(default)]
    pub summary_button_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedFlowSubmission {
    pub generate_ready: bool,
    pub observed_config: super::manifest::FlowObservedGenerationConfig,
    #[serde(default)]
    pub live_displayed_credit_cost: Option<u32>,
    pub cost_provenance: super::orchestrator::FlowCostProvenance,
    pub prepared_fingerprint: String,
    #[serde(default)]
    pub source_identity: Option<String>,
    #[serde(default)]
    pub uploaded_source_evidence: Option<super::manifest::FlowUploadedSourceEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "outcome",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum FlowSubmissionOutcome {
    #[serde(rename_all = "camelCase")]
    PreClickRejected {
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        click_dispatched: bool,
        local_submission_attempt_id: String,
    },
    #[serde(rename_all = "camelCase")]
    ProvenSubmitted {
        #[serde(alias = "generationEvidence")]
        generation_evidence: String,
        click_dispatched: bool,
        local_submission_attempt_id: String,
        #[serde(default)]
        post_click_state: Option<String>,
        #[serde(default)]
        submitted_at: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    PostClickAmbiguous {
        #[serde(default)]
        reason: Option<String>,
        click_dispatched: bool,
        local_submission_attempt_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcRequest {
    id: String,
    method: String,
    params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcError {
    code: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcResponse {
    id: String,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

pub struct PlaywrightSidecarProcess {
    child: Child,
    stdin: ChildStdin,
    reader: tokio::io::Lines<BufReader<ChildStdout>>,
}

impl PlaywrightSidecarProcess {
    pub async fn spawn() -> Result<Self, String> {
        let script_path = resolve_sidecar_script_path();
        if !script_path.exists() {
            return Err(format!(
                "SIDECAR_NOT_FOUND: Playwright sidecar not found at {:?}",
                script_path
            ));
        }

        let mut cmd = Command::new("node");
        cmd.arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn Playwright Node sidecar: {}", e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to open stdin for Playwright sidecar".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to open stdout for Playwright sidecar".to_string())?;

        let reader = BufReader::new(stdout).lines();

        Ok(Self {
            child,
            stdin,
            reader,
        })
    }

    pub async fn call_rpc(
        &mut self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, String> {
        let req_id = Uuid::new_v4().to_string();
        let req = JsonRpcRequest {
            id: req_id.clone(),
            method: method.to_string(),
            params,
        };

        let mut payload = serde_json::to_string(&req)
            .map_err(|e| format!("Failed to serialize RPC request: {}", e))?;
        payload.push('\n');

        self.stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to Playwright sidecar stdin: {}", e))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush Playwright sidecar stdin: {}", e))?;

        let read_future = async {
            while let Ok(Some(line)) = self.reader.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(trimmed) {
                    if resp.id == req_id {
                        if let Some(err) = resp.error {
                            return Err(format!("{}: {}", err.code, err.message));
                        }
                        return Ok(resp.result.unwrap_or(serde_json::Value::Null));
                    }
                }
            }
            Err("SIDECAR_CRASHED: Node sidecar process terminated unexpectedly".to_string())
        };

        match tokio::time::timeout(timeout, read_future).await {
            Ok(res) => res,
            Err(_) => Err(format!("RPC_TIMEOUT: Method {} timed out", method)),
        }
    }

    pub async fn close(&mut self) {
        let _ = self
            .call_rpc(
                "close_browser",
                serde_json::json!({}),
                Duration::from_secs(5),
            )
            .await;
        let _ = self.child.kill().await;
    }
}

impl Drop for PlaywrightSidecarProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

pub fn resolve_sidecar_script_path() -> PathBuf {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest_dir)
            .join("sidecars")
            .join("flow-playwright")
            .join("dist")
            .join("index.js");
        if p.exists() {
            return p;
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let p1 = cwd
            .join("src-tauri")
            .join("sidecars")
            .join("flow-playwright")
            .join("dist")
            .join("index.js");
        if p1.exists() {
            return p1;
        }

        let p2 = cwd
            .join("sidecars")
            .join("flow-playwright")
            .join("dist")
            .join("index.js");
        if p2.exists() {
            return p2;
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let p = parent
                .join("sidecars")
                .join("flow-playwright")
                .join("dist")
                .join("index.js");
            if p.exists() {
                return p;
            }
        }
    }

    PathBuf::from("src-tauri/sidecars/flow-playwright/dist/index.js")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowAuthStatus {
    Ready,
    LoginRequired,
    Unknown,
    FlowUiChanged,
    FlowEligibilityRequired,
    FlowLanding,
}

impl FlowAuthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::LoginRequired => "LOGIN_REQUIRED",
            Self::Unknown => "UNKNOWN",
            Self::FlowUiChanged => "FLOW_UI_CHANGED",
            Self::FlowEligibilityRequired => "FLOW_ELIGIBILITY_REQUIRED",
            Self::FlowLanding => "FLOW_LANDING",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_uppercase().as_str() {
            "READY" => Self::Ready,
            "LOGIN_REQUIRED" | "LOGINREQUIRED" => Self::LoginRequired,
            "FLOW_UI_CHANGED" | "FLOWUICHANGED" | "UI_CHANGED" => Self::FlowUiChanged,
            "FLOW_ELIGIBILITY_REQUIRED"
            | "FLOWELIGIBILITYREQUIRED"
            | "ELIGIBILITY_REQUIRED"
            | "USER_ACTION_REQUIRED" => Self::FlowEligibilityRequired,
            "FLOW_LANDING" | "FLOWLANDING" => Self::FlowLanding,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for FlowAuthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct PlaywrightBridge {
    mock_url: Option<String>,
}

impl PlaywrightBridge {
    pub const OFFICIAL_FLOW_URL: &'static str = "https://labs.google/fx/vi/tools/flow";

    pub fn new() -> Self {
        Self { mock_url: None }
    }

    pub fn with_mock_url(mock_url: String) -> Self {
        Self {
            mock_url: Some(mock_url),
        }
    }

    pub fn target_url(&self) -> String {
        self.mock_url
            .clone()
            .unwrap_or_else(|| Self::OFFICIAL_FLOW_URL.to_string())
    }

    pub fn validate_url_security(&self, url: &str) -> Result<(), String> {
        let trimmed = url.trim();
        if trimmed == Self::OFFICIAL_FLOW_URL
            || trimmed == "https://labs.google/fx/tools/flow"
            || trimmed == "https://labs.google/fx/en/tools/flow"
            || trimmed == "https://labs.google/flow"
        {
            return Ok(());
        }

        // localhost is permitted ONLY when mock_url was explicitly injected in this instance
        if let Some(ref mock) = self.mock_url {
            if trimmed.starts_with(mock)
                || (mock.starts_with("http://127.0.0.1:")
                    && (trimmed.starts_with("http://127.0.0.1:")
                        || trimmed.starts_with("http://localhost:")))
            {
                return Ok(());
            }
        }

        Err(format!(
            "SECURITY_VIOLATION: Unauthorized Flow URL origin: {}",
            url
        ))
    }

    pub fn validate_path_confinement(
        candidate: &Path,
        expected_root: &Path,
    ) -> Result<PathBuf, String> {
        let canonical_root = std::fs::canonicalize(expected_root)
            .map_err(|e| format!("Invalid expected root directory {:?}: {}", expected_root, e))?;

        if candidate.exists() {
            let canonical_cand = std::fs::canonicalize(candidate)
                .map_err(|e| format!("Invalid candidate path {:?}: {}", candidate, e))?;
            if !canonical_cand.starts_with(&canonical_root) {
                return Err(format!(
                    "SECURITY_VIOLATION: Path {:?} is not inside expected root {:?}",
                    candidate, expected_root
                ));
            }
            Ok(canonical_cand)
        } else {
            // For a not-yet-existing output path: verify parent directory
            let parent = candidate
                .parent()
                .ok_or_else(|| "Candidate path has no parent directory".to_string())?;
            let canonical_parent = std::fs::canonicalize(parent)
                .map_err(|e| format!("Invalid parent directory {:?}: {}", parent, e))?;
            if !canonical_parent.starts_with(&canonical_root) {
                return Err(format!(
                    "SECURITY_VIOLATION: Target parent {:?} is not inside expected root {:?}",
                    parent, expected_root
                ));
            }
            let file_name = candidate
                .file_name()
                .ok_or_else(|| "Candidate path has no file name".to_string())?;
            Ok(canonical_parent.join(file_name))
        }
    }

    pub fn launch_browser_params(&self, profile_dir: &Path, headless: bool) -> serde_json::Value {
        let runtime_mode = if self.mock_url.is_some() {
            "MOCK_CHROMIUM"
        } else {
            "PRODUCTION_CHROME"
        };
        serde_json::json!({
            "profilePath": profile_dir.to_string_lossy(),
            "headless": headless,
            "runtimeMode": runtime_mode,
            "channel": if runtime_mode == "PRODUCTION_CHROME" { "chrome" } else { "chromium" }
        })
    }

    pub async fn check_auth_status(&self, profile_dir: &Path) -> Result<FlowAuthStatus, String> {
        let url = self.target_url();
        self.validate_url_security(&url)?;

        let mut sidecar = PlaywrightSidecarProcess::spawn().await?;

        let launch_res = sidecar
            .call_rpc(
                "launch_browser",
                self.launch_browser_params(profile_dir, true),
                Duration::from_secs(30),
            )
            .await;

        if let Err(e) = launch_res {
            sidecar.close().await;
            return Err(e);
        }

        let nav_res = sidecar
            .call_rpc(
                "navigate_to_flow",
                serde_json::json!({ "flowUrl": url }),
                Duration::from_secs(30),
            )
            .await;

        if let Err(e) = nav_res {
            sidecar.close().await;
            return Err(e);
        }

        let auth_val = sidecar
            .call_rpc(
                "check_auth_status",
                serde_json::json!({}),
                Duration::from_secs(30),
            )
            .await;

        sidecar.close().await;

        match auth_val {
            Ok(val) => {
                let status_str = val
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("UNKNOWN");
                Ok(FlowAuthStatus::from_str_loose(status_str))
            }
            Err(e) => {
                if e.contains("FLOW_UI_CHANGED") {
                    Ok(FlowAuthStatus::FlowUiChanged)
                } else if e.contains("FLOW_ELIGIBILITY_REQUIRED")
                    || e.contains("ELIGIBILITY_REQUIRED")
                    || e.contains("USER_ACTION_REQUIRED")
                {
                    Ok(FlowAuthStatus::FlowEligibilityRequired)
                } else if e.contains("FLOW_LANDING") {
                    Ok(FlowAuthStatus::FlowLanding)
                } else if e.contains("LOGIN_REQUIRED") {
                    Ok(FlowAuthStatus::LoginRequired)
                } else {
                    Err(e)
                }
            }
        }
    }

    pub async fn open_active_session(
        &self,
        profile_dir: &Path,
    ) -> Result<FlowActiveBrowserSession, String> {
        let url = self.target_url();
        self.validate_url_security(&url)?;

        let mut sidecar = PlaywrightSidecarProcess::spawn().await?;

        if let Err(e) = sidecar
            .call_rpc(
                "launch_browser",
                self.launch_browser_params(profile_dir, true),
                Duration::from_secs(30),
            )
            .await
        {
            sidecar.close().await;
            return Err(e);
        }

        if let Err(e) = sidecar
            .call_rpc(
                "navigate_to_flow",
                serde_json::json!({ "flowUrl": url }),
                Duration::from_secs(30),
            )
            .await
        {
            sidecar.close().await;
            return Err(e);
        }

        Ok(FlowActiveBrowserSession {
            sidecar,
            target_url: url,
        })
    }

    pub async fn dry_run_preflight(
        &self,
        profile_dir: &Path,
        prompt: &str,
        video_path: Option<&Path>,
    ) -> Result<serde_json::Value, String> {
        let mut session = self.open_active_session(profile_dir).await?;
        let res = session.dry_run_preflight(prompt, video_path).await;
        session.close().await;
        res
    }

    pub async fn read_credit_balance(
        &self,
        profile_dir: &Path,
    ) -> Result<serde_json::Value, String> {
        let mut session = self.open_active_session(profile_dir).await?;
        let res = session.read_credit_balance().await;
        session.close().await;
        res
    }

    pub async fn submit_generation(
        &self,
        profile_dir: &Path,
        prompt: &str,
        video_path: Option<&Path>,
        duration_sec: f64,
        local_submission_attempt_id: &str,
    ) -> Result<String, String> {
        let mut session = self.open_active_session(profile_dir).await?;
        let res = session
            .submit(
                prompt,
                video_path,
                duration_sec,
                local_submission_attempt_id,
            )
            .await;
        session.close().await;
        res
    }

    pub async fn poll_generation(
        &self,
        profile_dir: &Path,
        submission_evidence: &str,
    ) -> Result<FlowPollResult, String> {
        let mut session = self.open_active_session(profile_dir).await?;
        let res = session.poll(submission_evidence).await;
        session.close().await;
        res
    }

    pub async fn download_artifact(
        &self,
        profile_dir: &Path,
        download_url: Option<&str>,
        destination_path: &Path,
    ) -> Result<PathBuf, String> {
        let mut session = self.open_active_session(profile_dir).await?;
        let res = session.download(download_url, destination_path).await;
        session.close().await;
        res
    }
}

pub struct FlowActiveBrowserSession {
    sidecar: PlaywrightSidecarProcess,
    target_url: String,
}

impl FlowActiveBrowserSession {
    pub async fn ensure_uploaded_video_edit_active(
        &mut self,
        video_path: Option<&Path>,
        expected_duration_sec: f64,
        expected_orientation: &str,
    ) -> Result<VideoEditModeVerification, String> {
        let video_path_str = video_path.map(|p| p.to_string_lossy().to_string());
        let val = self
            .sidecar
            .call_rpc(
                "ensure_uploaded_video_edit_active",
                serde_json::json!({
                    "videoPath": video_path_str,
                    "expectedDurationSec": expected_duration_sec,
                    "expectedOrientation": expected_orientation
                }),
                Duration::from_secs(60),
            )
            .await?;

        serde_json::from_value(val)
            .map_err(|e| format!("Failed to parse edit mode verification: {}", e))
    }

    pub async fn dry_run_preflight(
        &mut self,
        prompt: &str,
        video_path: Option<&Path>,
    ) -> Result<serde_json::Value, String> {
        let video_path_str = video_path.map(|p| p.to_string_lossy().to_string());
        self.sidecar
            .call_rpc(
                "dry_run_preflight",
                serde_json::json!({
                    "prompt": prompt,
                    "videoPath": video_path_str
                }),
                Duration::from_secs(120),
            )
            .await
    }

    pub async fn read_credit_balance(&mut self) -> Result<serde_json::Value, String> {
        self.sidecar
            .call_rpc(
                "read_credit_balance",
                serde_json::json!({}),
                Duration::from_secs(45),
            )
            .await
    }

    pub async fn prepare_video_edit(
        &mut self,
        prompt: &str,
        video_path: Option<&Path>,
        duration_sec: Option<f64>,
        requested_config: Option<&super::manifest::FlowRequestedGenerationConfig>,
        local_submission_attempt_id: &str,
    ) -> Result<PreparedFlowSubmission, String> {
        let video_path_str = video_path.map(|p| p.to_string_lossy().to_string());
        let val = self
            .sidecar
            .call_rpc(
                "prepare_video_edit_submission",
                serde_json::json!({
                    "prompt": prompt,
                    "videoPath": video_path_str,
                    "durationSec": duration_sec,
                    "requestedConfig": requested_config,
                    "localSubmissionAttemptId": local_submission_attempt_id,
                }),
                Duration::from_secs(90),
            )
            .await?;

        serde_json::from_value(val)
            .map_err(|e| format!("Failed to parse prepared flow submission: {}", e))
    }

    pub async fn submit_prepared(
        &mut self,
        local_submission_attempt_id: &str,
        expected_live_cost: u32,
        max_credits: u32,
        expected_fingerprint: &str,
        expected_config: Option<&super::manifest::FlowRequestedGenerationConfig>,
        prompt_hash: Option<&str>,
        source_identity: Option<&str>,
    ) -> Result<FlowSubmissionOutcome, String> {
        let mut cfg_val = match expected_config {
            Some(c) => serde_json::to_value(c).unwrap_or(serde_json::Value::Null),
            None => serde_json::Value::Null,
        };
        if let Some(obj) = cfg_val.as_object_mut() {
            if let Some(ph) = prompt_hash {
                obj.insert(
                    "promptHash".to_string(),
                    serde_json::Value::String(ph.to_string()),
                );
            }
            if let Some(si) = source_identity {
                obj.insert(
                    "sourceIdentity".to_string(),
                    serde_json::Value::String(si.to_string()),
                );
            }
        }

        let val = self
            .sidecar
            .call_rpc(
                "submit_prepared_video_edit",
                serde_json::json!({
                    "localSubmissionAttemptId": local_submission_attempt_id,
                    "expectedLiveCost": expected_live_cost,
                    "maxCredits": max_credits,
                    "expectedFingerprint": expected_fingerprint,
                    "expectedConfig": cfg_val,
                }),
                Duration::from_secs(90),
            )
            .await?;

        serde_json::from_value(val)
            .map_err(|e| format!("Failed to parse submission outcome: {}", e))
    }

    pub async fn submit(
        &mut self,
        prompt: &str,
        video_path: Option<&Path>,
        duration_sec: f64,
        local_submission_attempt_id: &str,
    ) -> Result<String, String> {
        let video_path_str = video_path.map(|p| p.to_string_lossy().to_string());
        let submit_val = self
            .sidecar
            .call_rpc(
                "submit_prompt_generation",
                serde_json::json!({
                    "prompt": prompt,
                    "videoPath": video_path_str,
                    "durationSec": duration_sec,
                    "localSubmissionAttemptId": local_submission_attempt_id
                }),
                Duration::from_secs(90),
            )
            .await?;

        let evidence = submit_val
            .get("generationEvidence")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing generation evidence in sidecar response".to_string())?;
        Ok(evidence.to_string())
    }

    pub async fn poll(&mut self, submission_evidence: &str) -> Result<FlowPollResult, String> {
        let poll_val = self
            .sidecar
            .call_rpc(
                "poll_generation_progress",
                serde_json::json!({ "submissionEvidence": submission_evidence }),
                Duration::from_secs(30),
            )
            .await?;

        serde_json::from_value(poll_val).map_err(|e| format!("Failed to parse poll result: {}", e))
    }

    pub async fn download(
        &mut self,
        download_url: Option<&str>,
        destination_path: &Path,
    ) -> Result<PathBuf, String> {
        let full_download_url = download_url.map(|dl| {
            if dl.starts_with("http://") || dl.starts_with("https://") {
                dl.to_string()
            } else {
                format!("{}{}", self.target_url.trim_end_matches('/'), dl)
            }
        });

        let dl_val = self
            .sidecar
            .call_rpc(
                "download_artifact",
                serde_json::json!({
                    "downloadUrl": full_download_url,
                    "destinationPath": destination_path.to_string_lossy()
                }),
                Duration::from_secs(120),
            )
            .await?;

        let success = dl_val
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if success && destination_path.exists() {
            Ok(destination_path.to_path_buf())
        } else {
            Err(format!(
                "DOWNLOAD_FAILED: Artifact was not created at {:?}",
                destination_path
            ))
        }
    }

    pub async fn recover_submission(
        &mut self,
        provider_project_url: &str,
        expected_source_stem: &str,
        submitted_at: Option<&str>,
        destination_path: Option<&Path>,
    ) -> Result<FlowRecoveryResult, String> {
        let dest_str = destination_path.map(|p| p.to_string_lossy().to_string());
        let val = self
            .sidecar
            .call_rpc(
                "recover_existing_submission",
                serde_json::json!({
                    "providerProjectUrl": provider_project_url,
                    "expectedSourceStem": expected_source_stem,
                    "submittedAt": submitted_at,
                    "destinationPath": dest_str,
                }),
                Duration::from_secs(120),
            )
            .await?;

        serde_json::from_value(val).map_err(|e| format!("Failed to parse recovery result: {}", e))
    }

    pub async fn close(mut self) {
        self.sidecar.close().await;
    }
}
