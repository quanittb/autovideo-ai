use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowProfileInfo {
    pub profile_id: String,
    pub name: String,
    pub profile_dir: PathBuf,
    pub is_locked: bool,
    pub is_authenticated: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug)]
pub struct FlowProfileGuard {
    profile_id: String,
    lock_file: PathBuf,
    manager_locks: Arc<Mutex<HashSet<String>>>,
}

impl Drop for FlowProfileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_file);
        if let Ok(mut locks) = self.manager_locks.lock() {
            locks.remove(&self.profile_id);
        }
    }
}

#[derive(Debug, Clone)]
pub struct FlowProfileManager {
    base_dir: PathBuf,
    active_locks: Arc<Mutex<HashSet<String>>>,
}

impl FlowProfileManager {
    pub fn new(base_dir: PathBuf) -> Self {
        let profiles_dir = base_dir.join("flow_profiles");
        let _ = fs::create_dir_all(&profiles_dir);
        Self {
            base_dir: profiles_dir,
            active_locks: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn sanitize_profile_id(id: &str) -> Result<String, String> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err("PROFILE_INVALID: Profile ID cannot be empty".to_string());
        }
        if trimmed.contains("..") || trimmed.contains('/') || trimmed.contains('\\') {
            return Err(
                "SECURITY_VIOLATION: Profile ID contains path traversal characters".to_string(),
            );
        }
        if !trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err("PROFILE_INVALID: Profile ID contains invalid characters".to_string());
        }
        Ok(trimmed.to_string())
    }

    pub fn get_profile_dir(&self, profile_id: &str) -> Result<PathBuf, String> {
        let clean_id = Self::sanitize_profile_id(profile_id)?;
        let target = self.base_dir.join(&clean_id);
        Ok(target)
    }

    pub fn create_profile(&self, profile_id: &str, name: &str) -> Result<FlowProfileInfo, String> {
        let clean_id = Self::sanitize_profile_id(profile_id)?;
        let target_dir = self.get_profile_dir(&clean_id)?;
        fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create profile directory: {}", e))?;

        let now = Utc::now().to_rfc3339();
        let meta_file = target_dir.join("profile_meta.json");
        let info = FlowProfileInfo {
            profile_id: clean_id,
            name: if name.trim().is_empty() {
                "Default Profile".to_string()
            } else {
                name.trim().to_string()
            },
            profile_dir: target_dir,
            is_locked: false,
            is_authenticated: false,
            created_at: now.clone(),
            updated_at: now,
        };

        let json = serde_json::to_string_pretty(&info)
            .map_err(|e| format!("Serialization error: {}", e))?;
        fs::write(&meta_file, json).map_err(|e| format!("Failed to write profile meta: {}", e))?;

        Ok(info)
    }

    pub fn list_profiles(&self) -> Vec<FlowProfileInfo> {
        let mut profiles = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let meta_file = path.join("profile_meta.json");
                    if meta_file.exists() {
                        if let Ok(data) = fs::read_to_string(&meta_file) {
                            if let Ok(mut info) = serde_json::from_str::<FlowProfileInfo>(&data) {
                                if let Ok(locks) = self.active_locks.lock() {
                                    info.is_locked = locks.contains(&info.profile_id);
                                }
                                profiles.push(info);
                            }
                        }
                    }
                }
            }
        }
        profiles
    }

    pub fn try_lock_profile(&self, profile_id: &str) -> Result<FlowProfileGuard, String> {
        let clean_id = Self::sanitize_profile_id(profile_id)?;
        let target_dir = self.get_profile_dir(&clean_id)?;
        if !target_dir.exists() {
            return Err(format!(
                "PROFILE_NOT_FOUND: Profile {} does not exist",
                clean_id
            ));
        }

        let mut locks = self
            .active_locks
            .lock()
            .map_err(|_| "Lock poisoned".to_string())?;
        if locks.contains(&clean_id) {
            return Err(format!(
                "PROFILE_IN_USE: Profile {} is currently locked by another operation",
                clean_id
            ));
        }

        let lock_file = target_dir.join(".session.lock");
        if lock_file.exists() {
            // Check if stale lock file (older than 1 hour) or active
            let _ = fs::remove_file(&lock_file);
        }

        fs::write(&lock_file, Utc::now().to_rfc3339().as_bytes())
            .map_err(|e| format!("Failed to create lock file: {}", e))?;

        locks.insert(clean_id.clone());

        Ok(FlowProfileGuard {
            profile_id: clean_id,
            lock_file,
            manager_locks: self.active_locks.clone(),
        })
    }

    pub fn delete_profile(
        &self,
        profile_id: &str,
        is_referenced_by_jobs: bool,
    ) -> Result<(), String> {
        if is_referenced_by_jobs {
            return Err(format!("PROFILE_IN_USE: Cannot delete profile {} because it is referenced by active Flow jobs", profile_id));
        }

        let clean_id = Self::sanitize_profile_id(profile_id)?;
        if let Ok(locks) = self.active_locks.lock() {
            if locks.contains(&clean_id) {
                return Err(format!(
                    "PROFILE_IN_USE: Cannot delete profile {} while it is currently locked",
                    clean_id
                ));
            }
        }

        let target_dir = self.get_profile_dir(&clean_id)?;
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)
                .map_err(|e| format!("Failed to remove profile directory: {}", e))?;
        }
        Ok(())
    }
}
