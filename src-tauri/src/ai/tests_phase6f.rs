#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    use crate::ai::manifest::{AiModelManifest, ModelFormat, ModelRequirements};
    use crate::ai::onnx::{generate_minimal_onnx_model, OnnxAiRuntime};
    use crate::ai::package::{
        calculate_file_sha256, validate_model_id, validate_version_str, AiModelFamily,
        AiModelPackage, SemVer,
    };
    use crate::ai::pipeline::generate_image_onnx_model;
    use crate::ai::pipeline::layout::{ChannelOrder, TensorLayout};
    use crate::ai::pipeline::preprocess::PreprocessConfig;
    use crate::ai::profile::{AiModelProfile, OutputInterpretationType};
    use crate::ai::provider::ExecutionProvider;
    use crate::ai::registry::ModelRegistry;
    use crate::ai::tensor::TensorDataType;
    use crate::ai::validation::{validate_model_package_deep, validate_profile_against_onnx};
    use crate::error::ErrorCode;

    fn create_test_onnx_file(dir: &std::path::Path, filename: &str) -> PathBuf {
        let path = dir.join(filename);
        generate_image_onnx_model(&path).expect("Failed to create test ONNX model");
        path
    }

    fn create_test_1d_onnx_file(dir: &std::path::Path, filename: &str) -> PathBuf {
        let path = dir.join(filename);
        generate_minimal_onnx_model(&path).expect("Failed to create test 1D ONNX model");
        path
    }

    #[test]
    fn test_phase6f_01_model_package_serialization() {
        let temp = tempdir().unwrap();
        let model_path = create_test_onnx_file(temp.path(), "model.onnx");
        let sha256 = calculate_file_sha256(&model_path).unwrap();

        let manifest = AiModelManifest::new(
            "test-pkg",
            "Test Package",
            "1.0.0",
            ModelFormat::Onnx,
            model_path.clone(),
            "Description",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let pkg = AiModelPackage::new(
            "test-pkg",
            "Test Package",
            "1.0.0",
            "Test Package Display",
            "Description",
            ModelFormat::Onnx,
            model_path,
            1024,
            sha256,
            manifest,
            AiModelProfile::default(),
            ModelRequirements::default(),
            vec![ExecutionProvider::Cpu],
        )
        .unwrap();

        let json = serde_json::to_string_pretty(&pkg).unwrap();
        let deserialized: AiModelPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(pkg.model_id, deserialized.model_id);
        assert_eq!(pkg.version, deserialized.version);
        assert_eq!(pkg.sha256, deserialized.sha256);
    }

    #[test]
    fn test_phase6f_02_semantic_version_validation() {
        let v1 = SemVer::parse("1.2.3").unwrap();
        assert_eq!(v1, SemVer::new(1, 2, 3));
        assert_eq!(v1.to_string(), "1.2.3");

        let v2 = SemVer::parse("v2.0.1").unwrap();
        assert_eq!(v2, SemVer::new(2, 0, 1));

        assert!(v2 > v1);
        assert_eq!(SemVer::parse("1.0").unwrap(), SemVer::new(1, 0, 0));
        assert!(SemVer::parse("invalid").is_err());
        assert!(SemVer::parse("1.2.3.4").is_err());
    }

    #[test]
    fn test_phase6f_03_duplicate_version_rejection() {
        let temp = tempdir().unwrap();
        let model_path = create_test_onnx_file(temp.path(), "model.onnx");
        let sha256 = calculate_file_sha256(&model_path).unwrap();

        let mut family = AiModelFamily::new("person-seg", "Person Segmentation").unwrap();

        let pkg1 = AiModelPackage::new(
            "person-seg",
            "Person Segmentation",
            "1.0.0",
            "Person Seg v1.0",
            "Desc",
            ModelFormat::Onnx,
            model_path.clone(),
            1024,
            sha256.clone(),
            AiModelManifest::new(
                "person-seg",
                "Person Segmentation",
                "1.0.0",
                ModelFormat::Onnx,
                model_path.clone(),
                "Desc",
                vec![],
                vec![],
                ModelRequirements::default(),
            ),
            AiModelProfile::default(),
            ModelRequirements::default(),
            vec![ExecutionProvider::Cpu],
        )
        .unwrap();

        assert!(family.add_version(pkg1.clone()).is_ok());

        // Attempting to add duplicate version 1.0.0 must fail
        let err = family.add_version(pkg1).unwrap_err();
        assert_eq!(err.code, ErrorCode::ModelVersionExists);
    }

    #[test]
    fn test_phase6f_04_model_sha256_calculation() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("test.bin");
        let data = b"Hello AutoVideo AI Zero-Fake SHA-256";
        fs::write(&file_path, data).unwrap();

        let hash = calculate_file_sha256(&file_path).unwrap();
        assert_eq!(hash.len(), 64);

        // Verify repeatability
        let hash2 = calculate_file_sha256(&file_path).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_phase6f_05_integrity_validation() {
        let temp = tempdir().unwrap();
        let model_path = create_test_onnx_file(temp.path(), "model.onnx");
        let sha256 = calculate_file_sha256(&model_path).unwrap();

        let manifest = AiModelManifest::new(
            "seg",
            "Seg",
            "1.0.0",
            ModelFormat::Onnx,
            model_path.clone(),
            "Desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let pkg = AiModelPackage::new(
            "seg",
            "Seg",
            "1.0.0",
            "Seg",
            "Desc",
            ModelFormat::Onnx,
            model_path,
            100,
            sha256,
            manifest,
            AiModelProfile::default(),
            ModelRequirements::default(),
            vec![ExecutionProvider::Cpu],
        )
        .unwrap();

        assert!(pkg.verify_integrity().is_ok());
    }

    #[test]
    fn test_phase6f_06_integrity_mismatch_rejection() {
        let temp = tempdir().unwrap();
        let model_path = create_test_onnx_file(temp.path(), "model.onnx");

        let manifest = AiModelManifest::new(
            "seg",
            "Seg",
            "1.0.0",
            ModelFormat::Onnx,
            model_path.clone(),
            "Desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let pkg = AiModelPackage::new(
            "seg",
            "Seg",
            "1.0.0",
            "Seg",
            "Desc",
            ModelFormat::Onnx,
            model_path,
            100,
            "0000000000000000000000000000000000000000000000000000000000000000",
            manifest,
            AiModelProfile::default(),
            ModelRequirements::default(),
            vec![ExecutionProvider::Cpu],
        )
        .unwrap();

        let err = pkg.verify_integrity().unwrap_err();
        assert_eq!(err.code, ErrorCode::ModelIntegrityMismatch);
    }

    #[test]
    fn test_phase6f_07_model_import_flow() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "source_model.onnx");

        // Profile matching image model [1, 3, 2, 2]
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;
        profile.output.output_type = OutputInterpretationType::Image;

        let pkg = registry
            .import_model(
                &src_onnx,
                "imported-image-model",
                "Imported Image Model",
                "1.0.0",
                "Imported Model Display",
                "Test imported model",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        assert_eq!(pkg.model_id, "imported-image-model");
        assert_eq!(pkg.version, "1.0.0");
        assert!(pkg.model_file.exists());
        assert!(pkg.verify_integrity().is_ok());
    }

    #[test]
    fn test_phase6f_08_imported_model_metadata_validation() {
        let temp = tempdir().unwrap();
        let model_path = create_test_onnx_file(temp.path(), "image_model.onnx");

        let meta = OnnxAiRuntime::inspect_onnx_file(&model_path).unwrap();
        assert_eq!(meta.input_count, 1);
        assert_eq!(meta.output_count, 1);
        assert_eq!(meta.inputs[0].name, "images");
        assert_eq!(meta.outputs[0].name, "output");
        assert_eq!(meta.inputs[0].data_type, TensorDataType::Float32);
    }

    #[test]
    fn test_phase6f_09_profile_serialization() {
        let profile = AiModelProfile::default();
        let json = serde_json::to_string_pretty(&profile).unwrap();
        let des: AiModelProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile, des);
    }

    #[test]
    fn test_phase6f_10_profile_validation() {
        let temp = tempdir().unwrap();
        let model_path = create_test_onnx_file(temp.path(), "image_model.onnx");
        let meta = OnnxAiRuntime::inspect_onnx_file(&model_path).unwrap();

        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;
        profile.output.output_type = OutputInterpretationType::Image;

        assert!(validate_profile_against_onnx(&profile, &meta).is_ok());
    }

    #[test]
    fn test_phase6f_11_profile_input_mismatch() {
        let temp = tempdir().unwrap();
        let model_path = create_test_onnx_file(temp.path(), "image_model.onnx");
        let meta = OnnxAiRuntime::inspect_onnx_file(&model_path).unwrap();

        let mut profile = AiModelProfile::default();
        // Model expects 2x2, we specify 640x640
        profile.input.target_width = 640;
        profile.input.target_height = 640;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let res = validate_profile_against_onnx(&profile, &meta);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert!(errs.iter().any(|e| e.contains("Input height mismatch")));
    }

    #[test]
    fn test_phase6f_12_profile_output_mismatch() {
        let temp = tempdir().unwrap();
        let model_path = create_test_onnx_file(temp.path(), "image_model.onnx");
        let meta = OnnxAiRuntime::inspect_onnx_file(&model_path).unwrap();

        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;
        profile.output.tensor_name = Some("non_existent_tensor".to_string());

        let res = validate_profile_against_onnx(&profile, &meta);
        assert!(res.is_err());
        let errs = res.unwrap_err();
        assert!(errs
            .iter()
            .any(|e| e.contains("Output tensor name mismatch")));
    }

    #[test]
    fn test_phase6f_13_provider_compatibility() {
        let available = crate::ai::provider::get_available_providers();
        assert!(available.contains(&ExecutionProvider::Cpu));
    }

    #[test]
    fn test_phase6f_14_unavailable_provider_rejection() {
        let temp = tempdir().unwrap();
        let model_path = create_test_onnx_file(temp.path(), "model.onnx");
        let sha256 = calculate_file_sha256(&model_path).unwrap();

        let manifest = AiModelManifest::new(
            "cuda-model",
            "Cuda Model",
            "1.0.0",
            ModelFormat::Onnx,
            model_path.clone(),
            "Desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let pkg = AiModelPackage::new(
            "cuda-model",
            "Cuda Model",
            "1.0.0",
            "Cuda Model",
            "Desc",
            ModelFormat::Onnx,
            model_path,
            100,
            sha256,
            manifest,
            AiModelProfile::default(),
            ModelRequirements::default(),
            vec![ExecutionProvider::Cuda], // Only CUDA
        )
        .unwrap();

        let report = validate_model_package_deep(&pkg).unwrap();
        // If CUDA is not installed on the system, deep validation should report invalid
        let has_cuda =
            crate::ai::provider::get_available_providers().contains(&ExecutionProvider::Cuda);
        if !has_cuda {
            assert!(!report.valid);
        }
    }

    #[test]
    fn test_phase6f_15_model_activation() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "model_v1.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "multi-version-model",
                "Multi Version Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile.clone(),
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let _ = registry
            .import_model(
                &src_onnx,
                "multi-version-model",
                "Multi Version Model",
                "1.1.0",
                "v1.1",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let active = registry
            .activate_version("multi-version-model", "1.1.0")
            .unwrap();
        assert_eq!(active.version, "1.1.0");

        let active_pkg = registry.get_active_package("multi-version-model").unwrap();
        assert_eq!(active_pkg.version, "1.1.0");
    }

    #[test]
    fn test_phase6f_16_failed_activation_preserves_previous_active() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "model_v1.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "safe-active-model",
                "Safe Active Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        // Attempt to activate non-existent version 2.0.0
        let res = registry.activate_version("safe-active-model", "2.0.0");
        assert!(res.is_err());

        // Previous version 1.0.0 must remain active
        let active = registry.get_active_package("safe-active-model").unwrap();
        assert_eq!(active.version, "1.0.0");
    }

    #[test]
    fn test_phase6f_17_rollback() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "rollback-model",
                "Rollback Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile.clone(),
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let _ = registry
            .import_model(
                &src_onnx,
                "rollback-model",
                "Rollback Model",
                "1.1.0",
                "v1.1",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let _ = registry
            .activate_version("rollback-model", "1.1.0")
            .unwrap();
        assert_eq!(
            registry
                .get_active_package("rollback-model")
                .unwrap()
                .version,
            "1.1.0"
        );

        let rolled = registry.rollback_model("rollback-model").unwrap();
        assert_eq!(rolled.version, "1.0.0");
        assert_eq!(
            registry
                .get_active_package("rollback-model")
                .unwrap()
                .version,
            "1.0.0"
        );
    }

    #[test]
    fn test_phase6f_18_rollback_with_no_previous_version() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "single-ver-model",
                "Single Ver Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let res = registry.rollback_model("single-ver-model");
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6f_19_remove_inactive_version() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "removable-model",
                "Removable Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile.clone(),
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let _ = registry
            .import_model(
                &src_onnx,
                "removable-model",
                "Removable Model",
                "1.1.0",
                "v1.1",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        // 1.0.0 is active by default. Let's remove 1.1.0 (inactive)
        let removed = registry.remove_version("removable-model", "1.1.0").unwrap();
        assert_eq!(removed.version, "1.1.0");

        assert!(registry.get_package("removable-model", "1.1.0").is_err());
        assert!(registry.get_package("removable-model", "1.0.0").is_ok());
    }

    #[test]
    fn test_phase6f_20_reject_removal_of_active_version() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "protect-active-model",
                "Protect Active Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile.clone(),
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let _ = registry
            .import_model(
                &src_onnx,
                "protect-active-model",
                "Protect Active Model",
                "1.1.0",
                "v1.1",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        // 1.0.0 is active. Attempting to remove it when other versions exist must fail
        let res = registry.remove_version("protect-active-model", "1.0.0");
        assert!(res.is_err());
    }

    #[test]
    fn test_phase6f_21_registry_persistence() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir.clone());

        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "persisted-model",
                "Persisted Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        // New registry instance pointing to same directory
        let registry2 = ModelRegistry::new(models_dir);
        let families = registry2.list_families().unwrap();
        assert_eq!(families.len(), 1);
        assert_eq!(families[0].model_id, "persisted-model");
    }

    #[test]
    fn test_phase6f_22_registry_recovery() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir.clone());

        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "recovery-model",
                "Recovery Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        assert!(registry.exists("recovery-model"));
    }

    #[test]
    fn test_phase6f_23_deterministic_config_hash() {
        let prep = PreprocessConfig::default();
        let h1 = crate::ai::compute_ai_config_hash("test-model", &prep, None);
        let h2 = crate::ai::compute_ai_config_hash("test-model", &prep, None);
        assert_eq!(h1, h2);

        let h3 = crate::ai::compute_ai_config_hash("other-model", &prep, None);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_phase6f_24_job_model_version_pinning() {
        let temp = tempdir().unwrap();
        let storage = crate::system::StoragePaths::resolve_from_base(temp.path());
        let registry = ModelRegistry::new(storage.models_dir.clone());
        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");

        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "pinned-model",
                "Pinned Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let engine = crate::jobs::JobEngine::new(storage);
        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: "pinned-model".to_string(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: PreprocessConfig::default(),
            postprocessing: None,
            frame_sampling: crate::ai::FrameSamplingConfig::all(),
            output_mode: crate::ai::AiFrameOutputMode::Image,
        };

        let job = engine
            .create_ai_job("proj-pin-test", None, vec![], ai_cfg)
            .unwrap();
        let job_ai = job.ai_config.unwrap();

        // Must be automatically pinned to active version 1.0.0
        assert_eq!(job_ai.model_version, Some("1.0.0".to_string()));
        assert!(job_ai.model_hash.is_some());
        assert!(job_ai.profile_hash.is_some());
    }

    #[test]
    fn test_phase6f_25_job_model_hash_pinning() {
        let temp = tempdir().unwrap();
        let storage = crate::system::StoragePaths::resolve_from_base(temp.path());
        let registry = ModelRegistry::new(storage.models_dir.clone());
        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let expected_sha256 = calculate_file_sha256(&src_onnx).unwrap();

        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "hash-pinned-model",
                "Hash Pinned Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let engine = crate::jobs::JobEngine::new(storage);
        let ai_cfg = crate::ai::AiJobConfig {
            enabled: true,
            model_id: "hash-pinned-model".to_string(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: PreprocessConfig::default(),
            postprocessing: None,
            frame_sampling: crate::ai::FrameSamplingConfig::all(),
            output_mode: crate::ai::AiFrameOutputMode::Image,
        };

        let job = engine
            .create_ai_job("proj-hash-test", None, vec![], ai_cfg)
            .unwrap();
        let job_ai = job.ai_config.unwrap();
        assert_eq!(job_ai.model_hash, Some(expected_sha256));
    }

    #[test]
    fn test_phase6f_26_model_package_validation_report() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "report-model",
                "Report Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let report = registry.validate_package("report-model", "1.0.0").unwrap();
        assert!(report.valid);
        assert!(report.integrity_valid);
        assert!(report.onnx_valid);
        assert!(report.profile_valid);
        assert_eq!(report.model_id, "report-model");
        assert_eq!(report.version, "1.0.0");
    }

    #[test]
    fn test_phase6f_27_path_traversal_rejection() {
        assert!(validate_model_id("../malicious").is_err());
        assert!(validate_model_id("models/escape").is_err());
        assert!(validate_model_id("..\\windows\\traversal").is_err());
        assert!(validate_model_id("C:nested").is_err());
        assert!(validate_model_id("valid-model_id-123").is_ok());
    }

    #[test]
    fn test_phase6f_28_invalid_model_id_rejection() {
        assert!(validate_model_id("").is_err());
        assert!(validate_model_id("   ").is_err());
        assert!(validate_model_id("invalid spaces").is_err());
        assert!(validate_model_id("invalid$symbol").is_err());
    }

    #[test]
    fn test_phase6f_29_invalid_version_rejection() {
        assert!(validate_version_str("abc").is_err());
        assert!(validate_version_str("1.0").is_ok());
        assert!(validate_version_str("1.0.0.0").is_err());
        assert!(validate_version_str("1.0.0").is_ok());
        assert!(validate_version_str("v2.1.0").is_ok());
    }

    #[test]
    fn test_phase6f_30_backward_compatibility_with_existing_model_registry() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "legacy.onnx");

        let manifest = AiModelManifest::new(
            "legacy-manifest-model",
            "Legacy Model",
            "1.0.0",
            ModelFormat::Onnx,
            src_onnx,
            "Legacy Manifest Desc",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        // Calling legacy register_model
        let registered = registry.register_model(manifest).unwrap();
        assert_eq!(registered.id, "legacy-manifest-model");

        // Should be accessible via get_model and list_models
        let m = registry.get_model("legacy-manifest-model").unwrap();
        assert_eq!(m.id, "legacy-manifest-model");

        // Should also be mapped into active package
        let pkg = registry
            .get_active_package("legacy-manifest-model")
            .unwrap();
        assert_eq!(pkg.model_id, "legacy-manifest-model");
    }

    #[test]
    fn test_phase6f_31_full_import_validate_activate_flow() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        // 1. Import
        let pkg = registry
            .import_model(
                &src_onnx,
                "end-to-end-model",
                "End To End Model",
                "1.0.0",
                "E2E",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        // 2. Validate
        let report = registry
            .validate_package(&pkg.model_id, &pkg.version)
            .unwrap();
        assert!(report.valid);

        // 3. Activate
        let activated = registry
            .activate_version(&pkg.model_id, &pkg.version)
            .unwrap();
        assert_eq!(activated.version, "1.0.0");
    }

    #[test]
    fn test_phase6f_32_activate_rollback_flow() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx = create_test_onnx_file(temp.path(), "model.onnx");
        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let _ = registry
            .import_model(
                &src_onnx,
                "flow-model",
                "Flow Model",
                "1.0.0",
                "v1.0",
                "Desc",
                profile.clone(),
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let _ = registry
            .import_model(
                &src_onnx,
                "flow-model",
                "Flow Model",
                "1.1.0",
                "v1.1",
                "Desc",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let _ = registry.activate_version("flow-model", "1.1.0").unwrap();
        assert_eq!(
            registry.get_active_package("flow-model").unwrap().version,
            "1.1.0"
        );

        let rolled = registry.rollback_model("flow-model").unwrap();
        assert_eq!(rolled.version, "1.0.0");
        assert_eq!(
            registry.get_active_package("flow-model").unwrap().version,
            "1.0.0"
        );
    }

    #[test]
    fn test_phase6f_33_model_version_isolation() {
        let temp = tempdir().unwrap();
        let models_dir = temp.path().join("models");
        let registry = ModelRegistry::new(models_dir);

        let src_onnx1 = create_test_onnx_file(temp.path(), "model_v1.onnx");
        let src_onnx2 = create_test_onnx_file(temp.path(), "model_v2.onnx");

        let mut profile = AiModelProfile::default();
        profile.input.target_width = 2;
        profile.input.target_height = 2;
        profile.input.channel_order = ChannelOrder::Rgb;
        profile.input.layout = TensorLayout::Nchw;

        let pkg1 = registry
            .import_model(
                &src_onnx1,
                "isolated-model",
                "Isolated Model",
                "1.0.0",
                "v1.0",
                "Desc 1",
                profile.clone(),
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        let pkg2 = registry
            .import_model(
                &src_onnx2,
                "isolated-model",
                "Isolated Model",
                "2.0.0",
                "v2.0",
                "Desc 2",
                profile,
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap();

        assert_ne!(pkg1.model_file, pkg2.model_file);
        assert_ne!(pkg1.version, pkg2.version);
        assert!(pkg1.model_file.exists());
        assert!(pkg2.model_file.exists());
    }

    #[test]
    fn test_phase6f_34_profile_hash_stability() {
        let mut p1 = AiModelProfile::default();
        p1.input.target_width = 640;
        p1.input.target_height = 480;

        let mut p2 = AiModelProfile::default();
        p2.input.target_width = 640;
        p2.input.target_height = 480;

        assert_eq!(p1.compute_profile_hash(), p2.compute_profile_hash());

        p2.input.target_height = 640;
        assert_ne!(p1.compute_profile_hash(), p2.compute_profile_hash());
    }

    #[test]
    fn test_phase6f_35_real_onnx_metadata_compatibility() {
        let temp = tempdir().unwrap();
        let model_1d = create_test_1d_onnx_file(temp.path(), "math_1d.onnx");
        let meta_1d = OnnxAiRuntime::inspect_onnx_file(&model_1d).unwrap();

        assert_eq!(meta_1d.input_count, 1);
        assert_eq!(meta_1d.output_count, 1);
        assert_eq!(meta_1d.inputs[0].name, "X");
        assert_eq!(meta_1d.outputs[0].name, "Y");
    }
}
