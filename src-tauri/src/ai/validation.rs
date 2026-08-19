use serde::{Deserialize, Serialize};

use crate::ai::onnx::{OnnxAiRuntime, OnnxModelMetadata};
use crate::ai::package::AiModelPackage;
use crate::ai::profile::{AiModelProfile, OutputInterpretationType};
use crate::ai::provider::{get_available_providers, ExecutionProvider};
use crate::ai::tensor::Dimension;
use crate::error::AppError;

/// Compatibility status for an execution provider on the local host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCompatibility {
    pub provider: ExecutionProvider,
    pub supported: bool,
    pub available_on_host: bool,
    pub reason: Option<String>,
}

/// Comprehensive Model Package Validation Report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelValidationReport {
    pub valid: bool,
    pub model_id: String,
    pub version: String,
    pub integrity_valid: bool,
    pub sha256: String,
    pub onnx_valid: bool,
    pub onnx_metadata: Option<OnnxModelMetadata>,
    pub profile_valid: bool,
    pub provider_compatibility: Vec<ProviderCompatibility>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Validates an AiModelProfile against real ONNX session metadata.
pub fn validate_profile_against_onnx(
    profile: &AiModelProfile,
    onnx_meta: &OnnxModelMetadata,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    // 1. Validate inputs count
    if onnx_meta.input_count == 0 {
        errors.push("ONNX model has no input tensors defined in graph".to_string());
    }

    if let Some(first_input) = onnx_meta.inputs.first() {
        // If tensor name is specified in profile, check exact match
        if let Some(ref expected_name) = profile.input.tensor_name {
            if &first_input.name != expected_name {
                errors.push(format!(
                    "Input tensor name mismatch: expected '{}', got '{}'",
                    expected_name, first_input.name
                ));
            }
        }

        // Validate data type
        if first_input.data_type != profile.input.data_type {
            errors.push(format!(
                "Input data type mismatch: expected {:?}, got {:?}",
                profile.input.data_type, first_input.data_type
            ));
        }

        // Validate rank
        let rank = first_input.shape.len();
        match profile.input.layout {
            crate::ai::pipeline::layout::TensorLayout::Nchw
            | crate::ai::pipeline::layout::TensorLayout::Nhwc => {
                if rank != 4 {
                    errors.push(format!(
                        "Input rank mismatch for 4D layout {:?}: expected rank 4, got rank {}",
                        profile.input.layout, rank
                    ));
                } else {
                    // Check dimension sizes for fixed dimensions
                    match profile.input.layout {
                        crate::ai::pipeline::layout::TensorLayout::Nchw => {
                            // [N, C, H, W]
                            let (exp_c, exp_h, exp_w) = (
                                profile.input.channel_order.channels() as u64,
                                profile.input.target_height as u64,
                                profile.input.target_width as u64,
                            );
                            if let Dimension::Fixed(c) = first_input.shape[1] {
                                if c != exp_c {
                                    errors.push(format!(
                                        "Input channel mismatch: expected {} channels (for {:?}), got {}",
                                        exp_c, profile.input.channel_order, c
                                    ));
                                }
                            }
                            if let Dimension::Fixed(h) = first_input.shape[2] {
                                if h != exp_h {
                                    errors.push(format!(
                                        "Input height mismatch: expected {}, got {}",
                                        exp_h, h
                                    ));
                                }
                            }
                            if let Dimension::Fixed(w) = first_input.shape[3] {
                                if w != exp_w {
                                    errors.push(format!(
                                        "Input width mismatch: expected {}, got {}",
                                        exp_w, w
                                    ));
                                }
                            }
                        }
                        crate::ai::pipeline::layout::TensorLayout::Nhwc => {
                            // [N, H, W, C]
                            let (exp_h, exp_w, exp_c) = (
                                profile.input.target_height as u64,
                                profile.input.target_width as u64,
                                profile.input.channel_order.channels() as u64,
                            );
                            if let Dimension::Fixed(h) = first_input.shape[1] {
                                if h != exp_h {
                                    errors.push(format!(
                                        "Input height mismatch: expected {}, got {}",
                                        exp_h, h
                                    ));
                                }
                            }
                            if let Dimension::Fixed(w) = first_input.shape[2] {
                                if w != exp_w {
                                    errors.push(format!(
                                        "Input width mismatch: expected {}, got {}",
                                        exp_w, w
                                    ));
                                }
                            }
                            if let Dimension::Fixed(c) = first_input.shape[3] {
                                if c != exp_c {
                                    errors.push(format!(
                                        "Input channel mismatch: expected {} channels (for {:?}), got {}",
                                        exp_c, profile.input.channel_order, c
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Validate outputs count & profile
    if onnx_meta.output_count == 0 {
        errors.push("ONNX model has no output tensors defined in graph".to_string());
    }

    if let Some(first_output) = onnx_meta.outputs.first() {
        if let Some(ref expected_name) = profile.output.tensor_name {
            if &first_output.name != expected_name {
                errors.push(format!(
                    "Output tensor name mismatch: expected '{}', got '{}'",
                    expected_name, first_output.name
                ));
            }
        }

        // Validate output interpretation types
        match profile.output.output_type {
            OutputInterpretationType::Mask => {
                // Mask tensors usually have rank 2, 3, or 4
                if first_output.shape.len() < 2 {
                    errors.push(format!(
                        "Mask output tensor rank too small: expected >= 2, got {}",
                        first_output.shape.len()
                    ));
                }
            }
            OutputInterpretationType::BBox => {
                // Bounding boxes usually have rank 2 or 3
                if first_output.shape.len() < 2 {
                    errors.push(format!(
                        "Bounding box output tensor rank too small: expected >= 2, got {}",
                        first_output.shape.len()
                    ));
                }
            }
            OutputInterpretationType::Image => {
                // Image output tensors usually have rank 3 or 4
                if first_output.shape.len() < 3 {
                    errors.push(format!(
                        "Image output tensor rank too small: expected >= 3, got {}",
                        first_output.shape.len()
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Authoritative deep validation runner for a complete AiModelPackage.
pub fn validate_model_package_deep(
    package: &AiModelPackage,
) -> Result<ModelValidationReport, AppError> {
    let mut errors = Vec::new();
    let warnings = Vec::new();

    // 1. File Integrity Check
    let mut integrity_valid = false;
    match package.verify_integrity() {
        Ok(()) => {
            integrity_valid = true;
        }
        Err(e) => {
            errors.push(format!("File integrity check failed: {}", e.message));
        }
    }

    // 2. Real ONNX Graph & Metadata Inspection
    let mut onnx_valid = false;
    let mut onnx_metadata = None;
    if package.model_file.exists() {
        match OnnxAiRuntime::inspect_onnx_file(&package.model_file) {
            Ok(meta) => {
                onnx_valid = true;
                onnx_metadata = Some(meta);
            }
            Err(e) => {
                errors.push(format!("ONNX graph inspection failed: {}", e.message));
            }
        }
    } else {
        errors.push(format!(
            "Model file not found on disk: {}",
            package.model_file.display()
        ));
    }

    // 3. Profile Compatibility Check
    let mut profile_valid = false;
    if let Some(ref meta) = onnx_metadata {
        match validate_profile_against_onnx(&package.profile, meta) {
            Ok(()) => {
                profile_valid = true;
            }
            Err(profile_errs) => {
                errors.extend(profile_errs);
            }
        }
    }

    // 4. Provider Compatibility Check
    let available_host_providers = get_available_providers();
    let mut provider_compatibility = Vec::new();

    let target_providers = if package.supported_providers.is_empty() {
        vec![ExecutionProvider::Cpu, ExecutionProvider::DirectML]
    } else {
        package.supported_providers.clone()
    };

    for &prov in &target_providers {
        let is_available = available_host_providers.contains(&prov);
        let reason = if is_available {
            None
        } else {
            Some(format!(
                "Provider {:?} is not available on host hardware/drivers",
                prov
            ))
        };

        provider_compatibility.push(ProviderCompatibility {
            provider: prov,
            supported: true,
            available_on_host: is_available,
            reason,
        });
    }

    let has_any_usable_provider = provider_compatibility.iter().any(|p| p.available_on_host);
    if !has_any_usable_provider {
        errors.push(
            "None of the required execution providers are supported on this host system"
                .to_string(),
        );
    }

    let is_overall_valid = integrity_valid
        && onnx_valid
        && profile_valid
        && has_any_usable_provider
        && errors.is_empty();

    Ok(ModelValidationReport {
        valid: is_overall_valid,
        model_id: package.model_id.clone(),
        version: package.version.clone(),
        integrity_valid,
        sha256: package.sha256.clone(),
        onnx_valid,
        onnx_metadata,
        profile_valid,
        provider_compatibility,
        warnings,
        errors,
    })
}
