use super::manifest::FlowGenerationManifest;
use crate::system::StoragePaths;
use chrono::Utc;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct FlowJobStore {
    storage_paths: StoragePaths,
}

impl FlowJobStore {
    pub fn new(storage_paths: StoragePaths) -> Self {
        Self { storage_paths }
    }

    pub fn sanitize_parent_id(id: &str) -> Result<String, String> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err("PARENT_ID_EMPTY: Parent ID cannot be empty".to_string());
        }
        if trimmed.contains("..") || trimmed.contains('/') || trimmed.contains('\\') {
            return Err("SECURITY_VIOLATION: Path traversal in parent ID".to_string());
        }
        Ok(trimmed.to_string())
    }

    pub fn parent_flow_job_dir(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<PathBuf, String> {
        let clean_parent = Self::sanitize_parent_id(parent_id)?;
        let project_dir = self.storage_paths.projects_dir.join(project_id);
        let flow_dir = project_dir.join("flow-jobs").join(clean_parent);
        Ok(flow_dir)
    }

    pub fn manifest_path(&self, project_id: &str, parent_id: &str) -> Result<PathBuf, String> {
        let dir = self.parent_flow_job_dir(project_id, parent_id)?;
        Ok(dir.join("manifest.json"))
    }

    pub fn save_manifest_atomic(
        &self,
        manifest: &mut FlowGenerationManifest,
    ) -> Result<(), String> {
        let clean_parent = Self::sanitize_parent_id(&manifest.parent_id)?;
        manifest.parent_id = clean_parent;

        let target_file = self.manifest_path(&manifest.project_id, &manifest.parent_id)?;
        let parent_dir = target_file
            .parent()
            .ok_or_else(|| "Invalid target directory".to_string())?;
        fs::create_dir_all(parent_dir)
            .map_err(|e| format!("Failed to create parent flow directory: {}", e))?;

        if target_file.exists() {
            if let Ok(existing_bytes) = fs::read(&target_file) {
                if let Ok(existing) =
                    serde_json::from_slice::<FlowGenerationManifest>(&existing_bytes)
                {
                    if existing.state_revision > manifest.state_revision {
                        return Err(format!(
                            "STALE_STATE_REVISION_CAS_REJECTED: Existing manifest has revision {}, attempted write with {}",
                            existing.state_revision, manifest.state_revision
                        ));
                    }
                }
            }
        }

        manifest.state_revision += 1;
        manifest.timestamps.updated_at = Utc::now().to_rfc3339();

        let json_bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|e| format!("Serialization error: {}", e))?;

        let tmp_file = parent_dir.join(format!(
            "manifest.json.{}.tmp",
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        fs::write(&tmp_file, &json_bytes)
            .map_err(|e| format!("Failed to write tmp manifest: {}", e))?;

        #[cfg(target_os = "windows")]
        {
            if target_file.exists() {
                let _ = fs::remove_file(&target_file);
            }
            fs::rename(&tmp_file, &target_file)
                .map_err(|e| format!("Atomic rename failed: {}", e))?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            fs::rename(&tmp_file, &target_file)
                .map_err(|e| format!("Atomic rename failed: {}", e))?;
        }

        Ok(())
    }

    pub fn load_manifest(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<FlowGenerationManifest, String> {
        let target_file = self.manifest_path(project_id, parent_id)?;
        if !target_file.exists() {
            return Err(format!(
                "FLOW_JOB_NOT_FOUND: Manifest not found for parent {}",
                parent_id
            ));
        }

        let bytes =
            fs::read(&target_file).map_err(|e| format!("Failed to read manifest: {}", e))?;
        let manifest: FlowGenerationManifest = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Corrupt flow manifest JSON: {}", e))?;

        Ok(manifest)
    }

    pub fn list_all_flow_jobs(
        &self,
        project_id: &str,
    ) -> Result<Vec<FlowGenerationManifest>, String> {
        let project_dir = self.storage_paths.projects_dir.join(project_id);
        let flow_base_dir = project_dir.join("flow-jobs");
        if !flow_base_dir.exists() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&flow_base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let manifest_file = path.join("manifest.json");
                    if manifest_file.exists() {
                        if let Ok(bytes) = fs::read(&manifest_file) {
                            if let Ok(manifest) =
                                serde_json::from_slice::<FlowGenerationManifest>(&bytes)
                            {
                                out.push(manifest);
                            }
                        }
                    }
                }
            }
        }

        out.sort_by(|a, b| b.timestamps.created_at.cmp(&a.timestamps.created_at));
        Ok(out)
    }
}
