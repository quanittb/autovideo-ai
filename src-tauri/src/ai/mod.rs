pub mod cloud;
pub mod control;
pub mod device;
pub mod flow;
pub mod frame_pipeline;
pub mod generative;
pub mod hybrid;
pub mod manifest;
pub mod onnx;
pub mod package;
pub mod pipeline;
pub mod preflight;
pub mod profile;
pub mod provider;
pub mod registry;
pub mod report;
pub mod resolver;
pub mod resource;
pub mod runtime;
pub mod tensor;
pub mod validation;

pub use device::DeviceInfo;
pub use frame_pipeline::{
    calculate_sha256, compute_ai_config_hash, compute_ai_job_config_hash, select_frames,
    AiArtifactManager, AiFrameExecutor, AiFrameMetadata, AiFrameOutputMode, AiFrameStatus,
    AiJobConfig, AiJobMetrics, AudioPreservationMode, FrameManifestEntry, FrameQualityReport,
    FrameQualityStatus, FrameQualityValidator, FrameSamplingConfig, FrameSamplingMode, RationalFps,
    ReconstructionManifest, ReconstructionResult, ReconstructionTelemetry, TechnicalQualityMetrics,
    VideoCodec, VideoReconstructionConfig, VideoReconstructor,
};
pub use manifest::{AiModelManifest, ModelFormat, ModelRequirements, ModelState};
pub use onnx::{
    generate_minimal_onnx_model, AiTensorInput, AiTensorOutput, InferenceRequest, InferenceResult,
    OnnxAiRuntime, OnnxModelMetadata, OnnxTensorMetadata,
};
pub use package::{
    calculate_file_sha256, validate_model_id, validate_version_str, AiModelFamily, AiModelPackage,
    SemVer,
};
pub use pipeline::{
    extract_mask_from_tensor, generate_image_onnx_model, generate_image_onnx_model_with_weight,
    preprocess_image, validate_preprocess_against_model, AiInferencePipeline, BoundingBox,
    ChannelOrder, CropConfig, CropMetadata, ImageFrame, LetterboxConfig, LetterboxTransform, Mask,
    NormalizationConfig, NormalizationMode, PipelineExecutionReport, PixelFormat,
    PostprocessConfig, PostprocessResult, PreprocessConfig, PreprocessResult,
    PreprocessValidationResult, ResizeConfig, ResizeFilter, ResizeMetadata, TensorLayout,
    TransformMetadata,
};
pub use preflight::{
    validate_ai_job_preflight, AiJobPreflightReport, PreflightCheckResult, PreflightCheckSeverity,
    PreflightCheckStatus,
};
pub use profile::{
    AiModelProfile, AspectHandling, BboxInterpretation, InputProfile, MaskInterpretation,
    OutputInterpretationType, OutputProfile,
};
pub use provider::{detect_providers, select_provider, ExecutionProvider, ProviderInfo};
pub use registry::ModelRegistry;
pub use report::AiProductionExecutionReport;
pub use resolver::{ProductionModelResolver, ResolvedProductionModel};
pub use resource::{probe_runtime_resources, AiResourceLimits, AiRuntimeResources};
pub use runtime::{AiRuntime, DefaultAiRuntime, RuntimeState, RuntimeStatus};
pub use tensor::{Dimension, TensorDataType, TensorSpec};
pub use validation::{
    validate_model_package_deep, validate_profile_against_onnx, ModelValidationReport,
    ProviderCompatibility,
};

// Legacy pipeline traits preserved for forward compatibility
use crate::error::AppError;
use crate::media::AnalysisResult;
use crate::projects::{TransformationPlan, TransformationRequest};
use std::path::{Path, PathBuf};

pub trait AnalysisEngine: Send + Sync {
    fn analyze(&self, video_path: &Path) -> Result<AnalysisResult, AppError>;
}

pub trait TransformationEngine: Send + Sync {
    fn plan(
        &self,
        request: &TransformationRequest,
        analysis: &AnalysisResult,
    ) -> Result<TransformationPlan, AppError>;
}

pub trait CharacterTransformationEngine: Send + Sync {
    fn transform_character(
        &self,
        frames_dir: &Path,
        mask_frames_dir: &Path,
        prompt: &str,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, AppError>;
}

pub trait BackgroundTransformationEngine: Send + Sync {
    fn transform_background(
        &self,
        frames_dir: &Path,
        depth_dir: &Path,
        prompt: &str,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, AppError>;
}

pub trait TemporalConsistencyEngine: Send + Sync {
    fn smooth_temporal_flow(
        &self,
        raw_frames_dir: &Path,
        inpainted_frames_dir: &Path,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, AppError>;
}

pub trait AudioEngine: Send + Sync {
    fn extract_audio(&self, video_path: &Path, output_audio_path: &Path) -> Result<(), AppError>;
    fn mux_audio(
        &self,
        video_frames_dir: &Path,
        audio_path: &Path,
        output_video: &Path,
    ) -> Result<(), AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn create_dummy_onnx_file(dir: &Path, filename: &str, size_bytes: usize) -> PathBuf {
        let path = dir.join(filename);
        let data = vec![0x30; size_bytes];
        fs::write(&path, data).expect("Failed to write dummy onnx file");
        path
    }

