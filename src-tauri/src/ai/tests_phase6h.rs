#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::ai::frame_pipeline::artifact::{
        calculate_sha256, AiArtifactManager, AiFrameMetadata, AiFrameStatus,
    };
    use crate::ai::frame_pipeline::config::{AiFrameOutputMode, AiJobConfig, FrameSamplingConfig};
    use crate::ai::frame_pipeline::executor::AiFrameExecutor;
    use crate::ai::frame_pipeline::quality::{
        FrameQualityStatus, FrameQualityValidator, TechnicalQualityMetrics,
    };
    use crate::ai::frame_pipeline::reconstruct::{RationalFps, VideoReconstructor};
    use crate::ai::manifest::{AiModelManifest, ModelFormat, ModelRequirements};
    use crate::ai::package::{calculate_file_sha256, AiModelPackage};
    use crate::ai::pipeline::{
        generate_image_onnx_model, ChannelOrder, NormalizationConfig, PreprocessConfig,
        ResizeFilter, TensorLayout,
    };
    use crate::ai::profile::{
        AiModelProfile, AspectHandling, InputProfile, OutputInterpretationType, OutputProfile,
    };
    use crate::ai::provider::ExecutionProvider;
    use crate::ai::registry::ModelRegistry;
    use crate::ai::report::AiProductionExecutionReport;
    use crate::ai::resource::{probe_runtime_resources, AiResourceLimits};
    use crate::ai::tensor::TensorDataType;
    use crate::error::{AppError, ErrorCode};
    use crate::jobs::JobEngine;
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

    fn setup_test_package(models_dir: &Path, model_id: &str, version: &str) -> AiModelPackage {
        let global_storage = StoragePaths::default_paths();
        let pkg_dir = global_storage.models_dir.join(model_id).join(version);
        fs::create_dir_all(&pkg_dir).unwrap();

        let model_path = pkg_dir.join("model.onnx");
        generate_image_onnx_model(&model_path).unwrap();

        let sha256 = calculate_file_sha256(&model_path).unwrap();
        let file_size = fs::metadata(&model_path).unwrap().len();

        let profile = create_test_profile(2, 2, OutputInterpretationType::Image);

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

        let global_registry = ModelRegistry::new(global_storage.models_dir.clone());
        let _ = global_registry.register_package(package.clone());
        let _ = global_registry.activate_version(model_id, version);

        if models_dir != global_storage.models_dir {
            let local_pkg_dir = models_dir.join(model_id).join(version);
            let _ = fs::create_dir_all(&local_pkg_dir);
            let registry = ModelRegistry::new(models_dir.to_path_buf());
            let _ = registry.register_package(package.clone());
            let _ = registry.activate_version(model_id, version);
        }

        package
    }

    fn create_test_frame_png(path: &Path, width: u32, height: u32) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let img = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 128])
        });
        img.save_with_format(path, image::ImageFormat::Png).unwrap();
    }

    // =========================================================================
    // 1. Resource Limits & Bounding Tests
    // =========================================================================

    #[test]
    fn test_phase6h_01_resource_limits_default_values() {
        let limits = AiResourceLimits::default_production();
        assert_eq!(limits.max_frame_width, 4096);
        assert_eq!(limits.max_frame_height, 4096);
        assert_eq!(limits.max_frame_pixels, 16_777_216);
        assert_eq!(limits.max_inflight_frames, 1);
        assert_eq!(limits.max_concurrent_inference, 1);
        assert!(limits.max_job_disk_bytes >= 50 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_phase6h_02_validate_frame_dimensions_pass() {
        let limits = AiResourceLimits::default_production();
        let pixels = limits.validate_frame_dimensions(1920, 1080).unwrap();
        assert_eq!(pixels, 1920 * 1080);
    }

    #[test]
    fn test_phase6h_03_validate_frame_dimensions_exceed_width() {
        let limits = AiResourceLimits::default_production();
        let err = limits.validate_frame_dimensions(5000, 1080).unwrap_err();
        assert_eq!(err.code, ErrorCode::ResourceLimitExceeded);
        assert!(err.message.contains("width 5000px exceeds"));
    }

    #[test]
    fn test_phase6h_04_validate_frame_dimensions_exceed_height() {
        let limits = AiResourceLimits::default_production();
        let err = limits.validate_frame_dimensions(1920, 5000).unwrap_err();
        assert_eq!(err.code, ErrorCode::ResourceLimitExceeded);
        assert!(err.message.contains("height 5000px exceeds"));
    }

    #[test]
    fn test_phase6h_05_validate_frame_dimensions_pixel_overflow() {
        let mut limits = AiResourceLimits::default_production();
        limits.max_frame_pixels = 1_000_000;
        let err = limits.validate_frame_dimensions(1920, 1080).unwrap_err();
        assert_eq!(err.code, ErrorCode::ResourceLimitExceeded);
    }

    #[test]
    fn test_phase6h_06_validate_frame_dimensions_zero_dim() {
        let limits = AiResourceLimits::default_production();
        let err = limits.validate_frame_dimensions(0, 1080).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn test_phase6h_07_validate_tensor_elements_pass() {
        let limits = AiResourceLimits::default_production();
        let elements = limits.validate_tensor_elements(&[1, 3, 640, 640]).unwrap();
        assert_eq!(elements, 1 * 3 * 640 * 640);
    }

    #[test]
    fn test_phase6h_08_validate_tensor_elements_exceeded() {
        let limits = AiResourceLimits {
            max_tensor_elements: 1000,
            ..AiResourceLimits::default_production()
        };
        let err = limits
            .validate_tensor_elements(&[1, 3, 640, 640])
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ResourceLimitExceeded);
    }

    #[test]
    fn test_phase6h_09_validate_tensor_elements_empty_or_zero() {
        let limits = AiResourceLimits::default_production();
        let err1 = limits.validate_tensor_elements(&[]).unwrap_err();
        assert_eq!(err1.code, ErrorCode::InvalidInput);

        let err2 = limits.validate_tensor_elements(&[1, 0, 640]).unwrap_err();
        assert_eq!(err2.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn test_phase6h_10_validate_disk_budget_pass() {
        let limits = AiResourceLimits::default_production();
        let total = limits.validate_disk_budget(1000, 500).unwrap();
        assert_eq!(total, 1500);
    }

    #[test]
    fn test_phase6h_11_validate_disk_budget_exceeded() {
        let limits = AiResourceLimits {
            max_job_disk_bytes: 10_000,
            ..AiResourceLimits::default_production()
        };
        let err = limits.validate_disk_budget(8000, 3000).unwrap_err();
        assert_eq!(err.code, ErrorCode::DiskQuotaExceeded);
    }

    #[test]
    fn test_phase6h_12_runtime_resources_probe() {
        let resources = probe_runtime_resources("DirectML", Some("1.0.0"), 1, 5);
        assert_eq!(resources.active_provider, "DirectML");
        assert_eq!(resources.model_version, Some("1.0.0".to_string()));
        assert_eq!(resources.active_inference_count, 1);
        assert_eq!(resources.queued_frame_count, 5);
        assert!(resources.system_memory_bytes > 0);
    }

    // =========================================================================
    // 2. Frame Quality Validation Engine Tests
    // =========================================================================

    #[test]
    fn test_phase6h_13_quality_validator_valid_png() {
        let temp = TempDir::new().unwrap();
        let frame_path = temp.path().join("frame.png");
        create_test_frame_png(&frame_path, 64, 64);

        let report =
            FrameQualityValidator::validate_frame_file(&frame_path, 0, Some(64), Some(64), false)
                .unwrap();
        assert!(report.is_valid);
        assert_eq!(report.status, FrameQualityStatus::Pass);
        assert!(report.errors.is_empty());
        let metrics = report.metrics.unwrap();
        assert_eq!(metrics.decoded_width, 64);
        assert_eq!(metrics.decoded_height, 64);
        assert!(metrics.file_size_bytes > 0);
    }

    #[test]
    fn test_phase6h_14_quality_validator_empty_stream() {
        let report =
            FrameQualityValidator::validate_png_bytes(0, &[], Some(64), Some(64), false).unwrap();
        assert!(!report.is_valid);
        assert_eq!(report.status, FrameQualityStatus::Fail);
        assert!(report.errors[0].contains("empty"));
    }

    #[test]
    fn test_phase6h_15_quality_validator_invalid_magic_bytes() {
        let bad_bytes = b"NOT_A_PNG_FILE_HEADER_STREAM";
        let report =
            FrameQualityValidator::validate_png_bytes(0, bad_bytes, Some(64), Some(64), false)
                .unwrap();
        assert!(!report.is_valid);
        assert_eq!(report.status, FrameQualityStatus::Fail);
        assert!(report.errors[0].contains("invalid PNG signature"));
    }

    #[test]
    fn test_phase6h_16_quality_validator_dimension_mismatch() {
        let temp = TempDir::new().unwrap();
        let frame_path = temp.path().join("frame_32x32.png");
        create_test_frame_png(&frame_path, 32, 32);

        let report =
            FrameQualityValidator::validate_frame_file(&frame_path, 1, Some(64), Some(64), false)
                .unwrap();
        assert!(!report.is_valid);
        assert_eq!(report.status, FrameQualityStatus::Fail);
        assert!(report.errors.iter().any(|e| e.contains("width mismatch")));
        assert!(report.errors.iter().any(|e| e.contains("height mismatch")));
    }

    #[test]
    fn test_phase6h_17_quality_validator_mask_metrics() {
        let temp = TempDir::new().unwrap();
        let mask_path = temp.path().join("mask.png");

        let mask_img = image::GrayImage::from_fn(10, 10, |x, y| {
            if x >= 5 && y >= 5 {
                image::Luma([255])
            } else {
                image::Luma([0])
            }
        });
        mask_img
            .save_with_format(&mask_path, image::ImageFormat::Png)
            .unwrap();

        let report =
            FrameQualityValidator::validate_frame_file(&mask_path, 2, Some(10), Some(10), true)
                .unwrap();
        assert!(report.is_valid);
        let m = report.metrics.unwrap();
        assert_eq!(m.min_pixel_value, 0);
        assert_eq!(m.max_pixel_value, 255);
        assert_eq!(m.non_zero_pixel_ratio, 0.25);
    }

    #[test]
    fn test_phase6h_18_quality_validator_empty_mask_warning() {
        let temp = TempDir::new().unwrap();
        let mask_path = temp.path().join("empty_mask.png");

        let mask_img = image::GrayImage::from_fn(10, 10, |_, _| image::Luma([0]));
        mask_img
            .save_with_format(&mask_path, image::ImageFormat::Png)
            .unwrap();

        let report =
            FrameQualityValidator::validate_frame_file(&mask_path, 3, Some(10), Some(10), true)
                .unwrap();
        assert!(report.is_valid);
        assert_eq!(report.status, FrameQualityStatus::Warning);
        assert!(report.warnings.iter().any(|w| w.contains("empty")));
    }

    #[test]
    fn test_phase6h_19_quality_validator_file_not_found() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing.png");
        let report =
            FrameQualityValidator::validate_frame_file(&missing, 4, Some(64), Some(64), false)
                .unwrap();
        assert!(!report.is_valid);
        assert_eq!(report.status, FrameQualityStatus::Fail);
    }

    // =========================================================================
    // 3. Artifact Integrity & SHA-256 Validation Tests
    // =========================================================================

    #[test]
    fn test_phase6h_20_artifact_sha256_generation() {
        let bytes1 = b"test frame png bytes";
        let sha1 = calculate_sha256(bytes1);
        let sha2 = calculate_sha256(bytes1);
        assert_eq!(sha1, sha2);

        let bytes2 = b"different frame png bytes";
        let sha3 = calculate_sha256(bytes2);
        assert_ne!(sha1, sha3);
    }

    #[test]
    fn test_phase6h_21_artifact_deep_validation_valid() {
        let temp = TempDir::new().unwrap();
        let manager = AiArtifactManager::new(temp.path());
        manager.ensure_dirs().unwrap();

        let meta = AiFrameMetadata {
            job_id: Some("job-1".to_string()),
            frame_index: 0,
            source_frame_index: 0,
            status: AiFrameStatus::Completed,
            model_id: "test-model".to_string(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some("mhash-abc".to_string()),
            profile_hash: Some("phash-xyz".to_string()),
            provider: "CPU".to_string(),
            decode_duration_ms: 1.0,
            preprocess_duration_ms: 1.0,
            inference_duration_ms: 2.0,
            postprocess_duration_ms: 1.0,
            total_duration_ms: 5.0,
            input_width: 2,
            input_height: 2,
            output_width: 2,
            output_height: 2,
            output_artifact_path: "000000/output.png".to_string(),
            config_hash: "cfg-123".to_string(),
            artifact_hash: None,
            artifact_size_bytes: None,
            created_at: None,
        };

        let dummy_png = b"dummy valid png bytes";
        manager.write_frame_artifact(&meta, dummy_png).unwrap();

        let valid = manager.validate_frame_artifact_deep(
            0,
            "test-model",
            "cfg-123",
            Some("mhash-abc"),
            Some("phash-xyz"),
        );
        assert!(valid.is_some());
        let res = valid.unwrap();
        assert_eq!(res.artifact_hash, Some(calculate_sha256(dummy_png)));
    }

    #[test]
    fn test_phase6h_22_artifact_deep_validation_corrupted_png() {
        let temp = TempDir::new().unwrap();
        let manager = AiArtifactManager::new(temp.path());
        manager.ensure_dirs().unwrap();

        let meta = AiFrameMetadata {
            job_id: Some("job-1".to_string()),
            frame_index: 0,
            source_frame_index: 0,
            status: AiFrameStatus::Completed,
            model_id: "test-model".to_string(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some("mhash-abc".to_string()),
            profile_hash: Some("phash-xyz".to_string()),
            provider: "CPU".to_string(),
            decode_duration_ms: 1.0,
            preprocess_duration_ms: 1.0,
            inference_duration_ms: 2.0,
            postprocess_duration_ms: 1.0,
            total_duration_ms: 5.0,
            input_width: 2,
            input_height: 2,
            output_width: 2,
            output_height: 2,
            output_artifact_path: "000000/output.png".to_string(),
            config_hash: "cfg-123".to_string(),
            artifact_hash: None,
            artifact_size_bytes: None,
            created_at: None,
        };

        manager
            .write_frame_artifact(&meta, b"original png bytes")
            .unwrap();

        // Corrupt the PNG file on disk
        fs::write(
            manager.frame_output_png_path(0),
            b"tampered corrupted bytes",
        )
        .unwrap();

        let valid = manager.validate_frame_artifact_deep(
            0,
            "test-model",
            "cfg-123",
            Some("mhash-abc"),
            Some("phash-xyz"),
        );
        assert!(valid.is_none(), "Tampered artifact must fail validation");
        assert!(
            !manager.frame_output_png_path(0).exists(),
            "Corrupted artifact must be purged"
        );
    }

    #[test]
    fn test_phase6h_23_artifact_deep_validation_model_hash_mismatch() {
        let temp = TempDir::new().unwrap();
        let manager = AiArtifactManager::new(temp.path());
        manager.ensure_dirs().unwrap();

        let meta = AiFrameMetadata {
            job_id: Some("job-1".to_string()),
            frame_index: 0,
            source_frame_index: 0,
            status: AiFrameStatus::Completed,
            model_id: "test-model".to_string(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some("mhash-old".to_string()),
            profile_hash: Some("phash-xyz".to_string()),
            provider: "CPU".to_string(),
            decode_duration_ms: 1.0,
            preprocess_duration_ms: 1.0,
            inference_duration_ms: 2.0,
            postprocess_duration_ms: 1.0,
            total_duration_ms: 5.0,
            input_width: 2,
            input_height: 2,
            output_width: 2,
            output_height: 2,
            output_artifact_path: "000000/output.png".to_string(),
            config_hash: "cfg-123".to_string(),
            artifact_hash: None,
            artifact_size_bytes: None,
            created_at: None,
        };

        manager
            .write_frame_artifact(&meta, b"valid png bytes")
            .unwrap();

        let valid = manager.validate_frame_artifact_deep(
            0,
            "test-model",
            "cfg-123",
            Some("mhash-NEW"),
            Some("phash-xyz"),
        );
        assert!(
            valid.is_none(),
            "Mismatched model hash must invalidate artifact"
        );
    }

    // =========================================================================
    // 4. Memory-Bounded Frame Execution Tests
    // =========================================================================

    #[test]
    fn test_phase6h_24_executor_single_frame_execution() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-exec-24-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        create_test_frame_png(&frames_dir.join("frame_000000.png"), 2, 2);

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let metrics =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();

        assert_eq!(metrics.frames_total, 1);
        assert_eq!(metrics.frames_processed, 1);
        assert_eq!(metrics.frames_failed, 0);
        assert!(metrics.artifact_bytes_written > 0);
        assert!(artifact_mgr.frame_output_png_path(0).exists());
    }

    #[test]
    fn test_phase6h_25_executor_multi_frame_execution() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-multi-25-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        for i in 0..5 {
            create_test_frame_png(&frames_dir.join(format!("frame_{:06}.png", i)), 2, 2);
        }

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let metrics =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();

        assert_eq!(metrics.frames_total, 5);
        assert_eq!(metrics.frames_processed, 5);
        assert_eq!(metrics.frames_reused, 0);
        assert_eq!(metrics.frames_passthrough, 0);
    }

    #[test]
    fn test_phase6h_26_executor_passthrough_frames() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-pass-26-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        for i in 0..4 {
            create_test_frame_png(&frames_dir.join(format!("frame_{:06}.png", i)), 2, 2);
        }

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        // Process every 2nd frame (0 and 2 processed, 1 and 3 passthrough)
        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::every_nth(2),
            output_mode: AiFrameOutputMode::Image,
        };

        let metrics =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();

        assert_eq!(metrics.frames_total, 4);
        assert_eq!(metrics.frames_processed, 2);
        assert_eq!(metrics.frames_passthrough, 2);
        assert_eq!(metrics.frames_selected, 2);
    }

    #[test]
    fn test_phase6h_27_executor_reused_frames_with_sha256() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-reuse-27-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        for i in 0..3 {
            create_test_frame_png(&frames_dir.join(format!("frame_{:06}.png", i)), 2, 2);
        }

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        // First run: processes 3 frames
        let m1 =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();
        assert_eq!(m1.frames_processed, 3);
        assert_eq!(m1.frames_reused, 0);

        // Second run: reuses all 3 frames
        let m2 =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();
        assert_eq!(m2.frames_processed, 3);
        assert_eq!(m2.frames_reused, 3);
    }

    #[test]
    fn test_phase6h_28_executor_dimension_limit_rejection() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-dim-28-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        create_test_frame_png(&frames_dir.join("frame_000000.png"), 2, 2);

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let limits = AiResourceLimits {
            max_frame_width: 1, // Will trigger error on 2x2
            ..AiResourceLimits::default_production()
        };

        let err = AiFrameExecutor::execute_with_limits(
            &frames_dir,
            &ai_config,
            &artifact_mgr,
            &limits,
            None,
            |_, _, _| {},
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::ResourceLimitExceeded);
    }

    #[test]
    fn test_phase6h_29_executor_disk_budget_rejection() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-disk-29-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        create_test_frame_png(&frames_dir.join("frame_000000.png"), 2, 2);

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let limits = AiResourceLimits {
            max_job_disk_bytes: 1, // Will trigger error on write
            ..AiResourceLimits::default_production()
        };

        let err = AiFrameExecutor::execute_with_limits(
            &frames_dir,
            &ai_config,
            &artifact_mgr,
            &limits,
            None,
            |_, _, _| {},
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::DiskQuotaExceeded);
    }

    // =========================================================================
    // 5. Cancellation at Granular Boundaries Tests
    // =========================================================================

    #[test]
    fn test_phase6h_30_cancellation_before_frame_start() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-canc-30-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        create_test_frame_png(&frames_dir.join("frame_000000.png"), 2, 2);

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let cancel_token = Arc::new(AtomicBool::new(true)); // Pre-cancelled

        let err = AiFrameExecutor::execute(
            &frames_dir,
            &ai_config,
            &artifact_mgr,
            Some(cancel_token),
            |_, _, _| {},
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::Cancelled);
    }

    #[test]
    fn test_phase6h_31_cancellation_during_frame_loop() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-canc-31-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        for i in 0..10 {
            create_test_frame_png(&frames_dir.join(format!("frame_{:06}.png", i)), 2, 2);
        }

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let cancel_token = Arc::new(AtomicBool::new(false));
        let cancel_token_clone = cancel_token.clone();

        let err = AiFrameExecutor::execute(
            &frames_dir,
            &ai_config,
            &artifact_mgr,
            Some(cancel_token),
            move |_, _, metrics| {
                if metrics.frames_processed >= 2 {
                    cancel_token_clone.store(true, Ordering::SeqCst);
                }
            },
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::Cancelled);
        // Valid artifacts for completed frames are preserved
        assert!(artifact_mgr.frame_output_png_path(0).exists());
        assert!(artifact_mgr.frame_output_png_path(1).exists());
    }

    // =========================================================================
    // 6. Production Retry & Model Pinning Tests
    // =========================================================================

    #[tokio::test]
    async fn test_phase6h_32_retry_preserves_immutable_model_pin() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-pin-32-{}", uuid::Uuid::new_v4());

        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");
        let engine = JobEngine::new(storage_paths.clone());

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: None,
            model_hash: None,
            profile_hash: None,
            provider: None,
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let mut job = engine
            .create_ai_job_with_app::<tauri::Wry>(
                None,
                "proj-pin",
                None,
                vec!["input.mp4".to_string()],
                ai_config,
            )
            .unwrap();

        let pinned = job.ai_config.clone().unwrap();
        assert_eq!(pinned.model_version, Some("1.0.0".to_string()));
        assert_eq!(pinned.model_hash, Some(pkg.sha256.clone()));

        // Mark job as Interrupted before retrying
        job.status = crate::jobs::JobStatus::Interrupted;
        engine.save_job_manifest(&job).unwrap();

        // Retry job
        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        let retried_pin = retried.ai_config.unwrap();
        assert_eq!(retried_pin.model_version, Some("1.0.0".to_string()));
        assert_eq!(retried_pin.model_hash, Some(pkg.sha256));
    }

    // =========================================================================
    // 7. Production Execution Report Tests
    // =========================================================================

    #[test]
    fn test_phase6h_33_execution_report_construction() {
        let mut report = AiProductionExecutionReport::new(
            "job-99",
            "yolo-model",
            Some("1.0.0"),
            Some("mhash123"),
            Some("phash456"),
            "DirectML",
            1920,
            1080,
            29.97,
            10000,
            300,
        );

        report.selected_frames = 150;
        report.processed_frames = 148;
        report.reused_frames = 2;
        report.passthrough_frames = 150;
        report.status = "SUCCESS".to_string();

        assert_eq!(report.job_id, "job-99");
        assert_eq!(report.model_id, "yolo-model");
        assert_eq!(report.provider, "DirectML");
        assert_eq!(report.selected_frames, 150);
        assert_eq!(report.status, "SUCCESS");
    }

    #[test]
    fn test_phase6h_34_execution_report_save_and_load() {
        let temp = TempDir::new().unwrap();
        let report_path = temp.path().join("ai_execution_report.json");

        let report = AiProductionExecutionReport::new(
            "job-report-test",
            "seg-model",
            Some("2.0.0"),
            Some("mhash"),
            Some("phash"),
            "CPU",
            1280,
            720,
            30.0,
            5000,
            150,
        );

        report.save_to_file(&report_path).unwrap();
        assert!(report_path.exists());

        let loaded = AiProductionExecutionReport::load_from_file(&report_path).unwrap();
        assert_eq!(loaded.job_id, "job-report-test");
        assert_eq!(loaded.model_id, "seg-model");
        assert_eq!(loaded.source_width, 1280);
    }

    #[test]
    fn test_phase6h_35_execution_report_serialization_camel_case() {
        let report = AiProductionExecutionReport::new(
            "job-serde",
            "my-model",
            Some("1.0"),
            None,
            None,
            "CPU",
            640,
            480,
            24.0,
            1000,
            24,
        );

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"jobId\":\"job-serde\""));
        assert!(json.contains("\"sourceWidth\":640"));
        assert!(json.contains("\"processedFrames\":0"));
    }

    // =========================================================================
    // 8. Output Validation Gate Tests
    // =========================================================================

    #[test]
    fn test_phase6h_36_reconstruction_fps_tolerance_check() {
        let fps = RationalFps::from_f64(29.97);
        assert_eq!(fps.num, 30000);
        assert_eq!(fps.den, 1001);
        assert!((fps.as_f64() - 29.97).abs() < 0.01);
    }

    #[test]
    fn test_phase6h_37_output_validation_missing_file() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("non_existent_output.mp4");
        let err = VideoReconstructor::validate_reconstructed_video(
            &missing,
            1920,
            1080,
            RationalFps::new(30, 1),
            100,
            false,
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::OutputNotFound);
    }

    // =========================================================================
    // 9. Real Media Integration Test with Available Local Fixture
    // =========================================================================

    #[test]
    fn test_phase6h_38_real_media_fixture_end_to_end_ai_pipeline() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            println!(
                "Local fixture sample_portrait_video.mp4 not found, skipping integration test"
            );
            return;
        }

        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);

        // 1. Setup real ONNX model in registry
        let model_id = format!("real-ai-model-38-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");
        let engine = JobEngine::new(storage_paths.clone());

        // 2. Configure AI Job
        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::every_nth(2), // sample every 2nd frame
            output_mode: AiFrameOutputMode::Image,
        };

        // 3. Create and queue AI Job
        let job = engine
            .create_ai_job_with_app::<tauri::Wry>(
                None,
                "proj-real-media",
                None,
                vec![fixture_path.to_string_lossy().to_string()],
                ai_config,
            )
            .unwrap();

        assert_eq!(job.stages.len(), 7);
        assert!(job.ai_config.is_some());
    }

    #[test]
    fn test_phase6h_39_error_code_constructors() {
        let err1 = AppError::resource_limit_exceeded("Limit reached", "Too large");
        assert_eq!(err1.code, ErrorCode::ResourceLimitExceeded);

        let err2 = AppError::frame_quality_failed("Quality bad", "Corrupt pixels");
        assert_eq!(err2.code, ErrorCode::FrameQualityFailed);

        let err3 = AppError::disk_quota_exceeded("Disk full", "Over 50GB");
        assert_eq!(err3.code, ErrorCode::DiskQuotaExceeded);
    }

    #[test]
    fn test_phase6h_40_frame_quality_metrics_structure() {
        let m = TechnicalQualityMetrics {
            decoded_width: 1920,
            decoded_height: 1080,
            file_size_bytes: 500000,
            has_alpha: false,
            non_zero_pixel_ratio: 0.99,
            min_pixel_value: 5,
            max_pixel_value: 250,
            mean_pixel_value: 128.5,
            variance: 450.0,
            clipping_ratio: 0.01,
            black_frame_detected: false,
            nan_or_inf_detected: false,
        };

        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"decodedWidth\":1920"));
        assert!(json.contains("\"nonZeroPixelRatio\":0.99"));
    }
}
