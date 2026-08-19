use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use crate::ai::manifest::{AiModelManifest, ModelFormat, ModelRequirements};
use crate::ai::profile::AiModelProfile;
use crate::ai::provider::ExecutionProvider;
use crate::error::AppError;

/// Strict Semantic Version representation (MAJOR.MINOR.PATCH).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parses a semantic version string (e.g. "1.2.0", "v1.2.0", or legacy "1.0").
    pub fn parse(s: &str) -> Result<Self, AppError> {
        let trimmed = s.trim().trim_start_matches('v').trim_start_matches('V');
        let parts: Vec<&str> = trimmed.split('.').collect();
        if parts.is_empty() || parts.len() > 3 {
            return Err(AppError::invalid_input(format!(
                "Invalid semantic version format '{}'. Expected format 'MAJOR.MINOR.PATCH' (e.g. 1.0.0)",
                s
            )));
        }

        let major = parts[0].parse::<u32>().map_err(|_| {
            AppError::invalid_input(format!("Invalid major version '{}' in '{}'", parts[0], s))
        })?;
        let minor = if parts.len() > 1 {
            parts[1].parse::<u32>().map_err(|_| {
                AppError::invalid_input(format!("Invalid minor version '{}' in '{}'", parts[1], s))
            })?
        } else {
            0
        };
        let patch = if parts.len() > 2 {
            parts[2].parse::<u32>().map_err(|_| {
                AppError::invalid_input(format!("Invalid patch version '{}' in '{}'", parts[2], s))
            })?
        } else {
            0
        };

        Ok(Self {
            major,
            minor,
            patch,
        })
    }

    pub fn to_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Validates that a model ID contains only safe alphanumeric, dash, and underscore characters.
/// Rejects path traversal (`..`, `/`, `\`) and empty strings.
pub fn validate_model_id(id: &str) -> Result<(), AppError> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid_input("Model ID cannot be empty"));
    }

    if trimmed.contains("..")
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains(':')
    {
        return Err(AppError::invalid_input(format!(
            "Model ID '{}' contains forbidden path traversal characters",
            id
        )));
    }

    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::invalid_input(format!(
            "Model ID '{}' contains invalid characters. Only alphanumeric, '-', and '_' are allowed",
            id
        )));
    }

    Ok(())
}

/// Validates that a version string is a valid SemVer format.
pub fn validate_version_str(version: &str) -> Result<SemVer, AppError> {
    SemVer::parse(version)
}

/// Calculates the authoritative SHA-256 hexadecimal checksum of a local file in 64KB chunks.
pub fn calculate_file_sha256(path: &Path) -> Result<String, AppError> {
    if !path.exists() {
        return Err(AppError::file_not_found(path.display().to_string()));
    }

    let file = File::open(path).map_err(|e| {
        AppError::storage_error(
            format!(
                "Failed to open file for SHA-256 checksum: {}",
                path.display()
            ),
            e.to_string(),
        )
    })?;

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let count = reader.read(&mut buffer).map_err(|e| {
            AppError::storage_error(
                format!(
                    "Failed to read file during SHA-256 calculation: {}",
                    path.display()
                ),
                e.to_string(),
            )
        })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Self-describing Production AI Model Package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiModelPackage {
    pub model_id: String,
    pub model_name: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub model_format: ModelFormat,
    pub model_file: PathBuf,
    pub file_size_bytes: u64,
    pub sha256: String,
    pub manifest: AiModelManifest,
    pub profile: AiModelProfile,
    pub requirements: ModelRequirements,
    pub supported_providers: Vec<ExecutionProvider>,
    #[serde(default)]
    pub is_production: bool,
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub package_schema_version: u32,
}

impl AiModelPackage {
    pub fn new(
        model_id: impl Into<String>,
        model_name: impl Into<String>,
        version: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        model_format: ModelFormat,
        model_file: PathBuf,
        file_size_bytes: u64,
        sha256: impl Into<String>,
        manifest: AiModelManifest,
        profile: AiModelProfile,
        requirements: ModelRequirements,
        supported_providers: Vec<ExecutionProvider>,
    ) -> Result<Self, AppError> {
        let id_str = model_id.into();
        let ver_str = version.into();
        validate_model_id(&id_str)?;
        let semver = validate_version_str(&ver_str)?;

        Ok(Self {
            model_id: id_str,
            model_name: model_name.into(),
            version: semver.to_string(),
            display_name: display_name.into(),
            description: description.into(),
            model_format,
            model_file,
            file_size_bytes,
            sha256: sha256.into(),
            manifest,
            profile,
            requirements,
            supported_providers,
            is_production: false,
            metadata: serde_json::json!({}),
            created_at: Utc::now().to_rfc3339(),
            package_schema_version: 1,
        })
    }

    pub fn with_production(mut self, is_production: bool) -> Self {
        self.is_production = is_production;
        self.manifest.is_production = is_production;
        self
    }

    /// Verifies the physical file integrity on disk against the recorded SHA-256 hash.
    pub fn verify_integrity(&self) -> Result<(), AppError> {
        if !self.model_file.exists() {
            return Err(AppError::file_not_found(
                self.model_file.display().to_string(),
            ));
        }

        let actual_size = std::fs::metadata(&self.model_file)
            .map(|m| m.len())
            .unwrap_or(0);
        if actual_size == 0 {
            return Err(AppError::model_integrity_mismatch(
                format!(
                    "Model file is empty (0 bytes): {}",
                    self.model_file.display()
                ),
                "File size must be greater than 0",
            ));
        }

        let actual_hash = calculate_file_sha256(&self.model_file)?;
        if !actual_hash.eq_ignore_ascii_case(&self.sha256) {
            return Err(AppError::model_integrity_mismatch(
                format!(
                    "Model SHA-256 integrity mismatch for '{}' (v{}): expected {}, got {}",
                    self.model_id, self.version, self.sha256, actual_hash
                ),
                self.model_file.display().to_string(),
            ));
        }

        Ok(())
    }
}

/// Model Family containing multiple semantic versions with active/rollback state tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiModelFamily {
    pub model_id: String,
    pub name: String,
    pub active_version: Option<String>,
    pub previous_version: Option<String>,
    pub versions: HashMap<String, AiModelPackage>,
    pub created_at: String,
    pub updated_at: String,
}