    // -------------------------------------------------------------
    // MODEL TESTS
    // -------------------------------------------------------------

    #[test]
    fn test_model_manifest_creation() {
        let temp = tempdir().unwrap();
        let model_path = create_dummy_onnx_file(temp.path(), "model.onnx", 1024);

        let inputs = vec![TensorSpec::new(
            "input",
            TensorDataType::Float32,
            vec![
                Dimension::dynamic("batch"),
                Dimension::fixed(3),
                Dimension::fixed(512),
                Dimension::fixed(512),
            ],
        )];
        let outputs = vec![TensorSpec::new(
            "output",
            TensorDataType::Float32,
            vec![
                Dimension::dynamic("batch"),
                Dimension::fixed(3),
                Dimension::fixed(512),
                Dimension::fixed(512),
            ],
        )];

        let manifest = AiModelManifest::new(
            "test-model-1",
            "Super Resolution v1",
            "1.0.0",
            ModelFormat::Onnx,
            model_path.clone(),
            "AI Upscaler model",
            inputs,
            outputs,
            ModelRequirements::default(),
        );

        assert_eq!(manifest.id, "test-model-1");
        assert_eq!(manifest.name, "Super Resolution v1");
        assert_eq!(manifest.format, ModelFormat::Onnx);
        assert_eq!(manifest.input_specs.len(), 1);
        assert_eq!(manifest.output_specs.len(), 1);
        assert_eq!(manifest.path, model_path);
    }

