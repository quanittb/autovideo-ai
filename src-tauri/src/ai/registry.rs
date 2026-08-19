use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::ai::manifest::{AiModelManifest, ModelFormat, ModelRequirements};
use crate::ai::package::{
    calculate_file_sha256, validate_model_id, validate_version_str, AiModelFamily, AiModelPackage,
};
use crate::ai::profile::AiModelProfile;
use crate::ai::provider::ExecutionProvider;
use crate::ai::validation::{validate_model_package_deep, ModelValidationReport};
use crate::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RegistryData {
    #[serde(default)]
    models: HashMap<String, AiModelManifest>,
    #[serde(default)]
    families: HashMap<String, AiModelFamily>,
}

/// Thread-safe and persistent local Model Registry supporting model families, semantic versions, and profiles.
#[derive(Debug, Clone)]
pub struct ModelRegistry {
    models_dir: PathBuf,
}

impl ModelRegistry {
    pub fn new(models_dir: PathBuf) -> Self {
        if !models_dir.exists() {
            let _ = fs::create_dir_all(&models_dir);
        }
        Self { models_dir }
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    fn registry_file_path(&self) -> PathBuf {
        self.models_dir.join("registry.json")
    }

    fn load_registry_data(&self) -> Result<RegistryData, AppError> {
        let path = self.registry_file_path();
        if !path.exists() {
            return Ok(RegistryData::default());
        }
        let content = fs::read_to_string(&path)
            .map_err(|e| AppError::storage_write_failed(path.to_string_lossy(), e.to_string()))?;
        let data: RegistryData = serde_json::from_str(&content)
            .map_err(|e| AppError::storage_write_failed(path.to_string_lossy(), e.to_string()))?;
        Ok(data)
    }

    fn save_registry_atomic(&self, data: &RegistryData) -> Result<(), AppError> {
        if !self.models_dir.exists() {
            fs::create_dir_all(&self.models_dir).map_err(|e| {
                AppError::storage_write_failed(self.models_dir.to_string_lossy(), e.to_string())
            })?;
        }

        let target_path = self.registry_file_path();
        let temp_path = self
            .models_dir
            .join(format!("registry.json.tmp.{}", Uuid::new_v4()));

        let json_bytes = serde_json::to_vec_pretty(data).map_err(|e| {
            AppError::storage_write_failed(target_path.to_string_lossy(), e.to_string())
        })?;

        let mut file = File::create(&temp_path).map_err(|e| {
            AppError::storage_write_failed(temp_path.to_string_lossy(), e.to_string())
        })?;

        file.write_all(&json_bytes).map_err(|e| {
            AppError::storage_write_failed(temp_path.to_string_lossy(), e.to_string())
        })?;

        file.flush().map_err(|e| {
            AppError::storage_write_failed(temp_path.to_string_lossy(), e.to_string())
        })?;

        file.sync_all().map_err(|e| {
            AppError::storage_write_failed(temp_path.to_string_lossy(), e.to_string())
        })?;

        drop(file);

        #[cfg(target_os = "windows")]
        if target_path.exists() {
            let _ = fs::remove_file(&target_path);
        }

        fs::rename(&temp_path, &target_path).map_err(|e| {
            AppError::storage_write_failed(target_path.to_string_lossy(), e.to_string())
        })?;

        Ok(())
    }

    /// Resolves the managed storage directory for a specific model version.
    pub fn version_dir(&self, model_id: &str, version: &str) -> Result<PathBuf, AppError> {
        validate_model_id(model_id)?;
        let semver = validate_version_str(version)?;
        Ok(self
            .models_dir
            .join(model_id)
            .join("versions")
            .join(semver.to_string()))
    }

    /// Validates model files and metadata before registration.
    pub fn validate_manifest(&self, manifest: &AiModelManifest) -> Result<(), AppError> {
        validate_model_id(&manifest.id)?;
        if manifest.name.trim().is_empty() {
            return Err(AppError::invalid_input("Model name cannot be empty"));
        }
        if !manifest.path.exists() {
            return Err(AppError::file_not_found(manifest.path.to_string_lossy()));
        }

        let metadata = fs::metadata(&manifest.path).map_err(|e| {
            AppError::invalid_input(format!("Cannot inspect model file metadata: {}", e))
        })?;

        if metadata.len() == 0 {
            return Err(AppError::invalid_input(format!(
                "Model file is empty (0 bytes): {}",
                manifest.path.display()
            )));
        }

        match manifest.format {
            ModelFormat::Onnx => {
                let ext = manifest
                    .path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if !ext.eq_ignore_ascii_case("onnx") {
                    return Err(AppError::invalid_input(format!(
                        "Unsupported model extension '{}'. Expected .onnx extension for ONNX model format",
                        ext
                    )));
                }
            }
        }

        Ok(())
    }

    // =========================================================================
    // PHASE 6F PRODUCTION MODEL PACKAGE & VERSION MANAGEMENT
    // =========================================================================

    /// Registers a complete AiModelPackage in the registry and sets it active if first version.
    pub fn register_package(&self, package: AiModelPackage) -> Result<AiModelPackage, AppError> {
        validate_model_id(&package.model_id)?;
        let _ = validate_version_str(&package.version)?;

        // Verify SHA-256 integrity
        package.verify_integrity()?;

        let mut data = self.load_registry_data()?;

        let family = data
            .families
            .entry(package.model_id.clone())
            .or_insert_with(|| {
                AiModelFamily::new(&package.model_id, &package.model_name).unwrap_or_else(|_| {
                    AiModelFamily {
                        model_id: package.model_id.clone(),
                        name: package.model_name.clone(),
                        active_version: None,
                        previous_version: None,
                        versions: HashMap::new(),
                        created_at: package.created_at.clone(),
                        updated_at: package.created_at.clone(),
                    }
                })
            });

        family.add_version(package.clone())?;

        // Synchronize legacy `models` map with the active version for backwards compatibility
        if let Some(active_pkg) = family.active_package() {
            data.models
                .insert(package.model_id.clone(), active_pkg.manifest.clone());
        }

        // Save version artifacts in managed directory
        let v_dir = self.version_dir(&package.model_id, &package.version)?;
        if !v_dir.exists() {
            let _ = fs::create_dir_all(&v_dir);
        }
        let _ = fs::write(
            v_dir.join("package.json"),
            serde_json::to_vec_pretty(&package).unwrap_or_default(),
        );
        let _ = fs::write(
            v_dir.join("manifest.json"),
            serde_json::to_vec_pretty(&package.manifest).unwrap_or_default(),
        );
        let _ = fs::write(
            v_dir.join("profile.json"),
            serde_json::to_vec_pretty(&package.profile).unwrap_or_default(),
        );

        self.save_registry_atomic(&data)?;
        Ok(package)
    }

    /// Imports an external .onnx file into managed storage, extracts metadata, verifies profile, and registers package.
    pub fn import_model(
        &self,
        source_path: &Path,
        model_id: &str,
        model_name: &str,
        version: &str,
        display_name: &str,
        description: &str,
        profile: AiModelProfile,
        requirements: ModelRequirements,
        supported_providers: Vec<ExecutionProvider>,
    ) -> Result<AiModelPackage, AppError> {
        validate_model_id(model_id)?;
        let semver = validate_version_str(version)?;

        if !source_path.exists() {
            return Err(AppError::file_not_found(source_path.display().to_string()));
        }

        let ext = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if !ext.eq_ignore_ascii_case("onnx") {
            return Err(AppError::invalid_input(format!(
                "Invalid model file extension '{}'. Only .onnx models are supported for import",
                ext
            )));
        }

        let file_size = fs::metadata(source_path)
            .map(|m| m.len())
            .map_err(|e| AppError::storage_error("Failed to read model metadata", e.to_string()))?;
        if file_size == 0 {
            return Err(AppError::invalid_input("Model file is empty (0 bytes)"));
        }

        // Calculate real SHA-256
        let sha256 = calculate_file_sha256(source_path)?;

        // Deep ONNX inspection
        let onnx_meta = crate::ai::onnx::OnnxAiRuntime::inspect_onnx_file(source_path)?;

        // Validate profile against real ONNX graph
        if let Err(errs) =
            crate::ai::validation::validate_profile_against_onnx(&profile, &onnx_meta)
        {
            return Err(AppError::model_profile_mismatch(
                "Imported model profile is incompatible with ONNX graph",
                errs.join("; "),
            ));
        }

        // Prepare managed directory
        let v_dir = self.version_dir(model_id, version)?;
        fs::create_dir_all(&v_dir).map_err(|e| {
            AppError::storage_error("Failed to create model version directory", e.to_string())
        })?;

        let managed_model_path = v_dir.join("model.onnx");
        fs::copy(source_path, &managed_model_path).map_err(|e| {
            AppError::storage_error(
                "Failed to copy model file to managed storage",
                e.to_string(),
            )
        })?;

        // Create manifest
        let manifest = AiModelManifest::new(
            model_id,
            model_name,
            semver.to_string(),
            ModelFormat::Onnx,
            managed_model_path.clone(),
            description,
            vec![],
            vec![],
            requirements.clone(),
        );

        let package = AiModelPackage::new(
            model_id,
            model_name,
            version,
            display_name,
            description,
            ModelFormat::Onnx,
            managed_model_path,
            file_size,
            sha256,
            manifest,
            profile,
            requirements,
            supported_providers,
        )?;

        self.register_package(package)
    }

    /// Activates a specific version of a model family atomically.
    pub fn activate_version(
        &self,
        model_id: &str,
        version: &str,
    ) -> Result<AiModelPackage, AppError> {
        validate_model_id(model_id)?;
        let semver = validate_version_str(version)?;
        let v_str = semver.to_string();

        let mut data = self.load_registry_data()?;
        let family = data.families.get_mut(model_id).ok_or_else(|| {
            AppError::model_not_available(model_id, "Model family not found in registry")
        })?;

        let target_pkg = family
            .versions
            .get(&v_str)
            .ok_or_else(|| {
                AppError::model_not_available(
                    format!("{}:{}", model_id, v_str),
                    "Version not found in model family",
                )
            })?
            .clone();

        // Deep validation of target package before switching
        let report = validate_model_package_deep(&target_pkg)?;
        if !report.valid {
            return Err(AppError::model_validation_failed(
                format!(
                    "Cannot activate invalid model version '{}' for model '{}'",
                    v_str, model_id
                ),
                report.errors.join("; "),
            ));
        }

        let _ = family.activate_version(&v_str)?;

        // Synchronize legacy `models` entry
        data.models
            .insert(model_id.to_string(), target_pkg.manifest.clone());

        // Update active.json in model family directory
        let active_file = self.models_dir.join(model_id).join("active.json");
        let _ = fs::write(
            active_file,
            serde_json::to_vec_pretty(&target_pkg).unwrap_or_default(),
        );

        self.save_registry_atomic(&data)?;
        Ok(target_pkg)
    }

    /// Rolls back the model family to its previously active version.
    pub fn rollback_model(&self, model_id: &str) -> Result<AiModelPackage, AppError> {
        validate_model_id(model_id)?;

        let mut data = self.load_registry_data()?;
        let family = data.families.get_mut(model_id).ok_or_else(|| {
            AppError::model_not_available(model_id, "Model family not found in registry")
        })?;

        let rolled_back_pkg = family.rollback()?.clone();

        // Validate package integrity
        rolled_back_pkg.verify_integrity()?;

        // Synchronize legacy `models` entry
        data.models
            .insert(model_id.to_string(), rolled_back_pkg.manifest.clone());

        let active_file = self.models_dir.join(model_id).join("active.json");
        let _ = fs::write(
            active_file,
            serde_json::to_vec_pretty(&rolled_back_pkg).unwrap_or_default(),
        );

        self.save_registry_atomic(&data)?;
        Ok(rolled_back_pkg)
    }

    /// Removes a specific version from a model family.
    pub fn remove_version(
        &self,
        model_id: &str,
        version: &str,
    ) -> Result<AiModelPackage, AppError> {
        validate_model_id(model_id)?;
        let semver = validate_version_str(version)?;
        let v_str = semver.to_string();

        let mut data = self.load_registry_data()?;
        let family = data.families.get_mut(model_id).ok_or_else(|| {
            AppError::model_not_available(model_id, "Model family not found in registry")
        })?;

        let removed = family.remove_version(&v_str)?;

        // If family is now completely empty, remove family
        if family.versions.is_empty() {
            data.families.remove(model_id);
            data.models.remove(model_id);
            let family_dir = self.models_dir.join(model_id);
            if family_dir.exists() {
                let _ = fs::remove_dir_all(&family_dir);
            }
        } else if let Some(active_pkg) = family.active_package() {
            data.models
                .insert(model_id.to_string(), active_pkg.manifest.clone());
        } else {
            data.models.remove(model_id);
        }

        // Delete version directory
        let v_dir = self.version_dir(model_id, &v_str)?;
        if v_dir.exists() {
            let _ = fs::remove_dir_all(&v_dir);
        }

        self.save_registry_atomic(&data)?;
        Ok(removed)
    }

    /// Runs deep validation on a specific model package version.
    pub fn validate_package(
        &self,
        model_id: &str,
        version: &str,
    ) -> Result<ModelValidationReport, AppError> {
        let pkg = self.get_package(model_id, version)?;
        validate_model_package_deep(&pkg)
    }

    /// Retrieves an exact package version.
    pub fn get_package(&self, model_id: &str, version: &str) -> Result<AiModelPackage, AppError> {
        validate_model_id(model_id)?;
        let semver = validate_version_str(version)?;
        let v_str = semver.to_string();

        let data = self.load_registry_data()?;
        let family = data.families.get(model_id).ok_or_else(|| {
            AppError::model_not_available(model_id, "Model family not found in registry")
        })?;

        family.versions.get(&v_str).cloned().ok_or_else(|| {
            AppError::model_not_available(
                format!("{}:{}", model_id, v_str),
                "Specified version not found in model family",
            )
        })
    }

    /// Retrieves the currently active package for a model family.
    pub fn get_active_package(&self, model_id: &str) -> Result<AiModelPackage, AppError> {
        validate_model_id(model_id)?;
        let data = self.load_registry_data()?;

        if let Some(family) = data.families.get(model_id) {
            if let Some(active) = family.active_package() {
                return Ok(active.clone());
            }
        }

        // Fallback to legacy models map
        if let Some(manifest) = data.models.get(model_id) {
            let sha256 = calculate_file_sha256(&manifest.path).unwrap_or_default();
            let size = fs::metadata(&manifest.path).map(|m| m.len()).unwrap_or(0);
            return AiModelPackage::new(
                &manifest.id,
                &manifest.name,
                &manifest.version,
                &manifest.name,
                &manifest.description,
                manifest.format,
                manifest.path.clone(),
                size,
                sha256,
                manifest.clone(),
                AiModelProfile::default(),
                manifest.requirements.clone(),
                vec![ExecutionProvider::Cpu, ExecutionProvider::DirectML],
            );
        }

        Err(AppError::model_not_available(
            model_id,
            "No active model package found in registry",
        ))
    }

    /// Lists all registered model families.
    pub fn list_families(&self) -> Result<Vec<AiModelFamily>, AppError> {
        let data = self.load_registry_data()?;
        let mut list: Vec<AiModelFamily> = data.families.into_values().collect();
        list.sort_by(|a, b| a.model_id.cmp(&b.model_id));
        Ok(list)
    }

    /// Lists all installed model packages across all families.
    pub fn list_packages(&self) -> Result<Vec<AiModelPackage>, AppError> {
        let data = self.load_registry_data()?;
        let mut list = Vec::new();
        for family in data.families.values() {
            for pkg in family.versions.values() {
                list.push(pkg.clone());
            }
        }
        list.sort_by(|a, b| {
            a.model_id
                .cmp(&b.model_id)
                .then_with(|| a.version.cmp(&b.version))
        });
        Ok(list)
    }

    // =========================================================================
    // LEGACY MANIFEST REGISTRATION (BACKWARD COMPATIBILITY)
    // =========================================================================

    /// Registers a new AI Model in the registry. Rejects duplicates.
    pub fn register_model(&self, manifest: AiModelManifest) -> Result<AiModelManifest, AppError> {
        self.validate_manifest(&manifest)?;

        let mut data = self.load_registry_data()?;
        if data.models.contains_key(&manifest.id) {
            return Err(AppError::invalid_input(format!(
                "Model ID '{}' is already registered. Duplicate IDs are not allowed.",
                manifest.id
            )));
        }

        // Also create model family and package entry
        let sha256 = calculate_file_sha256(&manifest.path).unwrap_or_default();
        let size = fs::metadata(&manifest.path).map(|m| m.len()).unwrap_or(0);
        let version = if manifest.version.trim().is_empty() {
            "1.0.0".to_string()
        } else {
            manifest.version.clone()
        };

        let pkg = AiModelPackage::new(
            &manifest.id,
            &manifest.name,
            &version,
            &manifest.name,
            &manifest.description,
            manifest.format,
            manifest.path.clone(),
            size,
            sha256,
            manifest.clone(),
            AiModelProfile::default(),
            manifest.requirements.clone(),
            vec![ExecutionProvider::Cpu, ExecutionProvider::DirectML],
        )?;

        let family = data.families.entry(manifest.id.clone()).or_insert_with(|| {
            AiModelFamily::new(&manifest.id, &manifest.name).unwrap_or_else(|_| AiModelFamily {
                model_id: manifest.id.clone(),
                name: manifest.name.clone(),
                active_version: Some(version.clone()),
                previous_version: None,
                versions: HashMap::new(),
                created_at: manifest.created_at.clone(),
                updated_at: manifest.updated_at.clone(),
            })
        });

        let _ = family.add_version(pkg);

        // Save model's dedicated manifest directory
        let model_dir = self.models_dir.join(&manifest.id);
        if !model_dir.exists() {
            let _ = fs::create_dir_all(&model_dir);
        }
        let manifest_file = model_dir.join("manifest.json");
        if let Ok(bytes) = serde_json::to_vec_pretty(&manifest) {
            let _ = fs::write(&manifest_file, bytes);
        }

        data.models.insert(manifest.id.clone(), manifest.clone());
        self.save_registry_atomic(&data)?;

        Ok(manifest)
    }

    /// Unregisters an AI Model by ID.
    pub fn unregister_model(&self, model_id: &str) -> Result<(), AppError> {
        let mut data = self.load_registry_data()?;
        if !data.models.contains_key(model_id) && !data.families.contains_key(model_id) {
            return Err(AppError::model_not_available(
                model_id,
                "Model ID not found in registry",
            ));
        }

        data.models.remove(model_id);
        data.families.remove(model_id);
        self.save_registry_atomic(&data)?;

        // Clean up individual directory if present
        let model_dir = self.models_dir.join(model_id);
        if model_dir.exists() {
            let _ = fs::remove_dir_all(&model_dir);
        }

        Ok(())
    }

    /// Retrieves an AI Model Manifest by ID.
    pub fn get_model(&self, model_id: &str) -> Result<AiModelManifest, AppError> {
        let data = self.load_registry_data()?;
        if let Some(f) = data.families.get(model_id) {
            if let Some(active) = f.active_package() {
                return Ok(active.manifest.clone());
            }
        }
        if let Some(m) = data.models.get(model_id) {
            return Ok(m.clone());
        }
        Err(AppError::model_not_available(
            model_id,
            "Model is not registered in AI registry",
        ))
    }

    /// Lists all registered AI Model Manifests.
    pub fn list_models(&self) -> Result<Vec<AiModelManifest>, AppError> {
        let data = self.load_registry_data()?;
        let mut map = data.models;
        for (id, family) in data.families {
            if let Some(active) = family.active_package() {
                map.entry(id).or_insert_with(|| active.manifest.clone());
            }
        }
        let mut list: Vec<AiModelManifest> = map.into_values().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(list)
    }

    /// Checks if a model ID is registered.
    pub fn exists(&self, model_id: &str) -> bool {
        if let Ok(data) = self.load_registry_data() {
            data.models.contains_key(model_id) || data.families.contains_key(model_id)
        } else {
            false
        }
    }
}
