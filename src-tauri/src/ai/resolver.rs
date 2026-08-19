use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ai::manifest::AiModelManifest;
use crate::ai::onnx::OnnxAiRuntime;
use crate::ai::package::{calculate_file_sha256, validate_model_id, validate_version_str};
use crate::ai::profile::AiModelProfile;
use crate::ai::provider::{get_available_providers, ExecutionProvider};
use crate::ai::registry::ModelRegistry;
use crate::ai::validation::validate_profile_against_onnx;
use crate::error::AppError;

/// Immutable production-resolved AI model metadata bundle ready for job binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedProductionModel {
    pub model_id: String,
    pub model_version: String,
    pub model_name: String,
    pub display_name: String,
    pub model_path: PathBuf,
    pub model_hash: String,
    pub profile_hash: String,
    pub profile: AiModelProfile,
    pub provider: ExecutionProvider,
    pub manifest: AiModelManifest,
    pub file_size_bytes: u64,
    pub supported_providers: Vec<ExecutionProvider>,
}

/// Authoritative resolver and gatekeeper for production AI models.
pub struct ProductionModelResolver;

impl ProductionModelResolver {
    /// Resolves, validates, and permanently binds an AI model from the registry.
    ///
    /// Rules:
    /// 1. If explicit `version` is provided, resolves that exact package version.
    /// 2. If `version` is omitted, resolves the currently ACTIVE production version.
    /// 3. Performs real disk integrity check (verifies file exists, size > 0, SHA-256 match).
    /// 4. Performs standalone ONNX graph inspection and profile compatibility check.
    /// 5. Validates execution provider availability on host and compatibility with model package.
    pub fn resolve_model(
        registry: &ModelRegistry,
        model_id: Option<&str>,
        version: Option<&str>,
        requested_provider: Option<ExecutionProvider>,
    ) -> Result<ResolvedProductionModel, AppError> {
        // A. Validate Model ID
        let id = model_id
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::invalid_input("Model ID is required to resolve a production AI model")
            })?;
        validate_model_id(id)?;

        // B. Retrieve Model Package (Explicit version or Active production version)
        let package = match version.map(|v| v.trim()).filter(|v| !v.is_empty()) {
            Some(ver) => {
                let _ = validate_version_str(ver)?;
                registry
                    .get_package(id, ver)
                    .map_err(|_| AppError::model_version_not_found(id, ver))?
            }
            None => registry
                .get_active_package(id)
                .map_err(|_| AppError::model_not_active(id))?,
        };

        // C. Real File Existence & Size Verification
        if !package.model_file.exists() {
            return Err(AppError::file_not_found(
                package.model_file.display().to_string(),
            ));
        }

        let file_meta = std::fs::metadata(&package.model_file).map_err(|e| {
            AppError::storage_error("Failed to inspect model file on disk", e.to_string())
        })?;

        if file_meta.len() == 0 {
            return Err(AppError::invalid_input(format!(
                "Model file '{}' is empty (0 bytes)",
                package.model_file.display()
            )));
        }

        // D. Real SHA-256 Checksum Verification
        let real_sha256 = calculate_file_sha256(&package.model_file)?;
        if !real_sha256.eq_ignore_ascii_case(&package.sha256) {
            return Err(AppError::model_integrity_mismatch(
                format!(
                    "Model file SHA-256 integrity mismatch for model '{}' v{}",
                    id, package.version
                ),
                format!("Expected: {}, Calculated: {}", package.sha256, real_sha256),
            ));
        }

        // E. Standalone ONNX Graph Inspection & Profile Compatibility
        let onnx_metadata = OnnxAiRuntime::inspect_onnx_file(&package.model_file).map_err(|e| {
            AppError::model_graph_invalid(
                id,
                format!("Failed to parse ONNX graph structure: {}", e.message),
            )
        })?;

        if let Err(profile_errors) = validate_profile_against_onnx(&package.profile, &onnx_metadata)
        {
            return Err(AppError::model_profile_mismatch(
                format!(
                    "Model profile is incompatible with ONNX graph for '{}' v{}",
                    id, package.version
                ),
                profile_errors.join("; "),
            ));
        }

        // F. Execution Provider Resolution (Strict Zero-Fake & Non-Silent Fallback)
        let host_providers = get_available_providers();
        let selected_provider = match requested_provider {
            Some(explicit_p) => {
                // Verify model package supports this provider
                if !package.supported_providers.contains(&explicit_p) {
                    return Err(AppError::model_provider_unsupported(
                        id,
                        format!("{:?}", explicit_p),
                    ));
                }

                // Verify host hardware currently has this provider available
                if !host_providers.contains(&explicit_p) {
                    return Err(AppError::provider_unavailable(
                        format!("{:?}", explicit_p),
                        format!(
                            "Hardware execution provider '{:?}' is not available or drivers missing on this system",
                            explicit_p
                        ),
                    ));
                }

                explicit_p
            }
            None => {
                // Provider AUTO: select best available provider supported by model
                let preference_order = [
                    ExecutionProvider::Cuda,
                    ExecutionProvider::DirectML,
                    ExecutionProvider::CoreML,
                    ExecutionProvider::Cpu,
                ];

                let mut chosen = ExecutionProvider::Cpu;
                for p in preference_order {
                    if package.supported_providers.contains(&p) && host_providers.contains(&p) {
                        chosen = p;
                        break;
                    }
                }
                chosen
            }
        };

        // G. Compute Deterministic Profile Hash
        let profile_hash = package.profile.compute_profile_hash();

        Ok(ResolvedProductionModel {
            model_id: package.model_id.clone(),
            model_version: package.version.clone(),
            model_name: package.model_name.clone(),
            display_name: package.display_name.clone(),
            model_path: package.model_file.clone(),
            model_hash: package.sha256.clone(),
            profile_hash,
            profile: package.profile.clone(),
            provider: selected_provider,
            manifest: package.manifest.clone(),
            file_size_bytes: file_meta.len(),
            supported_providers: package.supported_providers.clone(),
        })
    }
}
