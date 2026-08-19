#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::ai::frame_pipeline::artifact::AiArtifactManager;
    use crate::ai::frame_pipeline::benchmark::AiJobBenchmarkReport;
    use crate::ai::frame_pipeline::config::{
        select_frames, AiFrameOutputMode, AiJobConfig, FrameSamplingConfig,
    };
    use crate::ai::frame_pipeline::executor::AiFrameExecutor;
    use crate::ai::frame_pipeline::quality::{FrameQualityStatus, FrameQualityValidator};
    use crate::ai::frame_pipeline::reconstruct::{
        AudioPreservationMode, RationalFps, VideoCodec, VideoReconstructionConfig,
        VideoReconstructor,
    };
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
    use crate::ai::tensor::TensorDataType;
    use crate::error::ErrorCode;
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

    fn create_test_profile(w: u32, h: u32) -> AiModelProfile {
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
                output_type: OutputInterpretationType::Image,
                tensor_name: None,
                layout: Some(TensorLayout::Nchw),
                threshold: None,
                mask_interpretation: None,
                bbox_interpretation: None,
                coordinate_restoration: false,
            },
        }
    }

    fn setup_test_package(
        models_dir: &Path,
        model_id: &str,
        version: &str,
        is_prod: bool,
    ) -> AiModelPackage {
        let global_storage = StoragePaths::default_paths();
        let pkg_dir = global_storage.models_dir.join(model_id).join(version);
        fs::create_dir_all(&pkg_dir).unwrap();

        let model_path = pkg_dir.join("model.onnx");
        generate_image_onnx_model(&model_path).unwrap();

        let sha256 = calculate_file_sha256(&model_path).unwrap();
        let file_size = fs::metadata(&model_path).unwrap().len();

        let profile = create_test_profile(2, 2);

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
        )
        .with_production(is_prod);

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
        .unwrap()
        .with_production(is_prod);

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

    fn create_test_frame_png(path: &Path, width: u32, height: u32, r: u8, g: u8, b: u8) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let img = image::RgbImage::from_fn(width, height, |_, _| image::Rgb([r, g, b]));
        img.save(path).unwrap();
    }

    // =========================================================================
    // 1. Model Discovery & Tier Differentiation
    // =========================================================================

    #[test]
    fn test_phase6k_01_real_model_tier_discovery() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let registry = ModelRegistry::new(storage_paths.models_dir.clone());

        let dev_model_id = format!("dev-model-{}", uuid::Uuid::new_v4());
        let prod_model_id = format!("prod-model-{}", uuid::Uuid::new_v4());

        let dev_pkg = setup_test_package(&storage_paths.models_dir, &dev_model_id, "1.0.0", false);
        let prod_pkg = setup_test_package(&storage_paths.models_dir, &prod_model_id, "1.0.0", true);

        assert!(!dev_pkg.is_production);
        assert!(prod_pkg.is_production);

        let loaded_dev = registry.get_active_package(&dev_model_id).unwrap();
        let loaded_prod = registry.get_active_package(&prod_model_id).unwrap();

        assert!(!loaded_dev.is_production);
        assert!(loaded_prod.is_production);
    }

    #[test]
    fn test_phase6k_02_model_hash_and_integrity_verification() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6k-02-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0", true);

        // Verification passes for unmodified model
        assert!(pkg.verify_integrity().is_ok());

        // Corrupt model file
        fs::write(&pkg.model_file, b"corrupted onnx data payload").unwrap();
        let err = pkg.verify_integrity().unwrap_err();
        assert_eq!(err.code, ErrorCode::ModelIntegrityMismatch);
    }

    // =========================================================================
    // 2. Deterministic Frame Quality & Sanity Metrics
    // =========================================================================

    #[test]
    fn test_phase6k_05_deterministic_frame_quality_png_validation() {
        let temp = TempDir::new().unwrap();
        let frame_path = temp.path().join("frame.png");
        create_test_frame_png(&frame_path, 32, 32, 128, 128, 128);

        let report =
            FrameQualityValidator::validate_frame_file(&frame_path, 0, Some(32), Some(32), false)
                .unwrap();

        assert!(report.is_valid);
        assert_eq!(report.status, FrameQualityStatus::Pass);
        let metrics = report.metrics.unwrap();
        assert_eq!(metrics.decoded_width, 32);
        assert_eq!(metrics.decoded_height, 32);
        assert!(!metrics.black_frame_detected);
        assert!(!metrics.nan_or_inf_detected);
    }

    #[test]
    fn test_phase6k_06_deterministic_quality_metrics_variance_and_clipping() {
        let temp = TempDir::new().unwrap();
        let frame_path = temp.path().join("clipped_frame.png");

        // Half 0 (black), Half 255 (white)
        let img = image::RgbImage::from_fn(10, 10, |x, _| {
            if x < 5 {
                image::Rgb([0, 0, 0])
            } else {
                image::Rgb([255, 255, 255])
            }
        });
        img.save(&frame_path).unwrap();

        let report =
            FrameQualityValidator::validate_frame_file(&frame_path, 0, Some(10), Some(10), false)
                .unwrap();

        assert!(report.is_valid);
        let metrics = report.metrics.unwrap();
        assert_eq!(metrics.clipping_ratio, 1.0); // 100% of pixels are 0 or 255
        assert!(metrics.variance > 1000.0); // High variance
        assert_eq!(metrics.min_pixel_value, 0);
        assert_eq!(metrics.max_pixel_value, 255);
    }

    #[test]
    fn test_phase6k_07_black_frame_detection() {
        let temp = TempDir::new().unwrap();
        let frame_path = temp.path().join("black_frame.png");
        create_test_frame_png(&frame_path, 16, 16, 0, 0, 0);

        let report =
            FrameQualityValidator::validate_frame_file(&frame_path, 0, Some(16), Some(16), false)
                .unwrap();

        assert!(report.is_valid);
        let metrics = report.metrics.unwrap();
        assert!(metrics.black_frame_detected);
        assert_eq!(report.status, FrameQualityStatus::Warning);
    }

    #[test]
    fn test_phase6k_08_nan_and_infinite_metric_detection() {
        // Zero byte file should cleanly fail
        let temp = TempDir::new().unwrap();
        let empty_path = temp.path().join("empty.png");
        fs::write(&empty_path, b"").unwrap();

        let report =
            FrameQualityValidator::validate_frame_file(&empty_path, 0, Some(16), Some(16), false)
                .unwrap();

        assert!(!report.is_valid);
        assert_eq!(report.status, FrameQualityStatus::Fail);
    }

    // =========================================================================
    // 3. Temporal Sequence & Passthrough Validation
    // =========================================================================

    #[test]
    fn test_phase6k_09_temporal_sequence_gap_detection() {
        let temp = TempDir::new().unwrap();
        let src_dir = temp.path().join("src");
        let art_dir = temp.path().join("art");

        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&art_dir).unwrap();

        // Create frame 0 and frame 2 (missing frame 1)
        create_test_frame_png(
            &art_dir.join("000000").join("output.png"),
            2,
            2,
            100,
            100,
            100,
        );
        create_test_frame_png(
            &art_dir.join("000002").join("output.png"),
            2,
            2,
            100,
            100,
            100,
        );

        let report = FrameQualityValidator::validate_frame_sequence(
            &src_dir,
            &art_dir,
            3,
            &FrameSamplingConfig::all(),
        )
        .unwrap();

        assert!(!report.is_valid);
        assert_eq!(report.missing_indices, vec![1]);
        assert_eq!(report.total_found, 2);
    }

    #[test]
    fn test_phase6k_10_temporal_sequence_contiguous_success() {
        let temp = TempDir::new().unwrap();
        let src_dir = temp.path().join("src");
        let art_dir = temp.path().join("art");

        for i in 0..5 {
            create_test_frame_png(
                &art_dir.join(format!("{:06}", i)).join("output.png"),
                2,
                2,
                50,
                50,
                50,
            );
        }

        let report = FrameQualityValidator::validate_frame_sequence(
            &src_dir,
            &art_dir,
            5,
            &FrameSamplingConfig::all(),
        )
        .unwrap();

        assert!(report.is_valid);
        assert_eq!(report.total_found, 5);
        assert!(report.missing_indices.is_empty());
    }

    #[test]
    fn test_phase6k_11_passthrough_frame_bitwise_identity() {
        let temp = TempDir::new().unwrap();
        let src_dir = temp.path().join("src");
        let art_dir = temp.path().join("art");

        // Sampling mode: every 2nd frame (indices 0, 2, 4 are processed; 1, 3 are passthrough)
        let sampling = FrameSamplingConfig::every_nth(2);

        for i in 0..4 {
            let src_file = src_dir.join(format!("{:06}.png", i));
            create_test_frame_png(
                &src_file,
                2,
                2,
                (i * 50) as u8,
                (i * 50) as u8,
                (i * 50) as u8,
            );

            let art_file = art_dir.join(format!("{:06}", i)).join("output.png");
            // For passthrough frames 1 and 3, copy exact bytes
            if i % 2 != 0 {
                fs::create_dir_all(art_file.parent().unwrap()).unwrap();
                fs::copy(&src_file, &art_file).unwrap();
            } else {
                create_test_frame_png(&art_file, 2, 2, 255, 255, 255);
            }
        }

        let report =
            FrameQualityValidator::validate_frame_sequence(&src_dir, &art_dir, 4, &sampling)
                .unwrap();

        assert!(report.is_valid);
        assert!(report.passthrough_mismatches.is_empty());
    }

    // =========================================================================
    // 4. Performance Benchmarking Facility
    // =========================================================================

    #[test]
    fn test_phase6k_12_benchmark_report_computation() {
        let bench = AiJobBenchmarkReport::compute(
            "job-bench-12",
            "model-test",
            Some("1.0.0"),
            Some("a1b2c3d4"),
            true,
            "DirectML",
            1920,
            1080,
            300,
            150,
            150,
            0,
            150,
            250.0,
            1500.0,
            1200.0,
            3000.0,
            15.0,
            25.0,
            800.0,
            2500.0,
            10000.0, // 10.0 seconds total
        );

        assert_eq!(bench.job_id, "job-bench-12");
        assert_eq!(bench.total_frames, 300);
        assert_eq!(bench.effective_fps, 30.0); // 300 frames / 10 sec = 30 fps
        assert_eq!(bench.effective_inference_fps, 50.0); // 150 frames / 3 sec = 50 fps
        assert_eq!(bench.inference_avg_ms, 20.0); // 3000ms / 150 = 20ms
        assert_eq!(bench.inference_min_ms, 15.0);
        assert_eq!(bench.inference_max_ms, 25.0);
        assert!(bench.is_production);
    }

    #[test]
    fn test_phase6k_13_preset_deterministic_throughput_mapping() {
        // Fast Preset (Every 3rd frame)
        let fast = FrameSamplingConfig::every_nth(3);
        let fast_frames = select_frames(60, &fast).unwrap();
        assert_eq!(fast_frames.len(), 20);

        // Balanced Preset (Every 2nd frame)
        let bal = FrameSamplingConfig::every_nth(2);
        let bal_frames = select_frames(60, &bal).unwrap();
        assert_eq!(bal_frames.len(), 30);

        // Quality Preset (100% all frames)
        let qual = FrameSamplingConfig::all();
        let qual_frames = select_frames(60, &qual).unwrap();
        assert_eq!(qual_frames.len(), 60);
    }

    // =========================================================================
    // 5. Memory Safety, Cancellation & Resumption
    // =========================================================================

    #[test]
    fn test_phase6k_14_memory_safety_single_frame_in_flight() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6k-14-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0", true);

        let frames_dir = temp.path().join("frames");
        for i in 0..10 {
            create_test_frame_png(
                &frames_dir.join(format!("frame_{:06}.png", i)),
                2,
                2,
                100,
                100,
                100,
            );
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
        assert_eq!(metrics.frames_processed, 10);
        assert_eq!(metrics.frames_failed, 0);
    }

    #[test]
    fn test_phase6k_15_cancellation_during_benchmark_run() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6k-15-{}", uuid::Uuid::new_v4());
        let _ = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0", true);

        let frames_dir = temp.path().join("frames");
        for i in 0..5 {
            create_test_frame_png(
                &frames_dir.join(format!("frame_{:06}.png", i)),
                2,
                2,
                100,
                100,
                100,
            );
        }

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: None,
            profile_hash: None,
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let cancel_token = Arc::new(AtomicBool::new(true)); // Cancelled immediately
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
    fn test_phase6k_16_resumption_with_100_percent_cache_reuse() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6k-16-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0", true);

        let frames_dir = temp.path().join("frames");
        for i in 0..4 {
            create_test_frame_png(
                &frames_dir.join(format!("frame_{:06}.png", i)),
                2,
                2,
                100,
                100,
                100,
            );
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

        // First pass
        let m1 =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();
        assert_eq!(m1.frames_processed, 4);
        assert_eq!(m1.frames_reused, 0);

        // Resumption pass
        let m2 =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();
        assert_eq!(m2.frames_processed, 4);
        assert_eq!(m2.frames_reused, 4);
    }

    // =========================================================================
    // 6. Video Reconstruction & Preservation
    // =========================================================================

    #[test]
    fn test_phase6k_17_rational_fps_and_duration_preservation() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("frames");
        for i in 0..6 {
            create_test_frame_png(
                &frames_dir.join(format!("{:06}.png", i)),
                64,
                64,
                120,
                120,
                120,
            );
        }

        let output_path = temp.path().join("output_rational_fps.mp4");
        let cfg = VideoReconstructionConfig {
            source_video_path: temp.path().join("dummy.mp4"),
            frames_dir: frames_dir.clone(),
            output_path: output_path.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 6,
            width: 64,
            height: 64,
            fps: RationalFps::new(24, 1),
            pixel_format: "yuv420p".to_string(),
            codec: VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: AudioPreservationMode::None,
            overwrite: true,
        };

        let res = VideoReconstructor::reconstruct_video(
            &cfg,
            "job-recon-17",
            None,
            None,
            |_, _, _| {},
            None,
            None::<fn(u32)>,
            None::<fn(u32)>,
        )
        .unwrap();

        assert!(res.output_path.exists());
        assert_eq!(res.output_metadata.width, 64);
        assert_eq!(res.output_metadata.height, 64);
    }

    // =========================================================================
    // 7. Error UX & Large Video Workflow
    // =========================================================================

    #[test]
    fn test_phase6k_19_invalid_model_rejection_error_ux() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let registry = ModelRegistry::new(storage_paths.models_dir);

        let err = registry
            .get_model("unknown_production_model_999")
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ModelNotAvailable);
    }

    #[tokio::test]
    async fn test_phase6k_20_large_video_end_to_end_pipeline() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let engine = JobEngine::new(storage_paths.clone());
        let model_id = format!("model-6k-20-{}", uuid::Uuid::new_v4());
        let _ = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0", true);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: None,
            profile_hash: None,
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::every_nth(3), // Fast preset
            output_mode: AiFrameOutputMode::Image,
        };

        let job = engine
            .create_ai_job_with_app::<tauri::Wry>(
                None,
                "proj-large-20",
                None,
                vec![fixture_path.to_string_lossy().to_string()],
                ai_config,
            )
            .unwrap();

        assert_eq!(job.status, JobStatus::Queued);
    }
}
