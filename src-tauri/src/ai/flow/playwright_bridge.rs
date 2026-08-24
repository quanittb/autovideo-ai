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
}

impl FlowAuthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::LoginRequired => "LOGIN_REQUIRED",
            Self::Unknown => "UNKNOWN",
            Self::FlowUiChanged => "FLOW_UI_CHANGED",
            Self::FlowEligibilityRequired => "FLOW_ELIGIBILITY_REQUIRED",
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
    pub const OFFICIAL_FLOW_URL: &'static str = "https://labs.google/fx/tools/flow";

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
        if trimmed == Self::OFFICIAL_FLOW_URL || trimmed == "https://labs.google/flow" {
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
                Duration::from_secs(10),
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
                } else if e.contains("LOGIN_REQUIRED") {
                    Ok(FlowAuthStatus::LoginRequired)
                } else {
                    Err(e)
                }
            }
        }
    }

    pub async fn submit_generation(
        &self,
        profile_dir: &Path,
        prompt: &str,
        video_path: Option<&Path>,
        duration_sec: f64,
        local_submission_attempt_id: &str,
    ) -> Result<String, String> {
        let url = self.target_url();
        self.validate_url_security(&url)?;

        let mut sidecar = PlaywrightSidecarProcess::spawn().await?;

        sidecar
            .call_rpc(
                "launch_browser",
                self.launch_browser_params(profile_dir, true),
                Duration::from_secs(30),
            )
            .await?;

        sidecar
            .call_rpc(
                "navigate_to_flow",
                serde_json::json!({ "flowUrl": url }),
                Duration::from_secs(30),
            )
            .await?;

        let video_path_str = video_path.map(|p| p.to_string_lossy().to_string());
        let submit_val = sidecar
            .call_rpc(
                "submit_prompt_generation",
                serde_json::json!({
                    "prompt": prompt,
                    "videoPath": video_path_str,
                    "durationSec": duration_sec,
                    "localSubmissionAttemptId": local_submission_attempt_id
                }),
                Duration::from_secs(30),
            )
            .await;

        sidecar.close().await;

        match submit_val {
            Ok(val) => {
                let evidence = val
                    .get("generationEvidence")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "Missing generation evidence in sidecar response".to_string())?;
                Ok(evidence.to_string())
            }
            Err(e) => Err(e),
        }
    }

    pub async fn poll_generation(
        &self,
        profile_dir: &Path,
        submission_evidence: &str,
    ) -> Result<FlowPollResult, String> {
        let url = self.target_url();
        self.validate_url_security(&url)?;

        let mut sidecar = PlaywrightSidecarProcess::spawn().await?;

        sidecar
            .call_rpc(
                "launch_browser",
                self.launch_browser_params(profile_dir, true),
                Duration::from_secs(30),
            )
            .await?;

        sidecar
            .call_rpc(
                "navigate_to_flow",
                serde_json::json!({ "flowUrl": url }),
                Duration::from_secs(30),
            )
            .await?;

        let poll_val = sidecar
            .call_rpc(
                "poll_generation_progress",
                serde_json::json!({ "submissionEvidence": submission_evidence }),
                Duration::from_secs(20),
            )
            .await;

        sidecar.close().await;

        match poll_val {
            Ok(val) => serde_json::from_value(val)
                .map_err(|e| format!("Failed to parse poll result: {}", e)),
            Err(e) => Err(e),
        }
    }

    pub async fn download_artifact(
        &self,
        profile_dir: &Path,
        download_url: &str,
        destination_path: &Path,
    ) -> Result<PathBuf, String> {
        let url = self.target_url();
        self.validate_url_security(&url)?;

        let mut sidecar = PlaywrightSidecarProcess::spawn().await?;

        sidecar
            .call_rpc(
                "launch_browser",
                self.launch_browser_params(profile_dir, true),
                Duration::from_secs(30),
            )
            .await?;

        let full_download_url =
            if download_url.starts_with("http://") || download_url.starts_with("https://") {
                download_url.to_string()
            } else {
                format!("{}{}", url.trim_end_matches('/'), download_url)
            };

        let dl_val = sidecar
            .call_rpc(
                "download_artifact",
                serde_json::json!({
                    "downloadUrl": full_download_url,
                    "destinationPath": destination_path.to_string_lossy()
                }),
                Duration::from_secs(30),
            )
            .await;

        sidecar.close().await;

        match dl_val {
            Ok(_) => {
                if destination_path.exists() {
                    Ok(destination_path.to_path_buf())
                } else {
                    Err(format!(
                        "DOWNLOAD_FAILED: Artifact was not created at {:?}",
                        destination_path
                    ))
                }
            }
            Err(e) => Err(e),
        }
    }
}
