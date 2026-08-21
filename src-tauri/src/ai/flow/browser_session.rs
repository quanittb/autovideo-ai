use super::playwright_bridge::{PlaywrightBridge, PlaywrightSidecarProcess};
use super::profile::{FlowProfileGuard, FlowProfileManager};
use crate::system::StoragePaths;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct FlowBrowserSession {
    pub profile_id: String,
    pub guard: FlowProfileGuard,
    pub sidecar: PlaywrightSidecarProcess,
    pub opened_at: String,
    pub browser_mode: String,
    pub auth_status: String,
}

pub struct FlowBrowserSessionManager {
    sessions: Mutex<HashMap<String, Arc<tokio::sync::Mutex<FlowBrowserSession>>>>,
    mock_url: Option<String>,
}

impl FlowBrowserSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            mock_url: None,
        }
    }

    pub fn with_mock_url(mock_url: String) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            mock_url: Some(mock_url),
        }
    }

    pub fn target_url(&self) -> String {
        self.mock_url
            .clone()
            .unwrap_or_else(|| PlaywrightBridge::OFFICIAL_FLOW_URL.to_string())
    }

    pub fn is_session_open(&self, profile_id: &str) -> bool {
        let guard = self.sessions.lock().unwrap();
        guard.contains_key(profile_id)
    }

    pub fn get_session_auth_status(&self, profile_id: &str) -> Option<String> {
        let session_arc = {
            let map = self.sessions.lock().unwrap();
            map.get(profile_id).cloned()
        }?;
        let res = match session_arc.try_lock() {
            Ok(session) => session.auth_status.clone(),
            Err(_) => "UNKNOWN".to_string(),
        };
        Some(res)
    }

    pub async fn get_session_auth_status_async(&self, profile_id: &str) -> Option<String> {
        let session_arc = {
            let map = self.sessions.lock().unwrap();
            map.get(profile_id).cloned()
        }?;
        let session = session_arc.lock().await;
        Some(session.auth_status.clone())
    }

    pub async fn open_session(
        &self,
        profile_id: &str,
        profile_dir: &Path,
        paths: &StoragePaths,
    ) -> Result<String, String> {
        // 1. Check if session is already open
        {
            let guard = self.sessions.lock().unwrap();
            if guard.contains_key(profile_id) {
                return Ok("BROWSER_ALREADY_OPEN".to_string());
            }
        }

        // 2. Validate URL security
        let url = self.target_url();
        let bridge = match self.mock_url.clone() {
            Some(m) => PlaywrightBridge::with_mock_url(m),
            None => PlaywrightBridge::new(),
        };
        bridge.validate_url_security(&url)?;

        // 3. Acquire profile session lock
        let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
        let profile_guard = profile_mgr.acquire_session_lock(profile_id)?;

        // 4. Spawn Playwright sidecar process
        let mut sidecar = match PlaywrightSidecarProcess::spawn().await {
            Ok(s) => s,
            Err(e) => {
                drop(profile_guard);
                return Err(e);
            }
        };

        // 5. Launch persistent headed browser
        let launch_res = sidecar
            .call_rpc(
                "launch_browser",
                serde_json::json!({
                    "profilePath": profile_dir.to_string_lossy(),
                    "headless": false
                }),
                Duration::from_secs(30),
            )
            .await;

        if let Err(e) = launch_res {
            sidecar.close().await;
            drop(profile_guard);
            return Err(e);
        }

        // 6. Navigate to Flow
        let nav_res = sidecar
            .call_rpc(
                "navigate_to_flow",
                serde_json::json!({ "flowUrl": url }),
                Duration::from_secs(30),
            )
            .await;

        if let Err(e) = nav_res {
            sidecar.close().await;
            drop(profile_guard);
            return Err(e);
        }

        // 7. Store LIVE sidecar inside the managed session
        let session = FlowBrowserSession {
            profile_id: profile_id.to_string(),
            guard: profile_guard,
            sidecar,
            opened_at: chrono::Utc::now().to_rfc3339(),
            browser_mode: "HEADED_LOGIN".to_string(),
            auth_status: "UNKNOWN".to_string(),
        };

        {
            let mut map = self.sessions.lock().unwrap();
            map.insert(
                profile_id.to_string(),
                Arc::new(tokio::sync::Mutex::new(session)),
            );
        }

        Ok("OPEN".to_string())
    }

    pub async fn close_session(&self, profile_id: &str) -> Result<(), String> {
        let session_arc = {
            let mut map = self.sessions.lock().unwrap();
            map.remove(profile_id)
        };

        if let Some(session_arc) = session_arc {
            let mut session = session_arc.lock().await;
            session.sidecar.close().await;
            // FlowProfileGuard drops when session is dropped
        }

        Ok(())
    }

    pub async fn check_or_refresh_auth(
        &self,
        profile_id: &str,
        profile_dir: &Path,
        paths: &StoragePaths,
    ) -> Result<String, String> {
        let session_arc = {
            let map = self.sessions.lock().unwrap();
            map.get(profile_id).cloned()
        };

        if let Some(session_arc) = session_arc {
            // Check auth on existing live browser session
            let mut session = session_arc.lock().await;
            let auth_val = session
                .sidecar
                .call_rpc(
                    "check_auth_status",
                    serde_json::json!({}),
                    Duration::from_secs(10),
                )
                .await;

            match auth_val {
                Ok(val) => {
                    let status = val
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("UNKNOWN");
                    let semantic = match status {
                        "READY" => "READY",
                        "LOGIN_REQUIRED" => "LOGIN_REQUIRED",
                        _ => "UNKNOWN",
                    };
                    session.auth_status = semantic.to_string();
                    Ok(semantic.to_string())
                }
                Err(e) => Err(e),
            }
        } else {
            // No active session: perform safe temporary probe
            let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
            let profile_guard = profile_mgr.acquire_session_lock(profile_id)?;

            let bridge = match self.mock_url.clone() {
                Some(m) => PlaywrightBridge::with_mock_url(m),
                None => PlaywrightBridge::new(),
            };

            let check_res = bridge.check_auth_status(profile_dir).await;
            drop(profile_guard);

            match check_res {
                Ok(true) => Ok("READY".to_string()),
                Ok(false) => Ok("LOGIN_REQUIRED".to_string()),
                Err(e) => Err(e),
            }
        }
    }

    pub async fn close_all(&self) {
        let sessions = {
            let mut map = self.sessions.lock().unwrap();
            let drained: Vec<Arc<tokio::sync::Mutex<FlowBrowserSession>>> =
                map.drain().map(|(_, v)| v).collect();
            drained
        };

        for session_arc in sessions {
            let mut session = session_arc.lock().await;
            session.sidecar.close().await;
        }
    }
}