    #[test]
    fn test_model_id_uniqueness() {
        let temp = tempdir().unwrap();
        let registry = ModelRegistry::new(temp.path().join("models"));
        let model_path = create_dummy_onnx_file(temp.path(), "model.onnx", 512);

        let manifest1 = AiModelManifest::new(
            "unique-model-1",
            "Model 1",
            "1.0",
            ModelFormat::Onnx,
            model_path.clone(),
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let manifest2 = AiModelManifest::new(
            "unique-model-2",
            "Model 2",
            "1.0",
            ModelFormat::Onnx,
            model_path,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        assert!(registry.register_model(manifest1).is_ok());
        assert!(registry.register_model(manifest2).is_ok());
        assert_eq!(registry.list_models().unwrap().len(), 2);
    }

    #[test]
    fn test_model_registration() {
        let temp = tempdir().unwrap();
        let registry = ModelRegistry::new(temp.path().join("models"));
        let model_path = create_dummy_onnx_file(temp.path(), "test.onnx", 2048);

        let manifest = AiModelManifest::new(
            "reg-model",
            "Registered Model",
            "1.0",
            ModelFormat::Onnx,
            model_path,
            "A test model",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let registered = registry.register_model(manifest).unwrap();
        assert_eq!(registered.id, "reg-model");
        assert!(registry.exists("reg-model"));

        let retrieved = registry.get_model("reg-model").unwrap();
        assert_eq!(retrieved.name, "Registered Model");
    }

    #[test]
    fn test_model_unregistration() {
        let temp = tempdir().unwrap();
        let registry = ModelRegistry::new(temp.path().join("models"));
        let model_path = create_dummy_onnx_file(temp.path(), "unreg.onnx", 100);

        let manifest = AiModelManifest::new(
            "unreg-model",
            "To Be Unregistered",
            "1.0",
            ModelFormat::Onnx,
            model_path,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        registry.register_model(manifest).unwrap();
        assert!(registry.exists("unreg-model"));

        assert!(registry.unregister_model("unreg-model").is_ok());
        assert!(!registry.exists("unreg-model"));
        assert!(registry.get_model("unreg-model").is_err());
    }

    #[test]
    fn test_duplicate_model_rejected() {
        let temp = tempdir().unwrap();
        let registry = ModelRegistry::new(temp.path().join("models"));
        let model_path = create_dummy_onnx_file(temp.path(), "dupe.onnx", 100);

        let manifest1 = AiModelManifest::new(
            "duplicate-id",
            "First Model",
            "1.0",
            ModelFormat::Onnx,
            model_path.clone(),
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let manifest2 = AiModelManifest::new(
            "duplicate-id",
            "Second Model",
            "2.0",
            ModelFormat::Onnx,
            model_path,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        assert!(registry.register_model(manifest1).is_ok());
        let err = registry.register_model(manifest2).unwrap_err();
        assert!(err.to_string().contains("already registered"));
    }

    #[test]
    fn test_missing_model_rejected() {
        let temp = tempdir().unwrap();
        let registry = ModelRegistry::new(temp.path().join("models"));
        let non_existent = temp.path().join("missing.onnx");

        let manifest = AiModelManifest::new(
            "missing-model",
            "Missing Model",
            "1.0",
            ModelFormat::Onnx,
            non_existent,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let err = registry.register_model(manifest).unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::FileNotFound);
        assert!(
            err.to_string().contains("File not found") || err.to_string().contains("missing.onnx")
        );
    }

    #[test]
    fn test_empty_model_rejected() {
        let temp = tempdir().unwrap();
        let registry = ModelRegistry::new(temp.path().join("models"));
        let empty_path = create_dummy_onnx_file(temp.path(), "empty.onnx", 0);

        let manifest = AiModelManifest::new(
            "empty-model",
            "Empty Model",
            "1.0",
            ModelFormat::Onnx,
            empty_path,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let err = registry.register_model(manifest).unwrap_err();
        assert!(err.to_string().contains("empty (0 bytes)"));
    }

    // -------------------------------------------------------------
    // RUNTIME TESTS
    // -------------------------------------------------------------

    #[test]
    fn test_runtime_initial_state() {
        let runtime = DefaultAiRuntime::new();
        let status = runtime.status();
        assert_eq!(status.state, RuntimeState::Uninitialized);
        assert_eq!(status.model_state, ModelState::Unloaded);
        assert!(status.loaded_model_id.is_none());
    }

    #[test]
    fn test_runtime_initialization() {
        let mut runtime = DefaultAiRuntime::new();
        assert!(runtime.initialize(Some(ExecutionProvider::Cpu)).is_ok());
        let status = runtime.status();
        assert_eq!(status.state, RuntimeState::Ready);
        assert_eq!(status.provider, ExecutionProvider::Cpu);
    }

    #[test]
    fn test_model_loading_state() {
        let temp = tempdir().unwrap();
        let model_path = create_dummy_onnx_file(temp.path(), "loadable.onnx", 1024);

        let manifest = AiModelManifest::new(
            "loadable-model",
            "Loadable Model",
            "1.0",
            ModelFormat::Onnx,
            model_path,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let mut runtime = DefaultAiRuntime::new();
        runtime.initialize(Some(ExecutionProvider::Cpu)).unwrap();
        assert!(runtime.load_model(&manifest).is_ok());

        let status = runtime.status();
        assert_eq!(status.model_state, ModelState::Ready);
        assert_eq!(status.loaded_model_id, Some("loadable-model".to_string()));
    }

    #[test]
    fn test_model_unloading_state() {
        let temp = tempdir().unwrap();
        let model_path = create_dummy_onnx_file(temp.path(), "unloadable.onnx", 1024);

        let manifest = AiModelManifest::new(
            "unloadable-model",
            "Unloadable Model",
            "1.0",
            ModelFormat::Onnx,
            model_path,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let mut runtime = DefaultAiRuntime::new();
        runtime.initialize(Some(ExecutionProvider::Cpu)).unwrap();
        runtime.load_model(&manifest).unwrap();
        assert!(runtime.unload_model().is_ok());

        let status = runtime.status();
        assert_eq!(status.model_state, ModelState::Unloaded);
        assert!(status.loaded_model_id.is_none());
    }

    #[test]
    fn test_invalid_runtime_transition() {
        // ModelState transition logic
        assert!(!ModelState::Unloaded.can_transition_to(ModelState::Running));
        assert!(ModelState::Unloaded.can_transition_to(ModelState::Loading));
        assert!(ModelState::Loading.can_transition_to(ModelState::Ready));
        assert!(ModelState::Ready.can_transition_to(ModelState::Running));
        assert!(!ModelState::Loading.can_transition_to(ModelState::Running));
    }

    // -------------------------------------------------------------
    // PROVIDER TESTS
    // -------------------------------------------------------------

    #[test]
    fn test_cpu_provider_available() {
        let providers = detect_providers();
        let cpu = providers
            .iter()
            .find(|p| p.provider == ExecutionProvider::Cpu)
            .unwrap();
        assert!(cpu.supported);
        assert!(cpu.available);
    }

    #[test]
    fn test_provider_capability_reporting() {
        let providers = detect_providers();
        assert!(providers.len() >= 5);
        for p in &providers {
            assert!(p.reason.is_some());
        }
    }

    #[test]
    fn test_unavailable_requested_provider_returns_error() {
        // Explicitly request a provider that is unavailable
        let res = select_provider(Some(ExecutionProvider::TensorRT));
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(err.to_string().contains("not available"));
    }

    #[test]
    fn test_automatic_provider_selection() {
        let res = select_provider(None);
        assert!(res.is_ok());
        let provider = res.unwrap();
        // Automatic selection must pick an available provider (e.g. DirectML, CoreML, or CPU)
        let providers = detect_providers();
        let info = providers.iter().find(|p| p.provider == provider).unwrap();
        assert!(info.available);
    }

    #[test]
    fn test_cpu_fallback() {
        let mut runtime = DefaultAiRuntime::new();
        // Initialize with default/fallback CPU
        assert!(runtime.initialize(Some(ExecutionProvider::Cpu)).is_ok());
        assert_eq!(runtime.provider(), ExecutionProvider::Cpu);
    }

    // -------------------------------------------------------------
    // DEVICE TESTS
    // -------------------------------------------------------------

    #[test]
    fn test_device_detection() {
        let device = DeviceInfo::detect();
        assert!(!device.os.is_empty());
        assert!(!device.arch.is_empty());
        assert!(device.cpu_cores > 0);
    }

    #[test]
    fn test_device_info_has_real_values_or_none() {
        let device = DeviceInfo::detect();
        assert!(device.cpu_name.is_some());
        assert!(device.total_memory_bytes.is_some());
    }

    // -------------------------------------------------------------
    // PERSISTENCE TESTS
    // -------------------------------------------------------------

    #[test]
    fn test_model_registry_persistence() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir.clone());
        let model_path = create_dummy_onnx_file(temp.path(), "persist.onnx", 512);

        let manifest = AiModelManifest::new(
            "persist-model",
            "Persisted Model",
            "1.0",
            ModelFormat::Onnx,
            model_path,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        registry.register_model(manifest).unwrap();
        assert!(models_dir.join("registry.json").exists());
        assert!(models_dir
            .join("persist-model")
            .join("manifest.json")
            .exists());
    }

    #[test]
    fn test_atomic_registry_persistence() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir.clone());
        let model_path = create_dummy_onnx_file(temp.path(), "atomic.onnx", 512);

        let manifest = AiModelManifest::new(
            "atomic-model",
            "Atomic Model",
            "1.0",
            ModelFormat::Onnx,
            model_path,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        registry.register_model(manifest).unwrap();

        // Check that temp files were cleaned up
        let entries: Vec<_> = fs::read_dir(&models_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();

        assert!(entries.iter().any(|name| name == "registry.json"));
        assert!(!entries
            .iter()
            .any(|name| name.starts_with("registry.json.tmp.")));
    }

    #[test]
    fn test_registry_reload() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir.clone());
        let model_path = create_dummy_onnx_file(temp.path(), "reload.onnx", 512);

        let manifest = AiModelManifest::new(
            "reload-model",
            "Reload Model",
            "1.0",
            ModelFormat::Onnx,
            model_path,
            "desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        registry.register_model(manifest).unwrap();

        // Create a new registry instance pointing to the same folder
        let reloaded_registry = ModelRegistry::new(models_dir);
        let list = reloaded_registry.list_models().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "reload-model");
        assert_eq!(list[0].name, "Reload Model");
    }
}

#[cfg(test)]
mod tests_phase6f;

#[cfg(test)]
mod tests_phase6g;

#[cfg(test)]
mod tests_phase6h;

#[cfg(test)]
mod tests_phase6i;

#[cfg(test)]
mod tests_phase6j;

#[cfg(test)]
mod tests_phase6k;

#[cfg(test)]
mod tests_phase7a;

#[cfg(test)]
mod tests_phase7b;

#[cfg(test)]
mod tests_phase7c;

#[cfg(test)]
mod tests_phase7d;

#[cfg(test)]
mod tests_phase7e;

#[cfg(test)]
mod tests_phase7f;

#[cfg(test)]
mod tests_phase7g;

#[cfg(test)]
mod tests_phase8;

#[cfg(test)]
mod tests_phase9;

#[cfg(test)]
mod tests_phase10;

#[cfg(test)]
mod tests_phase11;

#[cfg(test)]
mod tests_phase12;

#[cfg(test)]
mod tests_cloud_mvp;

#[cfg(test)]
mod tests_phase15;

#[cfg(test)]
mod tests_phase16;

#[cfg(test)]
mod tests_phase17;

#[cfg(test)]
mod tests_phase18;

#[cfg(test)]
mod tests_phase19;

#[cfg(test)]
mod tests_phase20a;
