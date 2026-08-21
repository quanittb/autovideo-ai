use super::error::CloudProviderError;
use super::job::{ArtifactContainer, PersistentCloudJob};
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
        self.artifact_final_path_for_container(project_id, internal_job_id, ArtifactContainer::Mp4)
    }

    pub fn artifact_final_path_for_container(
        &self,
        project_id: &str,
        internal_job_id: &str,
        container: ArtifactContainer,
    ) -> Result<PathBuf, CloudProviderError> {
        validate_identifier(internal_job_id, "internalJobId")?;
        let artifacts_dir = self.project_artifacts_dir(project_id)?;
        Ok(artifacts_dir.join(format!("{}.{}", internal_job_id, container.extension())))
    }

    pub fn artifact_final_path_for_job(
        &self,
        job: &PersistentCloudJob,
    ) -> Result<PathBuf, CloudProviderError> {
        let container = job
            .artifact_descriptor
            .as_ref()
            .map(|d| d.container)
            .unwrap_or(ArtifactContainer::Mp4);
        self.artifact_final_path_for_container(&job.project_id, &job.internal_job_id, container)
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
    // Atomic Manifest Persistence with Store-Level CAS Revision Protection
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

        // 1. CAS guard against existing primary manifest:
        // If an existing valid primary record exists on disk, incoming revision must be strictly newer.
        if primary_path.exists() {
            if let Ok(content) = fs::read_to_string(&primary_path) {
                if let Ok(existing_job) = serde_json::from_str::<PersistentCloudJob>(&content) {
                    if job.state_revision <= existing_job.state_revision {
                        return Err(CloudProviderError::RequestInvalid(format!(
                            "STALE_JOB_REVISION: Cannot overwrite existing job {} with stale revision (disk revision {}, incoming revision {})",
                            job.internal_job_id, existing_job.state_revision, job.state_revision
                        )));
                    }
                }
            }
        }

        // 2. CAS guard against existing valid .tmp manifest:
        // If an existing valid temp record exists on disk, incoming revision must be strictly newer.
        if tmp_path.exists() {
            if let Ok(content) = fs::read_to_string(&tmp_path) {
                if let Ok(tmp_job) = serde_json::from_str::<PersistentCloudJob>(&content) {
                    if job.state_revision <= tmp_job.state_revision {
                        return Err(CloudProviderError::RequestInvalid(format!(
                            "STALE_JOB_REVISION: Temp file for job {} has equal or newer revision ({}) than incoming ({})",
                            job.internal_job_id, tmp_job.state_revision, job.state_revision
                        )));
                    }
                }
            }
        }

        let serialized = serde_json::to_string_pretty(job).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to serialize PersistentCloudJob: {}",
                e
            ))
        })?;

        // 3. Write to temporary file with sync
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

        // 4. Windows-safe atomic replace
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

        let mut job = match (primary_res, tmp_res) {
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
                    tmp_job
                } else {
                    // Primary is same or newer -> clean up stale temp
                    let _ = fs::remove_file(&tmp_path);
                    primary_job
                }
            }

            // Case 2: Primary valid, temp missing or corrupt -> use primary
            (Ok(primary_job), Err(_)) => {
                if tmp_path.exists() {
                    let _ = fs::remove_file(&tmp_path);
                }
                primary_job
            }

            // Case 3: Primary missing, temp valid -> promote temp
            (Err(e), Ok(tmp_job)) if e.contains("Primary file missing") => {
                atomic_replace(&tmp_path, &primary_path).map_err(|e| {
                    CloudProviderError::ProviderUnavailable(format!(
                        "Failed to promote recovered tmp file to primary: {}",
                        e
                    ))
                })?;
                tmp_job
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
                tmp_job
            }

            // Case 5: Both missing or corrupt -> recovery failure (Fail-Closed)
            (Err(e1), Err(e2)) => {
                return Err(CloudProviderError::ProviderUnavailable(format!(
                    "RECOVERY_FAILED: Primary corrupt/missing ({}) and temp corrupt/missing ({}) for job {}",
                    e1, e2, internal_job_id
                )));
            }
        };

        job.normalize_in_memory();
        Ok(job)
    }

    // -------------------------------------------------------------------------
    // Querying & Listing (Fail-Closed on Corrupt Manifests)
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
                // Fail closed if any manifest in the project is unrecoverable / corrupt
                let job = self.load_job(project_id, &file_stem)?;
                jobs.push(job);
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
        let entries = fs::read_dir(projects_dir).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to read projects directory {}: {}",
                projects_dir.display(),
                e
            ))
        })?;

        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let project_id = entry.file_name().to_string_lossy().to_string();
                // Fail-closed: propagate error if any project has an unrecoverable manifest
                let jobs = self.list_jobs_in_project(&project_id)?;
                for job in jobs {
                    if !job.state.is_terminal() {
                        active_jobs.push(job);
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

// -----------------------------------------------------------------------------
// SegmentedCloudJobStore
// -----------------------------------------------------------------------------

#[derive(Clone)]
pub struct SegmentedCloudJobStore {
    pub storage_paths: StoragePaths,
}

impl SegmentedCloudJobStore {
    pub fn new(storage_paths: StoragePaths) -> Self {
        Self { storage_paths }
    }

    pub fn project_segmented_dir(&self, project_id: &str) -> Result<PathBuf, CloudProviderError> {
        validate_identifier(project_id, "projectId")?;
        let dir = self
            .storage_paths
            .projects_dir
            .join(project_id)
            .join("cloud-jobs")
            .join("segmented");
        Ok(dir)
    }

    pub fn parent_job_dir(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<PathBuf, CloudProviderError> {
        validate_identifier(parent_id, "parentId")?;
        let dir = self.project_segmented_dir(project_id)?.join(parent_id);
        Ok(dir)
    }

    pub fn manifest_file_path(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<PathBuf, CloudProviderError> {
        let parent_dir = self.parent_job_dir(project_id, parent_id)?;
        Ok(parent_dir.join("manifest.json"))
    }

    pub fn manifest_tmp_file_path(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<PathBuf, CloudProviderError> {
        let parent_dir = self.parent_job_dir(project_id, parent_id)?;
        Ok(parent_dir.join("manifest.json.tmp"))
    }

    pub fn save_manifest_atomic(
        &self,
        manifest: &super::manifest::SegmentedCloudJobManifest,
    ) -> Result<(), CloudProviderError> {
        let parent_dir = self.parent_job_dir(&manifest.project_id, &manifest.parent_id)?;
        fs::create_dir_all(&parent_dir).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to create segmented job dir {}: {}",
                parent_dir.display(),
                e
            ))
        })?;

        let primary_path = self.manifest_file_path(&manifest.project_id, &manifest.parent_id)?;
        let tmp_path = self.manifest_tmp_file_path(&manifest.project_id, &manifest.parent_id)?;

        let serialized = serde_json::to_string_pretty(manifest).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to serialize segmented job manifest: {}",
                e
            ))
        })?;

        let mut file = File::create(&tmp_path).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to create tmp manifest file {}: {}",
                tmp_path.display(),
                e
            ))
        })?;

        file.write_all(serialized.as_bytes()).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to write tmp manifest file {}: {}",
                tmp_path.display(),
                e
            ))
        })?;

        file.sync_all().map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to flush tmp manifest file {}: {}",
                tmp_path.display(),
                e
            ))
        })?;

        drop(file);

        atomic_replace(&tmp_path, &primary_path).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Atomic rename failed from {} to {}: {}",
                tmp_path.display(),
                primary_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    pub fn load_manifest(
        &self,
        project_id: &str,
        parent_id: &str,
    ) -> Result<super::manifest::SegmentedCloudJobManifest, CloudProviderError> {
        let primary_path = self.manifest_file_path(project_id, parent_id)?;
        let tmp_path = self.manifest_tmp_file_path(project_id, parent_id)?;

        fn read_and_parse(
            p: &Path,
            is_primary: bool,
        ) -> Result<super::manifest::SegmentedCloudJobManifest, String> {
            if !p.exists() {
                return Err(if is_primary {
                    "Primary manifest file missing".to_string()
                } else {
                    "Temp manifest file missing".to_string()
                });
            }
            let data = fs::read_to_string(p).map_err(|e| format!("Read error: {}", e))?;
            serde_json::from_str(&data).map_err(|e| format!("Parse error: {}", e))
        }

        let primary_res = read_and_parse(&primary_path, true);
        let tmp_res = read_and_parse(&tmp_path, false);

        let manifest = match (primary_res, tmp_res) {
            // Case 1: Both primary and tmp valid -> compare state_revision (CAS)
            (Ok(p_manifest), Ok(tmp_manifest)) => {
                if tmp_manifest.state_revision > p_manifest.state_revision {
                    // Newer state was in temp before crash -> promote temp!
                    atomic_replace(&tmp_path, &primary_path).map_err(|e| {
                        CloudProviderError::ProviderUnavailable(format!(
                            "Failed to promote recovered tmp manifest to primary: {}",
                            e
                        ))
                    })?;
                    tmp_manifest
                } else {
                    // Primary is same or newer -> clean up stale temp
                    let _ = fs::remove_file(&tmp_path);
                    p_manifest
                }
            }

            // Case 2: Primary valid, temp missing or corrupt -> use primary
            (Ok(p_manifest), Err(_)) => {
                if tmp_path.exists() {
                    let _ = fs::remove_file(&tmp_path);
                }
                p_manifest
            }

            // Case 3: Primary missing, temp valid -> promote temp
            (Err(e), Ok(tmp_manifest)) if e.contains("Primary manifest file missing") => {
                atomic_replace(&tmp_path, &primary_path).map_err(|e| {
                    CloudProviderError::ProviderUnavailable(format!(
                        "Failed to promote recovered tmp manifest to primary: {}",
                        e
                    ))
                })?;
                tmp_manifest
            }

            // Case 4: Primary corrupt, temp valid -> backup corrupt primary and promote temp
            (Err(_), Ok(tmp_manifest)) => {
                let corrupt_path = primary_path.with_extension("json.corrupt");
                let _ = atomic_replace(&primary_path, &corrupt_path);
                atomic_replace(&tmp_path, &primary_path).map_err(|e| {
                    CloudProviderError::ProviderUnavailable(format!(
                        "Failed to promote recovered tmp manifest over corrupt primary: {}",
                        e
                    ))
                })?;
                tmp_manifest
            }

            // Case 5: Both missing or corrupt -> recovery failure (Fail-Closed)
            (Err(e1), Err(e2)) => {
                return Err(CloudProviderError::ProviderUnavailable(format!(
                    "RECOVERY_FAILED: Segmented job manifest primary corrupt/missing ({}) and tmp corrupt/missing ({}) for {}",
                    e1, e2, parent_id
                )));
            }
        };

        Ok(manifest)
    }

    pub fn list_segmented_jobs(
        &self,
        project_id: &str,
    ) -> Result<Vec<super::manifest::SegmentedCloudJobManifest>, CloudProviderError> {
        let seg_dir = self.project_segmented_dir(project_id)?;
        if !seg_dir.exists() {
            return Ok(Vec::new());
        }

        let mut manifests = Vec::new();
        let entries = fs::read_dir(&seg_dir).map_err(|e| {
            CloudProviderError::ProviderUnavailable(format!(
                "Failed to read segmented jobs directory {}: {}",
                seg_dir.display(),
                e
            ))
        })?;

        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let parent_id = entry.file_name().to_string_lossy().to_string();
                if let Ok(manifest) = self.load_manifest(project_id, &parent_id) {
                    manifests.push(manifest);
                }
            }
        }

        manifests.sort_by(|a, b| b.timestamps.created_at.cmp(&a.timestamps.created_at));
        Ok(manifests)
    }

    pub fn find_parent_by_client_request_id(
        &self,
        project_id: &str,
        client_request_id: &str,
    ) -> Result<Option<super::manifest::SegmentedCloudJobManifest>, CloudProviderError> {
        let list = self.list_segmented_jobs(project_id)?;
        for m in list {
            if m.client_request_id == client_request_id || m.parent_id == client_request_id {
                return Ok(Some(m));
            }
        }
        Ok(None)
    }
}
