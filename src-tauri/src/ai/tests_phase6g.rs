#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    use crate::ai::frame_pipeline::config::{
        AiFrameOutputMode, AiJobConfig, FrameSamplingConfig, FrameSamplingMode,
    };
    use crate::ai::manifest::{AiModelManifest, ModelFormat, ModelRequirements};
    use crate::ai::onnx::generate_minimal_onnx_model;
    use crate::ai::package::{calculate_file_sha256, AiModelPackage};
    use crate::ai::pipeline::{
        ChannelOrder, NormalizationConfig, PreprocessConfig, ResizeFilter, TensorLayout,
    };
    use crate::ai::preflight::{
        validate_ai_job_preflight, AiJobPreflightReport, PreflightCheckResult,
        PreflightCheckSeverity, PreflightCheckStatus,
    };
    use crate::ai::profile::{
        AiModelProfile, AspectHandling, InputProfile, OutputInterpretationType, OutputProfile,
    };
    use crate::ai::provider::ExecutionProvider;
    use crate::ai::registry::ModelRegistry;
    use crate::ai::resolver::{ProductionModelResolver, ResolvedProductionModel};
    use crate::ai::tensor::TensorDataType;
    use crate::error::{AppError, ErrorCode};
    use crate::jobs::{JobEngine, JobStatus};
    use crate::system::StoragePaths;

    fn make_storage_paths(temp: &TempDir) -> StoragePaths {
        StoragePaths {
            app_data_dir: temp.path().join("app"),
            projects_dir: temp.path().join("projects"),
            cache_dir: temp.path().join("cache"),
            temp_dir: temp.path().join("temp"),
            models_dir: temp.path().join("models"),
            logs_dir: temp.path().join("logs"),
        }
    }

    fn sample_preprocess_config(w: u32, h: u32) -> PreprocessConfig {
        PreprocessConfig {
            target_width: w,
            target_height: h,
            resize_filter: ResizeFilter::Bilinear,
            letterbox: false,
            letterbox_pad: [114, 114, 114],
            center_crop: false,
            crop_width: None,
            crop_height: None,
            channel_order: ChannelOrder::Rgb,
            normalization: NormalizationConfig::zero_to_one(),
            layout: TensorLayout::Nchw,
            batch_size: 1,
        }
    }

    fn create_test_profile(w: u32, h: u32, out_type: OutputInterpretationType) -> AiModelProfile {
        AiModelProfile {
            input: InputProfile {
                target_width: w,
                target_height: h,
                channel_order: ChannelOrder::Rgb,
                color_space: "sRGB".to_string(),
                layout: TensorLayout::Nchw,
                normalization: NormalizationConfig::zero_to_one(),
                resize_filter: ResizeFilter::Bilinear,
                aspect_handling: AspectHandling::Stretch,
                tensor_name: None,
                data_type: TensorDataType::Float32,
            },
            output: OutputProfile {
                output_type: out_type,
                tensor_name: None,
                layout: Some(TensorLayout::Nchw),
                threshold: None,
                mask_interpretation: None,
                bbox_interpretation: None,
                coordinate_restoration: false,
            },
        }
    }

    /// Helper to create a fully initialized test package on disk
    fn setup_test_package(
        models_dir: &Path,
        model_id: &str,
        version: &str,
        is_4d_image: bool,
    ) -> AiModelPackage {
        let pkg_dir = models_dir.join(model_id).join(version);
        fs::create_dir_all(&pkg_dir).unwrap();

        let model_path = pkg_dir.join("model.onnx");
        if is_4d_image {
            let weight = if version == "2.0.0" { 3.0 } else { 2.0 };
            crate::ai::generate_image_onnx_model_with_weight(&model_path, weight).unwrap();
        } else {
            generate_minimal_onnx_model(&model_path).unwrap();
        }

        let sha256 = calculate_file_sha256(&model_path).unwrap();
        let file_size = fs::metadata(&model_path).unwrap().len();

        let profile = if is_4d_image {
            create_test_profile(2, 2, OutputInterpretationType::Image)
        } else {
            create_test_profile(4, 1, OutputInterpretationType::Image)
        };

        let manifest = AiModelManifest::new(
            format!("{}:{}", model_id, version),
            format!("Test {}", model_id),
            version,
            ModelFormat::Onnx,
            model_path.clone(),
            "Test Model",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let package = AiModelPackage::new(
            model_id,
            format!("Test {}", model_id),
            version,
            format!("Test {} v{}", model_id, version),
            "Test Model Package",
            ModelFormat::Onnx,
            model_path,
            file_size,
            sha256,
            manifest,
            profile,
            ModelRequirements::default(),
            vec![ExecutionProvider::Cpu, ExecutionProvider::DirectML],
        )
        .unwrap();

        let registry = ModelRegistry::new(models_dir.to_path_buf());
        registry.register_package(package.clone()).unwrap();
        registry.activate_version(model_id, version).unwrap();

        package
    }

    /// Helper to create a dummy test video container
    fn setup_dummy_video(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut fake_mp4 = vec![0u8; 128];
        fake_mp4[4] = b'f';
        fake_mp4[5] = b't';
        fake_mp4[6] = b'y';
        fake_mp4[7] = b'p';
        fs::write(path, fake_mp4).unwrap();
    }

    // =========================================================================
    // 1. Model Resolution Tests
    // =========================================================================

    #[test]
    fn test_resolve_model_active_version() {
        let temp = TempDir::new().unwrap();
        let pkg = setup_test_package(temp.path(), "person-segmenter", "1.0.0", true);
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        let resolved =
            ProductionModelResolver::resolve_model(&registry, Some("person-segmenter"), None, None)
                .unwrap();

        assert_eq!(resolved.model_id, "person-segmenter");
        assert_eq!(resolved.model_version, "1.0.0");
        assert_eq!(resolved.model_hash, pkg.sha256);
        assert!(resolved.supported_providers.contains(&resolved.provider));
        assert_eq!(resolved.file_size_bytes, pkg.file_size_bytes);
    }

    #[test]
    fn test_resolve_model_explicit_version() {
        let temp = TempDir::new().unwrap();
        let _v1 = setup_test_package(temp.path(), "style-transfer", "1.0.0", true);
        let v2 = setup_test_package(temp.path(), "style-transfer", "2.0.0", true);
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        let resolved = ProductionModelResolver::resolve_model(
            &registry,
            Some("style-transfer"),
            Some("2.0.0"),
            None,
        )
        .unwrap();

        assert_eq!(resolved.model_version, "2.0.0");
        assert_eq!(resolved.model_hash, v2.sha256);
    }

    #[test]
    fn test_resolve_model_missing_id() {
        let temp = TempDir::new().unwrap();
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        let err = ProductionModelResolver::resolve_model(&registry, None, None, None).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);

        let err2 =
            ProductionModelResolver::resolve_model(&registry, Some("  "), None, None).unwrap_err();
        assert_eq!(err2.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn test_resolve_model_not_found() {
        let temp = TempDir::new().unwrap();
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        let err = ProductionModelResolver::resolve_model(
            &registry,
            Some("non-existent-model"),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::ModelNotActive);
    }

    #[test]
    fn test_resolve_model_version_not_found() {
        let temp = TempDir::new().unwrap();
        setup_test_package(temp.path(), "super-res", "1.0.0", true);
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        let err = ProductionModelResolver::resolve_model(
            &registry,
            Some("super-res"),
            Some("9.9.9"),
            None,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::ModelVersionNotFound);
    }

    #[test]
    fn test_resolve_model_missing_file() {
        let temp = TempDir::new().unwrap();
        let pkg = setup_test_package(temp.path(), "inpaint", "1.0.0", true);
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        fs::remove_file(&pkg.model_file).unwrap();

        let err = ProductionModelResolver::resolve_model(&registry, Some("inpaint"), None, None)
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::FileNotFound);
    }

    #[test]
    fn test_resolve_model_zero_byte_file() {
        let temp = TempDir::new().unwrap();
        let pkg = setup_test_package(temp.path(), "zero-model", "1.0.0", true);
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        fs::write(&pkg.model_file, b"").unwrap();

        let err = ProductionModelResolver::resolve_model(&registry, Some("zero-model"), None, None)
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn test_resolve_model_hash_mismatch() {
        let temp = TempDir::new().unwrap();
        let pkg = setup_test_package(temp.path(), "tampered-model", "1.0.0", true);
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        fs::write(&pkg.model_file, b"tampered byte stream").unwrap();

        let err =
            ProductionModelResolver::resolve_model(&registry, Some("tampered-model"), None, None)
                .unwrap_err();
        assert_eq!(err.code, ErrorCode::ModelIntegrityMismatch);
    }

    #[test]
    fn test_resolve_model_invalid_onnx_graph() {
        let temp = TempDir::new().unwrap();
        let pkg = setup_test_package(temp.path(), "corrupt-graph", "1.0.0", true);
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        // Corrupt ONNX file on disk while keeping valid registration
        fs::write(&pkg.model_file, b"invalid non-onnx content").unwrap();

        let err =
            ProductionModelResolver::resolve_model(&registry, Some("corrupt-graph"), None, None)
                .unwrap_err();
        assert!(matches!(
            err.code,
            ErrorCode::ModelIntegrityMismatch | ErrorCode::ModelGraphInvalid
        ));
    }

    #[test]
    fn test_resolve_model_provider_unsupported_by_model() {
        let temp = TempDir::new().unwrap();
        setup_test_package(temp.path(), "cpu-only-model", "1.0.0", true);
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        let err = ProductionModelResolver::resolve_model(
            &registry,
            Some("cpu-only-model"),
            None,
            Some(ExecutionProvider::Cuda),
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::ModelProviderUnsupported);
    }

    #[test]
    fn test_resolve_model_profile_hash_deterministic() {
        let temp = TempDir::new().unwrap();
        let pkg = setup_test_package(temp.path(), "det-model", "1.0.0", true);
        let registry = ModelRegistry::new(temp.path().to_path_buf());

        let resolved =
            ProductionModelResolver::resolve_model(&registry, Some("det-model"), None, None)
                .unwrap();

        let expected_hash = pkg.profile.compute_profile_hash();
        assert_eq!(resolved.profile_hash, expected_hash);
        assert!(!resolved.profile_hash.is_empty());
    }

    // =========================================================================
    // 2. Preflight Validation Engine Tests
    // =========================================================================

    #[test]
    fn test_preflight_valid_pipeline() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let pkg = setup_test_package(&storage_paths.models_dir, "face-enhancer", "1.0.0", true);

        let video_path = temp.path().join("input.mp4");
        setup_dummy_video(&video_path);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "face-enhancer".to_string(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig {
                mode: FrameSamplingMode::All,
                nth: None,
                start: None,
                end: None,
            },
            output_mode: AiFrameOutputMode::Image,
        };

        let report = validate_ai_job_preflight(&video_path, &ai_config, &storage_paths).unwrap();
        assert!(report.resolved_model.is_some());
        assert_eq!(report.resolved_model.unwrap().model_id, "face-enhancer");
    }

    #[test]
    fn test_preflight_missing_source_file() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        setup_test_package(&storage_paths.models_dir, "seg-model", "1.0.0", true);

        let missing_video = temp.path().join("does_not_exist.mp4");

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "seg-model".to_string(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let report = validate_ai_job_preflight(&missing_video, &ai_config, &storage_paths).unwrap();
        assert!(!report.is_valid);
        assert!(report
            .checks
            .iter()
            .any(|c| c.check == "SOURCE_FILE_EXISTS" && c.status == PreflightCheckStatus::Fail));
        assert!(report.errors.iter().any(|e| e.contains("does not exist")));
    }

    #[test]
    fn test_preflight_unsupported_source_format() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        setup_test_package(&storage_paths.models_dir, "seg-model", "1.0.0", true);

        let txt_file = temp.path().join("video.txt");
        fs::write(&txt_file, b"not a video").unwrap();

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "seg-model".to_string(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let report = validate_ai_job_preflight(&txt_file, &ai_config, &storage_paths).unwrap();
        assert!(!report.is_valid);
        assert!(report
            .checks
            .iter()
            .any(|c| c.check == "SOURCE_MEDIA_FORMAT" && c.status == PreflightCheckStatus::Fail));
    }

    #[test]
    fn test_preflight_unresolved_model() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let video_path = temp.path().join("video.mp4");
        setup_dummy_video(&video_path);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "non-existent-pkg".to_string(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let report = validate_ai_job_preflight(&video_path, &ai_config, &storage_paths).unwrap();
        assert!(!report.is_valid);
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.check == "MODEL_PACKAGE_RESOLVED"
                    && c.status == PreflightCheckStatus::Fail)
        );
    }

    #[test]
    fn test_preflight_profile_geometry_warn() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        setup_test_package(&storage_paths.models_dir, "geom-warn-model", "1.0.0", true);

        let video_path = temp.path().join("video.mp4");
        setup_dummy_video(&video_path);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "geom-warn-model".to_string(),
            model_version: Some("1.0.0".to_string()),
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: sample_preprocess_config(640, 640),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let report = validate_ai_job_preflight(&video_path, &ai_config, &storage_paths).unwrap();
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.check == "PREPROCESSING_GEOMETRY"
                    && c.status == PreflightCheckStatus::Warn)
        );
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("Preprocessing resolution")));
    }

    // =========================================================================
    // 3. Job Creation & Immutable Model Pinning Tests
    // =========================================================================

    #[test]
    fn test_create_ai_job_pins_model_metadata() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);

        let pkg = setup_test_package(
            &storage_paths.models_dir,
            "segmentation-pinned",
            "1.2.0",
            true,
        );
        let engine = JobEngine::new(storage_paths);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "segmentation-pinned".to_string(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let job = engine
            .create_ai_job_with_app::<tauri::Wry>(
                None,
                "project-test-1",
                None,
                vec!["input.mp4".to_string()],
                ai_config,
            )
            .unwrap();

        let pinned = job.ai_config.expect("Job must contain ai_config");
        assert_eq!(pinned.model_id, "segmentation-pinned");
        assert_eq!(pinned.model_version, Some("1.2.0".to_string()));
        assert_eq!(pinned.model_hash, Some(pkg.sha256.clone()));
        assert_eq!(
            pinned.profile_hash,
            Some(pkg.profile.compute_profile_hash())
        );
        assert!(pinned.provider.is_some());
        assert!(pkg.supported_providers.contains(&pinned.provider.unwrap()));
    }

    #[test]
    fn test_job_manifest_immutable_after_registry_version_bump() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);

        // v1 is 4D image model
        let pkg_v1 =
            setup_test_package(&storage_paths.models_dir, "versioned-model", "1.0.0", true);
        let engine = JobEngine::new(storage_paths.clone());

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "versioned-model".to_string(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let job_v1 = engine
            .create_ai_job_with_app::<tauri::Wry>(
                None,
                "project-1",
                None,
                vec!["input.mp4".to_string()],
                ai_config,
            )
            .unwrap();

        // v2 is 4D image model with different SHA-256
        let pkg_v2 =
            setup_test_package(&storage_paths.models_dir, "versioned-model", "2.0.0", true);
        assert_ne!(pkg_v1.sha256, pkg_v2.sha256);

        let loaded_job = engine.get_job(&job_v1.id).unwrap();
        let loaded_ai_config = loaded_job.ai_config.unwrap();

        assert_eq!(loaded_ai_config.model_version, Some("1.0.0".to_string()));
        assert_eq!(loaded_ai_config.model_hash, Some(pkg_v1.sha256));
    }

    #[test]
    fn test_job_manifest_immutable_after_registry_rollback() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);

        let _v1 = setup_test_package(&storage_paths.models_dir, "rollback-model", "1.0.0", true);
        let v2 = setup_test_package(&storage_paths.models_dir, "rollback-model", "2.0.0", true);
        let engine = JobEngine::new(storage_paths.clone());

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "rollback-model".to_string(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let job_v2 = engine
            .create_ai_job_with_app::<tauri::Wry>(
                None,
                "project-rollback",
                None,
                vec!["input.mp4".to_string()],
                ai_config,
            )
            .unwrap();

        let registry = ModelRegistry::new(storage_paths.models_dir.clone());
        registry.rollback_model("rollback-model").unwrap();

        let loaded_job = engine.get_job(&job_v2.id).unwrap();
        let loaded_ai_config = loaded_job.ai_config.unwrap();

        assert_eq!(loaded_ai_config.model_version, Some("2.0.0".to_string()));
        assert_eq!(loaded_ai_config.model_hash, Some(v2.sha256));
    }

    // =========================================================================
    // 4. Backward Compatibility & Serialization Tests
    // =========================================================================

    #[test]
    fn test_non_ai_job_creation_unaffected() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let engine = JobEngine::new(storage_paths);

        let job = engine
            .create_job_with_app::<tauri::Wry>(
                None,
                "proj-legacy",
                Some("video_pipeline".to_string()),
                vec!["legacy.mp4".to_string()],
            )
            .unwrap();

        assert_eq!(job.stages.len(), 6);
        assert!(job.ai_config.is_none());
        assert_eq!(job.status, JobStatus::Queued);
    }

    #[test]
    fn test_ai_job_creation_creates_7_stages() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);

        setup_test_package(
            &storage_paths.models_dir,
            "seven-stage-model",
            "1.0.0",
            true,
        );
        let engine = JobEngine::new(storage_paths);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "seven-stage-model".to_string(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let job = engine
            .create_ai_job_with_app::<tauri::Wry>(
                None,
                "proj-ai-7",
                None,
                vec!["input.mp4".to_string()],
                ai_config,
            )
            .unwrap();

        assert_eq!(job.stages.len(), 7);
        assert_eq!(job.stages[0].id, "stage_1_input_validation");
        assert_eq!(job.stages[1].id, "stage_2_media_probe");
        assert_eq!(job.stages[2].id, "stage_3_frame_extraction");
        assert_eq!(job.stages[3].id, "stage_4_audio_extraction");
        assert_eq!(job.stages[4].id, "stage_ai_frame_inference");
        assert_eq!(job.stages[5].id, "stage_5_video_reconstruction");
        assert_eq!(job.stages[6].id, "stage_6_output_validation");
    }

    #[test]
    fn test_preflight_report_serialization() {
        let report = AiJobPreflightReport {
            is_valid: true,
            checks: vec![PreflightCheckResult::pass("SOURCE_EXISTS", "File exists")],
            resolved_model: None,
            warnings: vec![],
            errors: vec![],
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"isValid\":true"));
        assert!(json.contains("\"checks\":["));
        assert!(json.contains("\"SOURCE_EXISTS\""));
    }

    #[test]
    fn test_resolved_production_model_serialization() {
        let manifest = AiModelManifest::new(
            "mod-1:1.0.0",
            "Mod One",
            "1.0.0",
            ModelFormat::Onnx,
            PathBuf::from("D:/models/model.onnx"),
            "Mod Description",
            vec![],
            vec![],
            ModelRequirements::default(),
        );

        let model = ResolvedProductionModel {
            model_id: "mod-1".to_string(),
            model_version: "1.0.0".to_string(),
            model_name: "Mod One".to_string(),
            display_name: "Mod One Pro".to_string(),
            model_path: PathBuf::from("D:/models/model.onnx"),
            model_hash: "abcdef123456".to_string(),
            profile_hash: "prof123".to_string(),
            profile: create_test_profile(2, 2, OutputInterpretationType::Image),
            provider: ExecutionProvider::Cpu,
            manifest,
            file_size_bytes: 100,
            supported_providers: vec![ExecutionProvider::Cpu],
        };

        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("\"modelId\":\"mod-1\""));
        assert!(json.contains("\"modelVersion\":\"1.0.0\""));
        assert!(json.contains("\"modelHash\":\"abcdef123456\""));
        assert!(json.contains("\"provider\":\"CPU\""));
    }

    #[test]
    fn test_phase6g_error_codes_serialization() {
        let err = AppError::model_not_found("my-model");
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"MODEL_NOT_FOUND\""));

        let err2 = AppError::model_version_not_found("my-model", "2.0.0");
        let json2 = serde_json::to_string(&err2).unwrap();
        assert!(json2.contains("\"MODEL_VERSION_NOT_FOUND\""));

        let err3 = AppError::model_not_active("my-model");
        let json3 = serde_json::to_string(&err3).unwrap();
        assert!(json3.contains("\"MODEL_NOT_ACTIVE\""));

        let err4 = AppError::model_provider_unsupported("my-model", "CUDA");
        let json4 = serde_json::to_string(&err4).unwrap();
        assert!(json4.contains("\"MODEL_PROVIDER_UNSUPPORTED\""));

        let err5 = AppError::provider_unavailable("CUDA", "Drivers missing");
        let json5 = serde_json::to_string(&err5).unwrap();
        assert!(json5.contains("\"PROVIDER_UNAVAILABLE\""));

        let err6 = AppError::model_graph_invalid("my-model", "Corrupt file");
        let json6 = serde_json::to_string(&err6).unwrap();
        assert!(json6.contains("\"MODEL_GRAPH_INVALID\""));

        let err7 = AppError::preflight_failed("Check failed", "Missing video");
        let json7 = serde_json::to_string(&err7).unwrap();
        assert!(json7.contains("\"PREFLIGHT_FAILED\""));

        let err8 = AppError::ai_job_configuration_invalid("Config invalid", "Bad params");
        let json8 = serde_json::to_string(&err8).unwrap();
        assert!(json8.contains("\"AI_JOB_CONFIGURATION_INVALID\""));
    }

    #[test]
    fn test_preflight_check_result_constructors() {
        let pass = PreflightCheckResult::pass("CHECK_1", "All good");
        assert_eq!(pass.status, PreflightCheckStatus::Pass);
        assert_eq!(pass.severity, PreflightCheckSeverity::Info);
        assert!(pass.technical_detail.is_none());

        let warn = PreflightCheckResult::warn("CHECK_2", "Notice", Some("Detail".to_string()));
        assert_eq!(warn.status, PreflightCheckStatus::Warn);
        assert_eq!(warn.severity, PreflightCheckSeverity::Warning);
        assert_eq!(warn.technical_detail, Some("Detail".to_string()));

        let fail = PreflightCheckResult::fail("CHECK_3", "Error", Some("Crit".to_string()));
        assert_eq!(fail.status, PreflightCheckStatus::Fail);
        assert_eq!(fail.severity, PreflightCheckSeverity::Error);
        assert_eq!(fail.technical_detail, Some("Crit".to_string()));
    }

    #[test]
    fn test_calculate_job_config_hash_uniqueness_for_model_versions() {
        let prep = sample_preprocess_config(640, 640);
        let h1 = crate::ai::compute_ai_config_hash("test-model:1.0.0", &prep, None);
        let h2 = crate::ai::compute_ai_config_hash("test-model:2.0.0", &prep, None);
        assert_ne!(
            h1, h2,
            "Different model versions must yield distinct config hashes"
        );
    }
}
