use super::manual_chrome::{ManualChromeProcess, SystemChromeLauncher};
use super::playwright_bridge::PlaywrightBridge;
use super::profile::{FlowProfileGuard, FlowProfileManager};
use crate::system::StoragePaths;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug)]
pub struct ManualLoginBrowserSession {
    pub profile_id: String,
    pub guard: FlowProfileGuard,
    pub process: ManualChromeProcess,
    pub opened_at: String,
}

pub struct FlowBrowserSessionManager {
    manual_sessions: Mutex<HashMap<String, ManualLoginBrowserSession>>,
    mock_url: Option<String>,
    custom_chrome_exe: Option<PathBuf>,
}

impl FlowBrowserSessionManager {
    pub fn new() -> Self {
        Self {
            manual_sessions: Mutex::new(HashMap::new()),
            mock_url: None,
            custom_chrome_exe: None,
        }
    }

    pub fn with_mock_url(mock_url: String) -> Self {
        Self {
            manual_sessions: Mutex::new(HashMap::new()),
            mock_url: Some(mock_url),
            custom_chrome_exe: None,
        }
    }

    pub fn with_custom_chrome(mut self, chrome_exe: PathBuf) -> Self {
        self.custom_chrome_exe = Some(chrome_exe);
        self
    }

    pub fn target_url(&self) -> String {
        self.mock_url
            .clone()
            .unwrap_or_else(|| PlaywrightBridge::OFFICIAL_FLOW_URL.to_string())
    }

    pub fn is_session_open(&self, profile_id: &str) -> bool {
        let mut map = self.manual_sessions.lock().unwrap();
        if let Some(session) = map.get_mut(profile_id) {
            if session.process.is_running() {
                true
            } else {
                // User closed Chrome window manually -> clean up
                map.remove(profile_id);
                false
            }
        } else {
            false
        }
    }

    pub fn open_session(
        &self,
        profile_id: &str,
        profile_dir: &Path,
        paths: &StoragePaths,
    ) -> Result<String, String> {
        // 1. Check if manual browser session is already open
        if self.is_session_open(profile_id) {
            return Ok("BROWSER_ALREADY_OPEN".to_string());
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

        // 4. Resolve installed Chrome executable
        let chrome_exe = if let Some(ref custom) = self.custom_chrome_exe {
            custom.clone()
        } else {
            match SystemChromeLauncher::find_chrome_executable() {
                Ok(exe) => exe,
                Err(e) => {
                    #[cfg(test)]
                    if self.mock_url.is_some() {
                        PathBuf::from("chrome.exe")
                    } else {
                        drop(profile_guard);
                        return Err(e);
                    }
                    #[cfg(not(test))]
                    {
                        drop(profile_guard);
                        return Err(e);
                    }
                }
            }
        };

        // 5. Launch standard user-driven Google Chrome process
        let process = match SystemChromeLauncher::launch(&chrome_exe, profile_dir, &url) {
            Ok(p) => p,
            Err(e) => {
                #[cfg(test)]
                if self.mock_url.is_some() {
                    ManualChromeProcess::mock(profile_dir)
                } else {
                    drop(profile_guard);
                    return Err(e);
                }
                #[cfg(not(test))]
                {
                    drop(profile_guard);
                    return Err(e);
                }
            }
        };

        let session = ManualLoginBrowserSession {
            profile_id: profile_id.to_string(),
            guard: profile_guard,
            process,
            opened_at: chrono::Utc::now().to_rfc3339(),
        };

        let mut map = self.manual_sessions.lock().unwrap();
        map.insert(profile_id.to_string(), session);

        Ok("OPEN".to_string())
    }

    pub fn close_session(&self, profile_id: &str) -> Result<(), String> {
        let mut map = self.manual_sessions.lock().unwrap();
        if let Some(mut session) = map.remove(profile_id) {
            let _ = session.process.close();
            // FlowProfileGuard drops here, releasing .session.lock
        }
        Ok(())
    }

    pub fn close_all(&self) {
        let mut map = self.manual_sessions.lock().unwrap();
        for (_, mut session) in map.drain() {
            let _ = session.process.close();
        }
    }

    pub async fn verify_login(
        &self,
        profile_id: &str,
        profile_dir: &Path,
        paths: &StoragePaths,
    ) -> Result<String, String> {
        // Precondition 1: Manual Chrome must be closed
        if self.is_session_open(profile_id) {
            return Err("LOGIN_BROWSER_STILL_OPEN: Cannot verify authentication while Google Chrome login window is open. Please close Chrome first.".to_string());
        }

        // Precondition 2: Acquire profile lock for automation verification
        let profile_mgr = FlowProfileManager::new(paths.app_data_dir.clone());
        let profile_guard = profile_mgr.acquire_session_lock(profile_id)?;

        let bridge = match self.mock_url.clone() {
            Some(m) => PlaywrightBridge::with_mock_url(m),
            None => PlaywrightBridge::new(),
        };

        let check_res = bridge.check_auth_status(profile_dir).await;
        drop(profile_guard);

        match check_res {
            Ok(status) => Ok(status.as_str().to_string()),
            Err(e) => {
                if e.contains("FLOW_UI_CHANGED") {
                    Ok("FLOW_UI_CHANGED".to_string())
                } else if e.contains("FLOW_ELIGIBILITY_REQUIRED")
                    || e.contains("ELIGIBILITY_REQUIRED")
                    || e.contains("USER_ACTION_REQUIRED")
                {
                    Ok("FLOW_ELIGIBILITY_REQUIRED".to_string())
                } else if e.contains("FLOW_LANDING") {
                    Ok("FLOW_LANDING".to_string())
                } else if e.contains("LOGIN_REQUIRED") {
                    Ok("LOGIN_REQUIRED".to_string())
                } else {
                    Err(e)
                }
            }
        }
    }

    pub async fn check_or_refresh_auth(
        &self,
        profile_id: &str,
        profile_dir: &Path,
        paths: &StoragePaths,
    ) -> Result<String, String> {
        self.verify_login(profile_id, profile_dir, paths).await
    }
}
