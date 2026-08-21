use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowProfileSnapshot {
    pub profile_id: String,
    pub name: String,
    pub status: String, // "READY" | "LOGIN_REQUIRED" | "UNKNOWN"
    pub is_locked: bool,
    pub created_at: String,
    pub updated_at: String,
}

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

impl FlowProfileInfo {
    pub fn to_snapshot(&self) -> FlowProfileSnapshot {
        FlowProfileSnapshot {
            profile_id: self.profile_id.clone(),
            name: self.name.clone(),
            status: if self.is_authenticated {
                "READY".to_string()
            } else {
                "LOGIN_REQUIRED".to_string()
            },
            is_locked: self.is_locked,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LockMetadata {
    pid: u32,
    instance_id: String,
    locked_at: String,
}

#[derive(Debug)]
pub struct FlowProfileGuard {
    lock_file: PathBuf,
}

impl Drop for FlowProfileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_file);
    }
}

#[derive(Debug, Clone)]
pub struct FlowProfileManager {
    base_dir: PathBuf,
}

impl FlowProfileManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let profiles_dir = app_data_dir.join("flow_profiles");
        let _ = fs::create_dir_all(&profiles_dir);
        Self {
            base_dir: profiles_dir,
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

        let canonical_base = fs::canonicalize(&self.base_dir)
            .map_err(|e| format!("Invalid base profiles directory: {}", e))?;

        if target.exists() {
            let canonical_target = fs::canonicalize(&target)
                .map_err(|e| format!("Invalid profile directory: {}", e))?;
            if !canonical_target.starts_with(&canonical_base) {
                return Err("SECURITY_VIOLATION: Profile directory escapes base".to_string());
            }
            Ok(canonical_target)
        } else {
            Ok(target)
        }
    }

    pub fn create_profile(
        &self,
        profile_id: &str,
        name: &str,
    ) -> Result<FlowProfileSnapshot, String> {
        let clean_id = Self::sanitize_profile_id(profile_id)?;
        let target_dir = self.base_dir.join(&clean_id);
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
        fs::write(meta_file, json)
            .map_err(|e| format!("Failed to write profile metadata: {}", e))?;

        Ok(info.to_snapshot())
    }

    pub fn list_profiles(&self) -> Vec<FlowProfileSnapshot> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let meta_file = p.join("profile_meta.json");
                    if meta_file.exists() {
                        if let Ok(content) = fs::read_to_string(&meta_file) {
                            if let Ok(mut info) = serde_json::from_str::<FlowProfileInfo>(&content)
                            {
                                info.is_locked = p.join(".session.lock").exists();
                                out.push(info.to_snapshot());
                            }
                        }
                    }
                }
            }
        }
        out
    }

    pub fn acquire_session_lock(&self, profile_id: &str) -> Result<FlowProfileGuard, String> {
        let profile_dir = self.get_profile_dir(profile_id)?;
        if !profile_dir.exists() {
            return Err(format!(
                "PROFILE_NOT_FOUND: Profile {} does not exist",
                profile_id
            ));
        }

        let lock_file = profile_dir.join(".session.lock");

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_file)
        {
            Ok(mut file) => {
                let meta = LockMetadata {
                    pid: std::process::id(),
                    instance_id: Uuid::new_v4().to_string(),
                    locked_at: Utc::now().to_rfc3339(),
                };
                let _ = serde_json::to_writer(&mut file, &meta);
                Ok(FlowProfileGuard { lock_file })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if Self::is_lock_stale(&lock_file) {
                    let _ = fs::remove_file(&lock_file);
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&lock_file)
                        .map_err(|_| {
                            "PROFILE_IN_USE: Profile is currently locked by another active session"
                                .to_string()
                        })?;
                    let meta = LockMetadata {
                        pid: std::process::id(),
                        instance_id: Uuid::new_v4().to_string(),
                        locked_at: Utc::now().to_rfc3339(),
                    };
                    let _ = serde_json::to_writer(&mut file, &meta);
                    Ok(FlowProfileGuard { lock_file })
                } else {
                    Err(
                        "PROFILE_IN_USE: Profile is currently locked by another active session"
                            .to_string(),
                    )
                }
            }
            Err(e) => Err(format!("Failed to acquire profile lock: {}", e)),
        }
    }

    fn is_lock_stale(lock_file: &Path) -> bool {
        if let Ok(content) = fs::read_to_string(lock_file) {
            if let Ok(meta) = serde_json::from_str::<LockMetadata>(&content) {
                // If the process that created the lock is our own PID and instance crashed/restarted
                // or if the lock is very old (> 12 hours)
                if let Ok(locked_time) = chrono::DateTime::parse_from_rfc3339(&meta.locked_at) {
                    let age = Utc::now().signed_duration_since(locked_time);
                    if age.num_hours() > 12 {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn delete_profile(&self, profile_id: &str, force: bool) -> Result<(), String> {
        let profile_dir = self.get_profile_dir(profile_id)?;
        if !profile_dir.exists() {
            return Ok(());
        }

        let lock_file = profile_dir.join(".session.lock");
        if lock_file.exists() && !force {
            return Err(
                "PROFILE_LOCKED: Cannot delete profile currently in active use".to_string(),
            );
        }

        fs::remove_dir_all(&profile_dir)
            .map_err(|e| format!("Failed to delete profile directory: {}", e))?;
        Ok(())
    }
}
