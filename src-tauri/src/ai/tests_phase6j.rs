#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    use crate::ai::frame_pipeline::artifact::AiArtifactManager;
    use crate::ai::frame_pipeline::config::{
        select_frames, AiFrameOutputMode, AiJobConfig, FrameSamplingConfig,
    };
    use crate::ai::frame_pipeline::executor::AiFrameExecutor;
    use crate::ai::frame_pipeline::reconstruct::{RationalFps, VideoReconstructor};
    use crate::ai::manifest::{AiModelManifest, ModelFormat, ModelRequirements};
    use crate::ai::package::{calculate_file_sha256, AiModelPackage};
    use crate::ai::pipeline::{
        generate_image_onnx_model, ChannelOrder, NormalizationConfig, PreprocessConfig,
        ResizeFilter, TensorLayout,
    };
    use crate::ai::preflight::{validate_ai_job_preflight, PreflightCheckStatus};
    use crate::ai::profile::{
        AiModelProfile, AspectHandling, InputProfile, OutputInterpretationType, OutputProfile,
    };
    use crate::ai::provider::ExecutionProvider;
    use crate::ai::registry::ModelRegistry;
    use crate::ai::tensor::TensorDataType;
    use crate::commands::{cleanup_temp_storage, clear_storage_cache, get_storage_usage};
    use crate::error::ErrorCode;
    use crate::jobs::{JobEngine, JobStatus};
    use crate::media::MediaService;
    use crate::projects::ProjectManager;
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

    fn setup_test_package(models_dir: &Path, model_id: &str, version: &str) -> AiModelPackage {
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
    // 1. Project Creation, Media Import & Probe
    // =========================================================================

    #[test]
    fn test_phase6j_01_project_creation_and_metadata() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let manager = ProjectManager::new(storage_paths.clone());

        let project = manager.create_project("My Production Project").unwrap();
        assert_eq!(project.name, "My Production Project");
        assert!(storage_paths.projects_dir.join(&project.id).exists());

        let loaded = manager.get_project(&project.id).unwrap();
        assert_eq!(loaded.id, project.id);
        assert_eq!(loaded.name, "My Production Project");
    }

    #[test]
    fn test_phase6j_02_media_import_and_probe() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let manager = ProjectManager::new(storage_paths.clone());
        let project = manager.create_project("Media Import Project").unwrap();
        let proj_dir = manager.project_dir(&project.id);

        let media_service = MediaService::new();
        let imported = media_service
            .import_to_project(&proj_dir, &fixture_path)
            .unwrap();

        assert!(imported.width > 0);
        assert!(imported.height > 0);
        assert!(imported.duration_ms > 0);
        assert!(imported.fps > 0.0);
        assert_eq!(imported.container, "mp4");
    }

    // =========================================================================
    // 2. Model Selection, Versioning & Presets
    // =========================================================================

    #[test]
    fn test_phase6j_03_model_selection_and_versioning() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let registry = ModelRegistry::new(storage_paths.models_dir.clone());

        let model_id = format!("prod-model-{}", uuid::Uuid::new_v4());
        let pkg1 = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");
        let pkg2 = setup_test_package(&storage_paths.models_dir, &model_id, "1.1.0");

        assert_eq!(pkg1.version, "1.0.0");
        assert_eq!(pkg2.version, "1.1.0");

        let active = registry.get_active_package(&model_id).unwrap();
        assert_eq!(active.version, "1.1.0");

        // Rollback / switch version
        registry.activate_version(&model_id, "1.0.0").unwrap();
        let active_switched = registry.get_active_package(&model_id).unwrap();
        assert_eq!(active_switched.version, "1.0.0");
    }

    #[test]
    fn test_phase6j_04_preset_mapping_fast_balanced_quality() {
        // Fast Preset: every 3rd frame
        let fast_sampling = FrameSamplingConfig::every_nth(3);
        let fast_sel = select_frames(30, &fast_sampling).unwrap();
        assert_eq!(fast_sel.len(), 10);

        // Balanced Preset: every 2nd frame
        let balanced_sampling = FrameSamplingConfig::every_nth(2);
        let bal_sel = select_frames(30, &balanced_sampling).unwrap();
        assert_eq!(bal_sel.len(), 15);

        // Quality Preset: 100% all frames
        let quality_sampling = FrameSamplingConfig::all();
        let qual_sel = select_frames(30, &quality_sampling).unwrap();
        assert_eq!(qual_sel.len(), 30);
    }

    // =========================================================================
    // 3. Preflight Validation Gates
    // =========================================================================

    #[test]
    fn test_phase6j_05_preflight_gate_blocking_invalid_source() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6j-05-{}", uuid::Uuid::new_v4());
        let _ = setup_test_package(&storage_paths.models_dir, &model_id, "1.0.0");

        let missing_video = temp.path().join("non_existent_video.mp4");
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
        assert!(report
            .checks
            .iter()
            .any(|c| c.status == PreflightCheckStatus::Fail));
    }

    #[test]
    fn test_phase6j_06_preflight_gate_success_valid_model_and_source() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6j-06-{}", uuid::Uuid::new_v4());
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

    // =========================================================================
    // 4. AI Job Creation & Immutable Model Pinning
    // =========================================================================

    #[tokio::test]
    async fn test_phase6j_07_job_creation_with_immutable_pinning() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6j-07-{}", uuid::Uuid::new_v4());
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

        let job = engine
            .create_ai_job_with_app::<tauri::Wry>(
                None,
                "proj-6j-07",
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

    // =========================================================================
    // 5. Cancellation, Retry & Resumption
    // =========================================================================

    #[tokio::test]
    async fn test_phase6j_08_cancellation_during_execution() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let engine = JobEngine::new(storage_paths.clone());

        let job = engine
            .create_job_with_app::<tauri::Wry>(
                None,
                "proj-6j-08",
                Some("video_pipeline".to_string()),
                vec!["input.mp4".to_string()],
            )
            .unwrap();

        let cancelled = engine
            .cancel_job::<tauri::Wry>(None, &job.id)
            .await
            .unwrap();
        assert_eq!(cancelled.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_phase6j_09_retry_of_failed_or_cancelled_job() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let engine = JobEngine::new(storage_paths.clone());

        let mut job = engine
            .create_job_with_app::<tauri::Wry>(
                None,
                "proj-6j-09",
                Some("video_pipeline".to_string()),
                vec!["input.mp4".to_string()],
            )
            .unwrap();

        job.status = JobStatus::Failed;
        job.error = Some(crate::jobs::JobError {
            code: "ProcessFailed".to_string(),
            message: "Temporary error".to_string(),
            details: None,
        });
        engine.save_job_manifest(&job).unwrap();

        let retried = engine.retry_job::<tauri::Wry>(None, &job.id).await.unwrap();
        assert!(retried.status == JobStatus::Queued || retried.status == JobStatus::Running);
        assert!(retried.error.is_none());
        assert_eq!(retried.retry_count, 1);
    }

    #[test]
    fn test_phase6j_10_resumption_with_valid_artifact_reuse() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let model_id = format!("model-6j-10-{}", uuid::Uuid::new_v4());
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

        // Run 1: process 5 frames
        let m1 =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();
        assert_eq!(m1.frames_processed, 5);
        assert_eq!(m1.frames_reused, 0);

        // Run 2 (Resumption): reuses all 5 cached frames
        let m2 =
            AiFrameExecutor::execute(&frames_dir, &ai_config, &artifact_mgr, None, |_, _, _| {})
                .unwrap();
        assert_eq!(m2.frames_processed, 5);
        assert_eq!(m2.frames_reused, 5);
    }

    // =========================================================================
    // 6. Video Reconstruction & Output Validation
    // =========================================================================

    #[test]
    fn test_phase6j_11_completed_video_reconstruction_and_output_validation() {
        let temp = TempDir::new().unwrap();
        let frames_dir = temp.path().join("frames");
        for i in 0..8 {
            create_test_frame_png(&frames_dir.join(format!("{:06}.png", i)), 64, 64);
        }

        let output_path = temp.path().join("reconstructed_output.mp4");
        let cfg = crate::ai::VideoReconstructionConfig {
            source_video_path: temp.path().join("dummy.mp4"),
            frames_dir: frames_dir.clone(),
            output_path: output_path.clone(),
            frame_pattern: "%06d.png".to_string(),
            expected_frame_count: 8,
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
            "job-recon-6j",
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

        let validated = VideoReconstructor::validate_reconstructed_video(
            &output_path,
            64,
            64,
            RationalFps::new(10, 1),
            0,
            false,
        )
        .unwrap();

        assert_eq!(validated.width, 64);
        assert_eq!(validated.height, 64);
    }

    // =========================================================================
    // 7. Error Handling & Edge Cases
    // =========================================================================

    #[test]
    fn test_phase6j_12_invalid_source_video_error_handling() {
        let temp = TempDir::new().unwrap();
        let zero_video = temp.path().join("zero_length.mp4");
        fs::write(&zero_video, b"").unwrap();

        let media_service = MediaService::new();
        let err = media_service.probe(&zero_video).unwrap_err();
        assert_eq!(err.code, ErrorCode::MediaInvalid);
    }

    #[test]
    fn test_phase6j_13_unavailable_model_error_handling() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let registry = ModelRegistry::new(storage_paths.models_dir);

        let err = registry
            .get_model("nonexistent_model_id_12345")
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ModelNotAvailable);
    }

    // =========================================================================
    // 8. Storage Management Commands
    // =========================================================================

    #[test]
    fn test_phase6j_14_storage_usage_calculation() {
        let usage = get_storage_usage().unwrap();
        assert_eq!(
            usage.total_bytes,
            usage.projects_bytes
                + usage.cache_bytes
                + usage.ai_cache_bytes
                + usage.models_bytes
                + usage.temp_bytes
                + usage.logs_bytes
        );
    }

    #[test]
    fn test_phase6j_15_storage_cache_clearing() {
        let res = clear_storage_cache();
        assert!(res.is_ok());
    }

    #[test]
    fn test_phase6j_16_temporary_file_cleanup() {
        let res = cleanup_temp_storage();
        assert!(res.is_ok());
    }

    // =========================================================================
    // 9. Complete Job History & Restart Recovery
    // =========================================================================

    #[tokio::test]
    async fn test_phase6j_17_job_history_persistence_and_query() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let engine = JobEngine::new(storage_paths.clone());

        // Create 2 jobs in different projects
        let _ = engine
            .create_job_with_app::<tauri::Wry>(
                None,
                "proj-history-1",
                Some("video_pipeline".to_string()),
                vec!["v1.mp4".to_string()],
            )
            .unwrap();

        let _ = engine
            .create_job_with_app::<tauri::Wry>(
                None,
                "proj-history-2",
                Some("video_pipeline".to_string()),
                vec!["v2.mp4".to_string()],
            )
            .unwrap();

        let all_jobs = engine.list_jobs(None).unwrap();
        assert!(all_jobs.len() >= 2);
        assert!(all_jobs.iter().any(|j| j.project_id == "proj-history-1"));
        assert!(all_jobs.iter().any(|j| j.project_id == "proj-history-2"));
    }

    #[tokio::test]
    async fn test_phase6j_18_app_restart_recovery_of_interrupted_jobs() {
        let temp = TempDir::new().unwrap();
        let storage_paths = make_storage_paths(&temp);
        let engine = JobEngine::new(storage_paths.clone());

        let mut job = engine
            .create_job_with_app::<tauri::Wry>(
                None,
                "proj-restart-18",
                Some("video_pipeline".to_string()),
                vec!["input.mp4".to_string()],
            )
            .unwrap();

        job.status = JobStatus::Running;
        engine.save_job_manifest(&job).unwrap();

        // Perform recovery
        let recovered = engine.recover_interrupted_jobs().unwrap();
        assert_eq!(recovered, 1);

        let reloaded = engine.get_job(&job.id).unwrap();
        assert_eq!(reloaded.status, JobStatus::Interrupted);
    }
}
