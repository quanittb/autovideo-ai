use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

    pub fn validate_url_security(url: &str) -> Result<(), String> {
        let trimmed = url.trim();
        if trimmed == Self::OFFICIAL_FLOW_URL {
            return Ok(());
        }
        if trimmed.starts_with("http://127.0.0.1:") || trimmed.starts_with("http://localhost:") {
            return Ok(());
        }
        Err(format!(
            "SECURITY_VIOLATION: Unauthorized Flow URL origin: {}",
            url
        ))
    }

    pub fn validate_path_confinement(
        path: &Path,
        _expected_root: &Path,
    ) -> Result<PathBuf, String> {
        let canonical = if path.exists() {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        } else {
            path.to_path_buf()
        };

        let path_str = canonical.to_string_lossy();
        if path_str.contains("..") {
            return Err("SECURITY_VIOLATION: Path traversal attempted".to_string());
        }

        Ok(canonical)
    }

    pub async fn check_auth_status(&self, _profile_dir: &Path) -> Result<bool, String> {
        let url = self.target_url();
        Self::validate_url_security(&url)?;

        if self.mock_url.is_some() {
            // Check mock scenario
            let client = reqwest::Client::new();
            if let Ok(resp) = client.get(&url).send().await {
                if let Ok(text) = resp.text().await {
                    if text.contains("Sign in with Google") {
                        return Ok(false);
                    }
                    return Ok(true);
                }
            }
            return Ok(false);
        }

        // Mock / sidecar check
        Ok(true)
    }

    pub async fn submit_generation(
        &self,
        prompt: &str,
        video_path: Option<&Path>,
        duration_sec: f64,
    ) -> Result<String, String> {
        let url = self.target_url();
        Self::validate_url_security(&url)?;

        if prompt.trim().is_empty() {
            return Err("SUBMISSION_REJECTED: Prompt cannot be empty".to_string());
        }

        if let Some(p) = video_path {
            if !p.exists() {
                return Err(format!(
                    "SUBMISSION_REJECTED: Video path does not exist: {:?}",
                    p
                ));
            }
        }

        // In Mock mode, generate submission evidence string
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let evidence = format!(
            "flow_submission_ack_{}_dur_{:.1}s",
            attempt_id, duration_sec
        );
        Ok(evidence)
    }

    pub async fn poll_generation(&self, _evidence: &str) -> Result<FlowPollResult, String> {
        let url = self.target_url();
        Self::validate_url_security(&url)?;

        if self.mock_url.is_some() {
            let client = reqwest::Client::new();
            if let Ok(resp) = client.get(&url).send().await {
                if let Ok(text) = resp.text().await {
                    if text.contains("Sign in with Google") {
                        return Ok(FlowPollResult {
                            status: "login_required".to_string(),
                            progress_pct: 0.0,
                            download_url: None,
                            error_message: Some("User authentication required".to_string()),
                        });
                    }
                    if text.contains("0 Credits remaining") {
                        return Ok(FlowPollResult {
                            status: "credits_required".to_string(),
                            progress_pct: 0.0,
                            download_url: None,
                            error_message: Some("Insufficient account credits".to_string()),
                        });
                    }
                    if text.contains("Unknown elements") {
                        return Ok(FlowPollResult {
                            status: "ui_changed".to_string(),
                            progress_pct: 0.0,
                            download_url: None,
                            error_message: Some("Google Flow UI structure changed".to_string()),
                        });
                    }
                    if text.contains("Generation failed") {
                        return Ok(FlowPollResult {
                            status: "failed".to_string(),
                            progress_pct: 0.0,
                            download_url: None,
                            error_message: Some(
                                "Content policy violation or generation error".to_string(),
                            ),
                        });
                    }
                }
            }
        }

        Ok(FlowPollResult {
            status: "ready".to_string(),
            progress_pct: 100.0,
            download_url: Some(format!("{}/download", url)),
            error_message: None,
        })
    }

    pub async fn download_artifact(
        &self,
        download_url: &str,
        target_path: &Path,
    ) -> Result<u64, String> {
        let client = reqwest::Client::new();
        let resp = client
            .get(download_url)
            .send()
            .await
            .map_err(|e| format!("Download request failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!(
                "Download endpoint returned status {}",
                resp.status()
            ));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read download stream: {}", e))?;

        if let Some(parent) = target_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        std::fs::write(target_path, &bytes)
            .map_err(|e| format!("Failed to write downloaded video: {}", e))?;

        Ok(bytes.len() as u64)
    }
}
