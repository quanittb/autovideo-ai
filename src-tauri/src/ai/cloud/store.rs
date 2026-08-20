use super::error::CloudProviderError;
use super::job::PersistentCloudJob;
use crate::system::StoragePaths;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// -----------------------------------------------------------------------------
// Windows-Safe Atomic File Replacement
// -----------------------------------------------------------------------------

#[cfg(windows)]
pub fn atomic_replace(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let src_wide: Vec<u16> = OsStr::new(src)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let dst_wide: Vec<u16> = OsStr::new(dst)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    extern "system" {
        fn MoveFileExW(
            lpExistingFileName: *const u16,
            lpNewFileName: *const u16,
            dwFlags: u32,
        ) -> i32;
    }

    // 0x1 = MOVEFILE_REPLACE_EXISTING, 0x8 = MOVEFILE_WRITE_THROUGH
    let res = unsafe { MoveFileExW(src_wide.as_ptr(), dst_wide.as_ptr(), 0x1 | 0x8) };
    if res == 0 {
        // Fallback to std::fs::rename if MoveFileExW failed
        std::fs::rename(src, dst)
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn atomic_replace(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

// -----------------------------------------------------------------------------
// Identifier Path Validation
// -----------------------------------------------------------------------------

pub fn validate_identifier(id: &str, field_name: &str) -> Result<(), CloudProviderError> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(CloudProviderError::RequestInvalid(format!(
            "INVALID_IDENTIFIER: {} cannot be empty",
            field_name
        )));
    }
    if trimmed.contains("..")
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains(':')
        || trimmed.contains('\0')
    {
        return Err(CloudProviderError::RequestInvalid(format!(
            "INVALID_IDENTIFIER: {} '{}' contains illegal characters or path traversal elements",
            field_name, trimmed
        )));
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// PersistentCloudJobStore
// -----------------------------------------------------------------------------

#[derive(Clone)]
pub struct PersistentCloudJobStore {
    pub storage_paths: StoragePaths,
    pub fail_next_save: Arc<AtomicBool>,
}

impl PersistentCloudJobStore {
    pub fn new(storage_paths: StoragePaths) -> Self {
        Self {
            storage_paths,
            fail_next_save: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_fail_next_save(&self, fail: bool) {
        self.fail_next_save.store(fail, Ordering::SeqCst);
    }

    pub fn project_cloud_jobs_dir(&self, project_id: &str) -> Result<PathBuf, CloudProviderError> {
        validate_identifier(project_id, "projectId")?;
        let dir = self
            .storage_paths
            .projects_dir
            .join(project_id)
            .join("cloud-jobs");
        Ok(dir)
    }

    pub fn project_artifacts_dir(&self, project_id: &str) -> Result<PathBuf, CloudProviderError> {
        let jobs_dir = self.project_cloud_jobs_dir(project_id)?;
        Ok(jobs_dir.join("artifacts"))
    }

    pub fn job_file_path(
        &self,
        project_id: &str,
        internal_job_id: &str,
    ) -> Result<PathBuf, CloudProviderError> {
        validate_identifier(internal_job_id, "internalJobId")?;
        let jobs_dir = self.project_cloud_jobs_dir(project_id)?;
        Ok(jobs_dir.join(format!("{}.json", internal_job_id)))
    }

    pub fn job_tmp_file_path(
        &self,
        project_id: &str,
        internal_job_id: &str,
    ) -> Result<PathBuf, CloudProviderError> {
        validate_identifier(internal_job_id, "internalJobId")?;
        let jobs_dir = self.project_cloud_jobs_dir(project_id)?;
        Ok(jobs_dir.join(format!("{}.json.tmp", internal_job_id)))
    }

    pub fn artifact_final_path(
        &self,
        project_id: &str,
        internal_job_id: &str,
    ) -> Result<PathBuf, CloudProviderError> {
        validate_identifier(internal_job_id, "internalJobId")?;
        let artifacts_dir = self.project_artifacts_dir(project_id)?;
        Ok(artifacts_dir.join(format!("{}.mp4", internal_job_id)))
    }

    pub fn artifact_partial_path(
        &self,
        project_id: &str,
        internal_job_id: &str,
    ) -> Result<PathBuf, CloudProviderError> {
        validate_identifier(internal_job_id, "internalJobId")?;
        let artifacts_dir = self.project_artifacts_dir(project_id)?;
        Ok(artifacts_dir.join(format!("{}.partial", internal_job_id)))
    }

    pub fn ensure_project_cloud_dirs(&self, project_id: &str) -> Result<(), CloudProviderError> {
        let jobs_dir = self.project_cloud_jobs_dir(project_id)?;
        let artifacts_dir = self.project_artifacts_dir(project_id)?;
        fs::create_dir_all(&jobs_dir).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to create cloud jobs dir {}: {}",
                jobs_dir.display(),
                e
            ))
        })?;
        fs::create_dir_all(&artifacts_dir).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to create cloud artifacts dir {}: {}",
                artifacts_dir.display(),
                e
            ))
        })?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Atomic Manifest Persistence
    // -------------------------------------------------------------------------

    pub fn save_job_atomic(&self, job: &PersistentCloudJob) -> Result<(), CloudProviderError> {
        if self.fail_next_save.swap(false, Ordering::SeqCst) {
            return Err(CloudProviderError::ProviderUnavailable(
                "SIMULATED_PERSISTENCE_FAILURE: Injected I/O error during atomic save".to_string(),
            ));
        }

        self.ensure_project_cloud_dirs(&job.project_id)?;

        let primary_path = self.job_file_path(&job.project_id, &job.internal_job_id)?;
        let tmp_path = self.job_tmp_file_path(&job.project_id, &job.internal_job_id)?;

        let serialized = serde_json::to_string_pretty(job).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to serialize PersistentCloudJob: {}",
                e
            ))
        })?;

        // 1. Write to temporary file with sync
        {
            let mut file = File::create(&tmp_path).map_err(|e| {
                CloudProviderError::ProviderUnavailable(format!(
                    "Failed to create temp job file {}: {}",
                    tmp_path.display(),
                    e
                ))
            })?;
            file.write_all(serialized.as_bytes()).map_err(|e| {
                CloudProviderError::ProviderUnavailable(format!(
                    "Failed to write temp job file {}: {}",
                    tmp_path.display(),
                    e
                ))
            })?;
            file.sync_all().map_err(|e| {
                CloudProviderError::ProviderUnavailable(format!(
                    "Failed to sync temp job file {}: {}",
                    tmp_path.display(),
                    e
                ))
            })?;
        }

        // 2. Windows-safe atomic replace
        // Note: Do not delete .tmp on atomic_replace failure so that newer fsynced data is preserved for recovery evidence
        atomic_replace(&tmp_path, &primary_path).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to atomically persist {}: {}",
                primary_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Crash-Safe Load with Monotonic State Revision Recovery
    // -------------------------------------------------------------------------

    pub fn load_job(
        &self,
        project_id: &str,
        internal_job_id: &str,
    ) -> Result<PersistentCloudJob, CloudProviderError> {
        let primary_path = self.job_file_path(project_id, internal_job_id)?;
        let tmp_path = self.job_tmp_file_path(project_id, internal_job_id)?;

        let primary_res: Result<PersistentCloudJob, _> = if primary_path.exists() {
            fs::read_to_string(&primary_path)
                .map_err(|e| e.to_string())
                .and_then(|c| {
                    serde_json::from_str::<PersistentCloudJob>(&c).map_err(|e| e.to_string())
                })
        } else {
            Err("Primary file missing".to_string())
        };

        let tmp_res: Result<PersistentCloudJob, _> = if tmp_path.exists() {
            fs::read_to_string(&tmp_path)
                .map_err(|e| e.to_string())
                .and_then(|c| {
                    serde_json::from_str::<PersistentCloudJob>(&c).map_err(|e| e.to_string())
                })
        } else {
            Err("Temp file missing".to_string())
        };

        match (primary_res, tmp_res) {
            // Case 1: Both primary and temp are valid -> Compare state_revision!
            (Ok(primary_job), Ok(tmp_job)) => {
                if tmp_job.state_revision > primary_job.state_revision {
                    // Newer state was in temp before crash -> promote temp!
                    atomic_replace(&tmp_path, &primary_path).map_err(|e| {
                        CloudProviderError::ProviderUnavailable(format!(
                            "Failed to promote recovered tmp file to primary: {}",
                            e
                        ))
                    })?;
                    Ok(tmp_job)
                } else {
                    // Primary is same or newer -> clean up stale temp
                    let _ = fs::remove_file(&tmp_path);
                    Ok(primary_job)
                }
            }

            // Case 2: Primary valid, temp missing or corrupt -> use primary
            (Ok(primary_job), Err(_)) => {
                if tmp_path.exists() {
                    let _ = fs::remove_file(&tmp_path);
                }
                Ok(primary_job)
            }

            // Case 3: Primary missing, temp valid -> promote temp
            (Err(e), Ok(tmp_job)) if e.contains("Primary file missing") => {
                atomic_replace(&tmp_path, &primary_path).map_err(|e| {
                    CloudProviderError::ProviderUnavailable(format!(
                        "Failed to promote recovered tmp file to primary: {}",
                        e
                    ))
                })?;
                Ok(tmp_job)
            }

            // Case 4: Primary corrupt, temp valid -> backup corrupt primary and promote temp
            (Err(_), Ok(tmp_job)) => {
                let corrupt_path = primary_path.with_extension("json.corrupt");
                let _ = atomic_replace(&primary_path, &corrupt_path);
                atomic_replace(&tmp_path, &primary_path).map_err(|e| {
                    CloudProviderError::ProviderUnavailable(format!(
                        "Failed to promote recovered tmp file over corrupt primary: {}",
                        e
                    ))
                })?;
                Ok(tmp_job)
            }

            // Case 5: Both missing or corrupt -> recovery failure
            (Err(e1), Err(e2)) => Err(CloudProviderError::ProviderUnavailable(format!(
                "RECOVERY_FAILED: Primary corrupt/missing ({}) and temp corrupt/missing ({}) for job {}",
                e1, e2, internal_job_id
            ))),
        }
    }

    // -------------------------------------------------------------------------
    // Querying & Listing
    // -------------------------------------------------------------------------

    pub fn list_jobs_in_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<PersistentCloudJob>, CloudProviderError> {
        let jobs_dir = self.project_cloud_jobs_dir(project_id)?;
        if !jobs_dir.exists() {
            return Ok(Vec::new());
        }

        let mut jobs = Vec::new();
        let entries = fs::read_dir(&jobs_dir).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to read cloud jobs directory {}: {}",
                jobs_dir.display(),
                e
            ))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some("json")
                && !path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(".tmp")
            {
                let file_stem = path.file_stem().unwrap().to_string_lossy().to_string();
                if let Ok(job) = self.load_job(project_id, &file_stem) {
                    jobs.push(job);
                }
            }
        }

        jobs.sort_by(|a, b| b.timestamps.created_at.cmp(&a.timestamps.created_at));
        Ok(jobs)
    }

    pub fn list_all_active_jobs(&self) -> Result<Vec<PersistentCloudJob>, CloudProviderError> {
        let projects_dir = &self.storage_paths.projects_dir;
        if !projects_dir.exists() {
            return Ok(Vec::new());
        }

        let mut active_jobs = Vec::new();
        if let Ok(entries) = fs::read_dir(projects_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let project_id = entry.file_name().to_string_lossy().to_string();
                    if let Ok(jobs) = self.list_jobs_in_project(&project_id) {
                        for job in jobs {
                            if !job.state.is_terminal() {
                                active_jobs.push(job);
                            }
                        }
                    }
                }
            }
        }
        Ok(active_jobs)
    }

    pub fn find_job_by_client_request_id(
        &self,
        project_id: &str,
        client_request_id: &str,
    ) -> Result<Option<PersistentCloudJob>, CloudProviderError> {
        let jobs = self.list_jobs_in_project(project_id)?;
        for job in jobs {
            if job.job_id == client_request_id || job.internal_job_id == client_request_id {
                return Ok(Some(job));
            }
        }
        Ok(None)
    }
}
