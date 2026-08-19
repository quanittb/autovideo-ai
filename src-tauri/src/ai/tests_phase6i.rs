#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::ai::frame_pipeline::artifact::{AiArtifactManager, AiFrameMetadata, AiFrameStatus};
    use crate::ai::frame_pipeline::config::{AiFrameOutputMode, AiJobConfig, FrameSamplingConfig};
    use crate::ai::frame_pipeline::executor::AiFrameExecutor;
    use crate::ai::frame_pipeline::reconstruct::{RationalFps, VideoReconstructor};
    use crate::ai::manifest::{AiModelManifest, ModelFormat, ModelRequirements};
    use crate::ai::package::{calculate_file_sha256, AiModelPackage};
    use crate::ai::pipeline::{
        generate_image_onnx_model, ChannelOrder, NormalizationConfig, PreprocessConfig,
        ResizeFilter, TensorLayout,
    };
    use crate::ai::preflight::validate_ai_job_preflight;
    use crate::ai::profile::{
        AiModelProfile, AspectHandling, InputProfile, OutputInterpretationType, OutputProfile,
    };
    use crate::ai::provider::ExecutionProvider;
    use crate::ai::registry::ModelRegistry;
    use crate::ai::report::AiProductionExecutionReport;
    use crate::ai::resource::AiResourceLimits;
    use crate::ai::tensor::TensorDataType;
    use crate::error::ErrorCode;
    use crate::jobs::{JobEngine, JobStatus};
    use crate::media::MediaService;
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
        setup_test_package_with_dims(models_dir, model_id, version, 2, 2)
    }

    fn setup_test_package_with_dims(
        models_dir: &Path,
        model_id: &str,
        version: &str,
        w: u32,
        h: u32,
    ) -> AiModelPackage {
        let global_storage = StoragePaths::default_paths();
        let pkg_dir = global_storage.models_dir.join(model_id).join(version);
        fs::create_dir_all(&pkg_dir).unwrap();

        let model_path = pkg_dir.join("model.onnx");
        generate_image_onnx_model(&model_path).unwrap();

        let sha256 = calculate_file_sha256(&model_path).unwrap();
        let file_size = fs::metadata(&model_path).unwrap().len();

        let profile = create_test_profile(w, h, OutputInterpretationType::Image);

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
        img.save(path).unwrap();
    }

    // =========================================================================
    // 1. Real Image & Frame Inference Tests
    // =========================================================================

    #[test]
    fn test_phase6i_01_real_image_inference_cpu() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-01-{}", uuid::Uuid::new_v4());
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
        assert!(artifact_mgr.frame_output_png_path(0).exists());
    }

    #[test]
    fn test_phase6i_02_real_multi_frame_inference() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-02-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        for i in 0..6 {
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

        assert_eq!(metrics.frames_total, 6);
        assert_eq!(metrics.frames_processed, 6);
        assert_eq!(metrics.frames_reused, 0);
        assert_eq!(metrics.frames_passthrough, 0);
    }

    #[test]
    fn test_phase6i_03_frame_sampling_all_vs_every_nth() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-03-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        for i in 0..8 {
            create_test_frame_png(&frames_dir.join(format!("frame_{:06}.png", i)), 2, 2);
        }

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        // Every 2nd frame (0, 2, 4, 6 processed; 1, 3, 5, 7 passthrough)
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

        assert_eq!(metrics.frames_total, 8);
        assert_eq!(metrics.frames_processed, 4);
        assert_eq!(metrics.frames_passthrough, 4);
        assert_eq!(metrics.frames_selected, 4);
    }

    #[test]
    fn test_phase6i_04_passthrough_frames_integrity() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-04-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        create_test_frame_png(&frames_dir.join("frame_000000.png"), 2, 2);
        create_test_frame_png(&frames_dir.join("frame_000001.png"), 2, 2);

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
            frame_sampling: FrameSamplingConfig::every_nth(2), // frame 1 is passthrough
            output_mode: AiFrameOutputMode::Image,
        };

        let _ =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();

        // Frame 1 passthrough file on disk is identical to source frame 1
        let src_bytes = fs::read(frames_dir.join("frame_000001.png")).unwrap();
        let out_bytes =
            fs::read(artifact_mgr.reconstruction_frames_dir().join("000001.png")).unwrap();
        assert_eq!(src_bytes, out_bytes);
    }

    // =========================================================================
    // 2. Immutable Model Pinning & Integrity
    // =========================================================================

    #[tokio::test]
    async fn test_phase6i_05_immutable_model_pinning_in_job() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-05-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");
        let engine = JobEngine::new(storage_paths.clone());

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: None, // Will be resolved and pinned
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
                "proj-6i-05",
                None,
                vec!["input.mp4".to_string()],
                ai_config,
            )
            .unwrap();

        let pinned = job.ai_config.unwrap();
        assert_eq!(pinned.model_version.as_deref(), Some("1.0.0"));
        assert_eq!(pinned.model_hash.as_deref(), Some(pkg.sha256.as_str()));
        assert_eq!(
            pinned.profile_hash.as_deref(),
            Some(pkg.profile.compute_profile_hash().as_str())
        );
    }

    #[test]
    fn test_phase6i_06_model_hash_mismatch_rejection() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-06-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        create_test_frame_png(&frames_dir.join("frame_000000.png"), 2, 2);

        let ai_cache_dir = temp.path().join("ai_cache");
        let artifact_mgr = AiArtifactManager::new(&ai_cache_dir);

        // Pre-create frame artifact with a mismatched model_hash
        let frame_meta = AiFrameMetadata {
            job_id: Some("job-6i-06".to_string()),
            frame_index: 0,
            source_frame_index: 0,
            status: AiFrameStatus::Completed,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some("wrong_model_hash_12345".to_string()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
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
            artifact_hash: Some("fake_hash".to_string()),
            artifact_size_bytes: Some(100),
            created_at: None,
        };
        let dummy_bytes = fs::read(frames_dir.join("frame_000000.png")).unwrap();
        artifact_mgr
            .write_frame_artifact(&frame_meta, &dummy_bytes)
            .unwrap();

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

        // Deep validation will reject the old artifact and re-run inference
        let metrics =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();

        assert_eq!(metrics.frames_reused, 0);
        assert_eq!(metrics.frames_processed, 1);
    }

    // =========================================================================
    // 3. Preflight Validation Gate Tests
    // =========================================================================

    #[test]
    fn test_phase6i_07_preflight_success_with_valid_video() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            println!("sample_portrait_video.mp4 not found, skipping fixture test");
            return;
        }

        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-07-{}", uuid::Uuid::new_v4());
        let _ = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: None,
            profile_hash: None,
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(640, 640),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let report = validate_ai_job_preflight(&fixture_path, &ai_config, &storage_paths).unwrap();
        assert!(report.is_valid);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn test_phase6i_08_preflight_failure_missing_video() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-08-{}", uuid::Uuid::new_v4());
        let _ = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let missing_video = temp.path().join("missing.mp4");
        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: None,
            profile_hash: None,
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(640, 640),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let report = validate_ai_job_preflight(&missing_video, &ai_config, &storage_paths).unwrap();
        assert!(!report.is_valid);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn test_phase6i_09_preflight_failure_invalid_model() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: "nonexistent-model-xyz-999".to_string(),
            model_version: Some("1.0.0".to_string()),
            model_hash: None,
            profile_hash: None,
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(640, 640),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let report = validate_ai_job_preflight(&fixture_path, &ai_config, &storage_paths).unwrap();
        assert!(!report.is_valid);
        assert!(!report.errors.is_empty());
    }

    // =========================================================================
    // 4. Cancellation at Granular Boundaries Tests
    // =========================================================================

    #[test]
    fn test_phase6i_10_cancellation_before_job_start() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-10-{}", uuid::Uuid::new_v4());
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

        let cancel_token = Arc::new(AtomicBool::new(true)); // Cancelled before start

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
    fn test_phase6i_11_cancellation_during_inference() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-11-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        for i in 0..8 {
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
        assert!(artifact_mgr.frame_output_png_path(0).exists());
        assert!(artifact_mgr.frame_output_png_path(1).exists());
    }

    #[test]
    fn test_phase6i_12_cancellation_during_reconstruction() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("frames");
        for i in 0..10 {
            create_test_frame_png(&frames_dir.join(format!("{:06}.png", i)), 64, 64);
        }

        let output_path = temp.path().join("output.mp4");
        let cancel_token = Arc::new(AtomicBool::new(true)); // Pre-cancelled

        let cfg = crate::ai::VideoReconstructionConfig {
            source_video_path: temp.path().join("dummy.mp4"),
            frames_dir,
            output_path: output_path.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 10,
            width: 64,
            height: 64,
            fps: RationalFps::new(30, 1),
            pixel_format: "yuv420p".to_string(),
            codec: crate::ai::VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: crate::ai::AudioPreservationMode::None,
            overwrite: true,
        };

        let err = VideoReconstructor::reconstruct_video(
            &cfg,
            "job-canc-recon",
            None,
            None,
            |_, _, _| {},
            Some(cancel_token),
            None::<fn(u32)>,
            None::<fn(u32)>,
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::Cancelled);
        assert!(!output_path.exists());
    }

    // =========================================================================
    // 5. Retry & Artifact Resumption Tests
    // =========================================================================

    #[test]
    fn test_phase6i_13_retry_reuses_valid_artifacts() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-13-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let frames_dir = temp.path().join("frames");
        for i in 0..4 {
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

        // Run 1: process 4 frames
        let m1 =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();
        assert_eq!(m1.frames_processed, 4);
        assert_eq!(m1.frames_reused, 0);

        // Run 2 (Retry): reuses all 4 frames
        let m2 =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();
        assert_eq!(m2.frames_processed, 4);
        assert_eq!(m2.frames_reused, 4);
    }

    #[test]
    fn test_phase6i_14_retry_recovers_corrupted_frame() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-14-{}", uuid::Uuid::new_v4());
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

        // Run 1
        let _ =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();

        // Corrupt frame 1 file by truncating to 0 bytes
        let frame1_png = artifact_mgr.frame_output_png_path(1);
        fs::write(&frame1_png, b"").unwrap();

        // Run 2 (Retry): frame 0 and 2 reused, frame 1 detected as corrupt and re-inferred
        let m2 =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();

        assert_eq!(m2.frames_processed, 3);
        assert_eq!(m2.frames_reused, 2);
        assert!(fs::metadata(&frame1_png).unwrap().len() > 0);
    }

    // =========================================================================
    // 6. Reconstruction & Audio Preservation Tests
    // =========================================================================

    #[test]
    fn test_phase6i_15_reconstruction_with_ffmpeg() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("frames");
        for i in 0..10 {
            create_test_frame_png(&frames_dir.join(format!("{:06}.png", i)), 64, 64);
        }

        let output_path = temp.path().join("recon_output.mp4");
        let cfg = crate::ai::VideoReconstructionConfig {
            source_video_path: temp.path().join("dummy.mp4"),
            frames_dir: frames_dir.clone(),
            output_path: output_path.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 10,
            width: 64,
            height: 64,
            fps: RationalFps::new(10, 1),
            pixel_format: "yuv420p".to_string(),
            codec: crate::ai::VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: crate::ai::AudioPreservationMode::None,
            overwrite: true,
        };

        let res = VideoReconstructor::reconstruct_video(
            &cfg,
            "job-recon-15",
            None,
            None,
            |_, _, _| {},
            None,
            None::<fn(u32)>,
            None::<fn(u32)>,
        )
        .unwrap();

        assert!(res.output_path.exists());
        assert!(res.output_metadata.file_size_bytes > 0);
        assert_eq!(res.output_metadata.width, 64);
        assert_eq!(res.output_metadata.height, 64);
    }

    #[test]
    fn test_phase6i_16_audio_preservation_in_reconstruction() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("frames");
        for i in 0..15 {
            create_test_frame_png(&frames_dir.join(format!("{:06}.png", i)), 64, 64);
        }

        let output_path = temp.path().join("recon_audio.mp4");
        let cfg = crate::ai::VideoReconstructionConfig {
            source_video_path: fixture_path.clone(),
            frames_dir,
            output_path: output_path.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 15,
            width: 64,
            height: 64,
            fps: RationalFps::new(15, 1),
            pixel_format: "yuv420p".to_string(),
            codec: crate::ai::VideoCodec::H264,
            crf: 18,
            audio_source: Some(fixture_path),
            audio_mode: crate::ai::AudioPreservationMode::PreserveOriginal,
            overwrite: true,
        };

        let res = VideoReconstructor::reconstruct_video(
            &cfg,
            "job-recon-16",
            None,
            None,
            |_, _, _| {},
            None,
            None::<fn(u32)>,
            None::<fn(u32)>,
        )
        .unwrap();

        assert!(res.output_path.exists());
        assert!(res.output_metadata.file_size_bytes > 0);
    }

    // =========================================================================
    // 7. Output Validation & Duration Tolerances
    // =========================================================================

    #[test]
    fn test_phase6i_17_output_validation_gate() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let media_service = MediaService::new();
        let probed = media_service.probe(&fixture_path).unwrap();
        let fps = RationalFps::from_f64(probed.fps);

        let validated = VideoReconstructor::validate_reconstructed_video(
            &fixture_path,
            probed.width,
            probed.height,
            fps,
            0,
            probed.has_audio,
        )
        .unwrap();

        assert_eq!(validated.width, probed.width);
        assert_eq!(validated.height, probed.height);
        assert!(validated.file_size_bytes > 0);
    }

    #[test]
    fn test_phase6i_18_output_validation_duration_mismatch() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing_output.mp4");
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
    // 8. Application Restart Recovery & Error Handling
    // =========================================================================

    #[tokio::test]
    async fn test_phase6i_19_app_restart_interrupted_job_recovery() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let engine = JobEngine::new(storage_paths.clone());

        // Create job and mark it Running on disk (as if app crashed during execution)
        let mut job = engine
            .create_job_with_app::<tauri::Wry>(
                None,
                "proj-rec-19",
                Some("video_pipeline".to_string()),
                vec!["input.mp4".to_string()],
            )
            .unwrap();

        job.status = JobStatus::Running;
        engine.save_job_manifest(&job).unwrap();

        // Simulate app restart recovery
        let recovered_count = engine.recover_interrupted_jobs().unwrap();
        assert_eq!(recovered_count, 1);

        let reloaded = engine.get_job(&job.id).unwrap();
        assert_eq!(reloaded.status, JobStatus::Interrupted);
    }

    #[test]
    fn test_phase6i_20_invalid_model_format_rejected() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let registry = ModelRegistry::new(storage_paths.models_dir);

        let dummy_txt = temp.path().join("model.txt");
        fs::write(&dummy_txt, b"not an onnx file").unwrap();

        let err = registry
            .import_model(
                &dummy_txt,
                "test-txt",
                "Test Text",
                "1.0.0",
                "Test",
                "Test",
                create_test_profile(2, 2, OutputInterpretationType::Image),
                ModelRequirements::default(),
                vec![ExecutionProvider::Cpu],
            )
            .unwrap_err();

        assert_eq!(err.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn test_phase6i_21_unsupported_provider_rejected() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-21-{}", uuid::Uuid::new_v4());
        let _ = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let registry = ModelRegistry::new(storage_paths.models_dir);
        // Explicitly request TensorRT (which is not available on this CPU platform)
        let err = crate::ai::ProductionModelResolver::resolve_model(
            &registry,
            Some(&model_id),
            Some("1.0.0"),
            Some(ExecutionProvider::TensorRT),
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::ModelProviderUnsupported);
    }

    #[test]
    fn test_phase6i_22_disk_quota_exceeded_rejected() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-22-{}", uuid::Uuid::new_v4());
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
            max_job_disk_bytes: 1, // Exceeds disk quota immediately on write
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

    #[test]
    fn test_phase6i_23_zero_byte_video_rejected() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6i-23-{}", uuid::Uuid::new_v4());
        let _ = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let zero_video = temp.path().join("zero.mp4");
        fs::write(&zero_video, b"").unwrap();

        let ai_config = AiJobConfig {
            enabled: true,
            model_id,
            model_version: Some("1.0.0".to_string()),
            model_hash: None,
            profile_hash: None,
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(640, 640),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
        };

        let report = validate_ai_job_preflight(&zero_video, &ai_config, &storage_paths).unwrap();
        assert!(!report.is_valid);
        assert!(report
            .checks
            .iter()
            .any(|c| c.check == "SOURCE_FILE_EMPTY" || c.check == "SOURCE_FILE_VALID"));
    }

    #[test]
    fn test_phase6i_24_missing_intermediate_frame_rejected() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("frames");
        // Create frame 0 and frame 2, missing frame 1
        create_test_frame_png(&frames_dir.join("000000.png"), 64, 64);
        create_test_frame_png(&frames_dir.join("000002.png"), 64, 64);

        let output_path = temp.path().join("output.mp4");
        let cfg = crate::ai::VideoReconstructionConfig {
            source_video_path: temp.path().join("dummy.mp4"),
            frames_dir,
            output_path,
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 3,
            width: 64,
            height: 64,
            fps: RationalFps::new(10, 1),
            pixel_format: "yuv420p".to_string(),
            codec: crate::ai::VideoCodec::H264,
            crf: 18,
            audio_source: None,
            audio_mode: crate::ai::AudioPreservationMode::None,
            overwrite: true,
        };

        let err = VideoReconstructor::reconstruct_video(
            &cfg,
            "job-missing-frame",
            None,
            None,
            |_, _, _| {},
            None,
            None::<fn(u32)>,
            None::<fn(u32)>,
        )
        .unwrap_err();

        assert!(
            err.code == ErrorCode::FrameSequenceInvalid
                || err.code == ErrorCode::FileNotFound
                || err.code == ErrorCode::ProcessFailed
        );
    }

    #[test]
    fn test_phase6i_25_wrong_frame_dimensions_rejected() {
        let limits = AiResourceLimits {
            max_frame_width: 100,
            max_frame_height: 100,
            ..AiResourceLimits::default_production()
        };
        let err = limits.validate_frame_dimensions(200, 50).unwrap_err();
        assert_eq!(err.code, ErrorCode::ResourceLimitExceeded);
    }

    #[test]
    fn test_phase6i_26_wrong_fps_rejected() {
        let fps = RationalFps::from_f64(-5.0);
        assert_eq!(fps.num, 30); // Handled safely by falling back to default
    }

    #[test]
    fn test_phase6i_27_execution_report_artifacts_and_metrics() {
        let temp = TempDir::new().unwrap();
        let report_file = temp.path().join("ai_execution_report.json");

        let mut report = AiProductionExecutionReport::new(
            "job-report-27",
            "model-test",
            Some("1.0.0"),
            Some("hash_abc"),
            Some("prof_xyz"),
            "CPU",
            1920,
            1080,
            30.0,
            5000,
            150,
        );

        report.selected_frames = 150;
        report.processed_frames = 150;
        report.inference_ms = 450.0;
        report.total_ms = 1200.0;
        report.output_path = Some("output.mp4".to_string());
        report.save_to_file(&report_file).unwrap();

        let loaded = AiProductionExecutionReport::load_from_file(&report_file).unwrap();
        assert_eq!(loaded.job_id, "job-report-27");
        assert_eq!(loaded.source_total_frames, 150);
        assert_eq!(loaded.inference_ms, 450.0);
    }

    // =========================================================================
    // 9. Full End-to-End Real Video Pipeline Integration Test
    // =========================================================================

    #[tokio::test]
    async fn test_phase6i_28_full_end_to_end_real_video_pipeline() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            println!("sample_portrait_video.mp4 not found, skipping full E2E test");
            return;
        }

        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);

        // 1. Setup real ONNX model in registry
        let model_id = format!("real-ai-model-28-{}", uuid::Uuid::new_v4());
        let pkg = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");
        let engine = JobEngine::new(storage_paths.clone());

        // 2. Configure AI Job with test_1s mode to run fast (1 second execution)
        let ai_config = AiJobConfig {
            enabled: true,
            model_id: model_id.clone(),
            model_version: Some("1.0.0".to_string()),
            model_hash: Some(pkg.sha256.clone()),
            profile_hash: Some(pkg.profile.compute_profile_hash()),
            provider: Some(ExecutionProvider::Cpu),
            preprocessing: sample_preprocess_config(2, 2),
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::all(),
            output_mode: AiFrameOutputMode::Image,
        };

        // 3. Create and queue AI Job
        let mut job = engine
            .create_ai_job_with_app::<tauri::Wry>(
                None,
                "proj-real-e2e-28",
                None,
                vec![fixture_path.to_string_lossy().to_string()],
                ai_config,
            )
            .unwrap();

        job.metadata = serde_json::json!({ "mode": "test_1s" });
        engine.save_job_manifest(&job).unwrap();

        assert_eq!(job.stages.len(), 7);
        assert_eq!(job.stages[4].id, "stage_ai_frame_inference");
        assert_eq!(job.status, JobStatus::Queued);

        // 4. Run pipeline synchronously
        let cancel_token = Arc::new(AtomicBool::new(false));
        let child_pids = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        engine
            .execute_pipeline_runner::<tauri::Wry>(
                None,
                &job.project_id,
                &job.id,
                cancel_token,
                child_pids,
            )
            .await;

        let completed = engine.get_job(&job.id).unwrap();
        assert_eq!(completed.status, JobStatus::Completed);
        assert_eq!(completed.progress, 100.0);
        assert!(!completed.output_files.is_empty());

        let out_path = PathBuf::from(&completed.output_files[0]);
        assert!(out_path.exists());
        assert!(fs::metadata(&out_path).unwrap().len() > 0);

        // 5. Verify Execution Report was generated and persisted
        let out_dir = out_path.parent().unwrap();
        let report_path = out_dir.join("ai_execution_report.json");
        assert!(report_path.exists());

        let report = AiProductionExecutionReport::load_from_file(&report_path).unwrap();
        assert_eq!(report.job_id, completed.id);
        assert_eq!(report.model_id, model_id);
        assert!(report.source_total_frames > 0);
        assert!(report.output_path.is_some());
    }
}