impl AiModelFamily {
    pub fn new(model_id: impl Into<String>, name: impl Into<String>) -> Result<Self, AppError> {
        let id = model_id.into();
        validate_model_id(&id)?;
        let now = Utc::now().to_rfc3339();

        Ok(Self {
            model_id: id,
            name: name.into(),
            active_version: None,
            previous_version: None,
            versions: HashMap::new(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    /// Adds a new model package version. Rejects duplicate versions.
    pub fn add_version(&mut self, package: AiModelPackage) -> Result<(), AppError> {
        if self.versions.contains_key(&package.version) {
            return Err(AppError::model_version_exists(
                &self.model_id,
                &package.version,
            ));
        }

        // If no active version exists, set this as active
        if self.active_version.is_none() {
            self.active_version = Some(package.version.clone());
        }

        self.versions.insert(package.version.clone(), package);
        self.updated_at = Utc::now().to_rfc3339();
        Ok(())
    }

    /// Activates a specific version, recording the previous version for rollback safety.
    pub fn activate_version(&mut self, version: &str) -> Result<&AiModelPackage, AppError> {
        let semver = validate_version_str(version)?;
        let v_str = semver.to_string();

        if !self.versions.contains_key(&v_str) {
            return Err(AppError::model_not_available(
                format!("{}:{}", self.model_id, v_str),
                "Specified version is not registered in model family",
            ));
        }

        // If already active, return ok
        if self.active_version.as_deref() == Some(&v_str) {
            return Ok(self.versions.get(&v_str).unwrap());
        }

        self.previous_version = self.active_version.take();
        self.active_version = Some(v_str.clone());
        self.updated_at = Utc::now().to_rfc3339();

        Ok(self.versions.get(&v_str).unwrap())
    }

    /// Rolls back to the previous active version.
    pub fn rollback(&mut self) -> Result<&AiModelPackage, AppError> {
        let prev = self.previous_version.clone().ok_or_else(|| {
            AppError::invalid_input(format!(
                "No previous version available for rollback on model '{}'",
                self.model_id
            ))
        })?;

        if !self.versions.contains_key(&prev) {
            return Err(AppError::model_not_available(
                format!("{}:{}", self.model_id, prev),
                "Previous rollback version is no longer installed",
            ));
        }

        // Swap active and previous
        let current = self.active_version.take();
        self.active_version = Some(prev.clone());
        self.previous_version = current;
        self.updated_at = Utc::now().to_rfc3339();

        Ok(self.versions.get(&prev).unwrap())
    }

    /// Removes a specific version. Rejects removing the currently active version if other versions exist.
    pub fn remove_version(&mut self, version: &str) -> Result<AiModelPackage, AppError> {
        let semver = validate_version_str(version)?;
        let v_str = semver.to_string();

        if !self.versions.contains_key(&v_str) {
            return Err(AppError::model_not_available(
                format!("{}:{}", self.model_id, v_str),
                "Version not found in model family",
            ));
        }

        if self.active_version.as_deref() == Some(&v_str) && self.versions.len() > 1 {
            return Err(AppError::invalid_input(format!(
                "Cannot remove active version '{}' of model '{}'. Activate a different version first.",
                v_str, self.model_id
            )));
        }

        if self.active_version.as_deref() == Some(&v_str) {
            self.active_version = None;
        }
        if self.previous_version.as_deref() == Some(&v_str) {
            self.previous_version = None;
        }

        self.updated_at = Utc::now().to_rfc3339();
        Ok(self.versions.remove(&v_str).unwrap())
    }

    /// Gets the currently active package.
    pub fn active_package(&self) -> Option<&AiModelPackage> {
        self.active_version
            .as_ref()
            .and_then(|v| self.versions.get(v))
    }

    /// Lists installed versions sorted by SemVer descending.
    pub fn sorted_versions(&self) -> Vec<&AiModelPackage> {
        let mut list: Vec<&AiModelPackage> = self.versions.values().collect();
        list.sort_by(|a, b| {
            let va = SemVer::parse(&a.version).unwrap_or_else(|_| SemVer::new(0, 0, 0));
            let vb = SemVer::parse(&b.version).unwrap_or_else(|_| SemVer::new(0, 0, 0));
            vb.cmp(&va) // Descending
        });
        list
    }
}
