pub mod artifact;
pub mod benchmark;
pub mod config;
pub mod executor;
pub mod quality;
pub mod reconstruct;

pub use artifact::{
    calculate_sha256, compute_ai_config_hash, compute_ai_job_config_hash, AiArtifactManager,
    AiFrameMetadata, AiFrameStatus, AiJobMetrics,
};
pub use benchmark::AiJobBenchmarkReport;
pub use config::{
    select_frames, AiFrameOutputMode, AiJobConfig, FrameSamplingConfig, FrameSamplingMode,
};
pub use executor::AiFrameExecutor;
pub use quality::{
    FrameQualityReport, FrameQualityStatus, FrameQualityValidator, FrameSequenceValidationReport,
    TechnicalQualityMetrics,
};
pub use reconstruct::{
    AudioPreservationMode, FrameManifestEntry, RationalFps, ReconstructionManifest,
    ReconstructionResult, ReconstructionTelemetry, VideoCodec, VideoReconstructionConfig,
    VideoReconstructor,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    use crate::ai::manifest::AiModelManifest;
    use crate::ai::onnx::get_global_ai_runtime;
    use crate::ai::pipeline::generate_image_onnx_model;
    use crate::ai::pipeline::image::{ImageFrame, PixelFormat};
    use crate::ai::pipeline::preprocess::PreprocessConfig;
    use crate::ai::registry::ModelRegistry;
    use crate::ai::runtime::AiRuntime;

    fn setup_test_image_model() -> (tempfile::TempDir, AiModelManifest) {
        let tmp = tempdir().unwrap();
        let model_path = tmp.path().join("image_multiplier.onnx");
        generate_image_onnx_model(&model_path).unwrap();

        let manifest = AiModelManifest::new(
            format!("test-img-mult-{}", uuid::Uuid::new_v4()),
            "Test 4D Image Multiplier",
            "1.0.0",
            crate::ai::manifest::ModelFormat::Onnx,
            model_path,
            "Test 4D graph multiplying float tensor by 2",
            vec![],
            vec![],
            crate::ai::manifest::ModelRequirements::default(),
        );

        let storage_paths = crate::StoragePaths::default_paths();
        let reg = ModelRegistry::new(storage_paths.models_dir);
        let _ = reg.register_model(manifest.clone());

        (tmp, manifest)
    }

    fn create_test_frame_pngs(dir: &std::path::Path, count: usize) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        for i in 0..count {
            let p = dir.join(format!("{:06}.png", i));
            let img = ImageFrame::new(
                2,
                2,
                PixelFormat::Rgb8,
                vec![
                    10 + i as u8,
                    20 + i as u8,
                    30 + i as u8,
                    40 + i as u8,
                    50 + i as u8,
                    60 + i as u8,
                    70 + i as u8,
                    80 + i as u8,
                    90 + i as u8,
                    100 + i as u8,
                    110 + i as u8,
                    120 + i as u8,
                ],
            )
            .unwrap();
            img.encode_to_png(&p).unwrap();
            paths.push(p);
        }
        paths
    }

    #[test]
    fn test_phase6d_01_frame_sampling_all() {
        let cfg = FrameSamplingConfig {
            mode: FrameSamplingMode::All,
            nth: None,
            start: None,
            end: None,
        };
        let res = select_frames(10, &cfg).unwrap();
        assert_eq!(res, (0..10).collect::<Vec<usize>>());
    }

    #[test]
    fn test_phase6d_02_frame_sampling_every_nth() {
        let cfg = FrameSamplingConfig {
            mode: FrameSamplingMode::EveryNth,
            nth: Some(3),
            start: None,
            end: None,
        };
        let res = select_frames(10, &cfg).unwrap();
        assert_eq!(res, vec![0, 3, 6, 9]);
    }

    #[test]
    fn test_phase6d_03_frame_sampling_range() {
        let cfg = FrameSamplingConfig {
            mode: FrameSamplingMode::Range,
            nth: None,
            start: Some(3),
            end: Some(7),
        };
        let res = select_frames(10, &cfg).unwrap();
        assert_eq!(res, vec![3, 4, 5, 6, 7]);
    }

    #[test]
    fn test_phase6d_04_frame_sampling_invalid_bounds() {
        let cfg_inv_nth = FrameSamplingConfig {
            mode: FrameSamplingMode::EveryNth,
            nth: Some(0),
            start: None,
            end: None,
        };
        assert!(select_frames(10, &cfg_inv_nth).is_err());

        let cfg_inv_range = FrameSamplingConfig {
            mode: FrameSamplingMode::Range,
            nth: None,
            start: Some(8),
            end: Some(2),
        };
        assert!(select_frames(10, &cfg_inv_range).is_err());
    }

    #[test]
    fn test_phase6d_05_config_hash_consistency() {
        let prep = PreprocessConfig {
            target_width: 2,
            target_height: 2,
            ..Default::default()
        };
        let h1 = compute_ai_config_hash("model-a", &prep, None);
        let h2 = compute_ai_config_hash("model-a", &prep, None);
        let h3 = compute_ai_config_hash("model-b", &prep, None);
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_phase6d_06_real_frame_inference_executor_all_frames() {
        let (_model_dir, manifest) = setup_test_image_model();
        let frames_tmp = tempdir().unwrap();
        let ai_tmp = tempdir().unwrap();

        create_test_frame_pngs(frames_tmp.path(), 4);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: manifest.id.clone(),
            provider: None,
            preprocessing: PreprocessConfig {
                target_width: 2,
                target_height: 2,
                ..Default::default()
            },
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
            ..Default::default()
        };

        let artifact_mgr = AiArtifactManager::new(ai_tmp.path());

        {
            let runtime = get_global_ai_runtime();
            let mut r = runtime.lock().unwrap();
            let _ = r.unload_model();
        }

        let mut progress_count = 0;
        let metrics = AiFrameExecutor::execute(
            frames_tmp.path(),
            &ai_config,
            &artifact_mgr,
            None,
            |_prog, meta, _m| {
                progress_count += 1;
                if let Some(m) = meta {
                    assert_eq!(m.status, AiFrameStatus::Completed);
                }
            },
        )
        .unwrap();

        assert_eq!(metrics.frames_total, 4);
        assert_eq!(metrics.frames_selected, 4);
        assert_eq!(metrics.frames_processed, 4);
        assert_eq!(metrics.frames_reused, 0);
        assert_eq!(metrics.frames_passthrough, 0);
        assert!(metrics.total_inference_duration_ms > 0.0);
        assert_eq!(progress_count, 4);

        // Verify output PNGs in reconstruction directory
        let recon_dir = artifact_mgr.reconstruction_frames_dir();
        assert!(recon_dir.join("000000.png").exists());
        assert!(recon_dir.join("000001.png").exists());
        assert!(recon_dir.join("000002.png").exists());
        assert!(recon_dir.join("000003.png").exists());
    }

    #[test]
    fn test_phase6d_07_real_frame_inference_executor_sampling_and_passthrough() {
        let (_model_dir, manifest) = setup_test_image_model();
        let frames_tmp = tempdir().unwrap();
        let ai_tmp = tempdir().unwrap();

        create_test_frame_pngs(frames_tmp.path(), 5);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: manifest.id.clone(),
            provider: None,
            preprocessing: PreprocessConfig {
                target_width: 2,
                target_height: 2,
                ..Default::default()
            },
            postprocessing: None,
            frame_sampling: FrameSamplingConfig {
                mode: FrameSamplingMode::EveryNth,
                nth: Some(2),
                start: None,
                end: None,
            },
            output_mode: AiFrameOutputMode::Image,
            ..Default::default()
        };

        let artifact_mgr = AiArtifactManager::new(ai_tmp.path());

        {
            let runtime = get_global_ai_runtime();
            let mut r = runtime.lock().unwrap();
            let _ = r.unload_model();
        }

        let metrics = AiFrameExecutor::execute(
            frames_tmp.path(),
            &ai_config,
            &artifact_mgr,
            None,
            |_prog, _meta, _m| {},
        )
        .unwrap();

        assert_eq!(metrics.frames_total, 5);
        assert_eq!(metrics.frames_selected, 3); // 0, 2, 4
        assert_eq!(metrics.frames_processed, 3);
        assert_eq!(metrics.frames_passthrough, 2); // 1, 3

        // All 5 frames exist in reconstruction directory
        let recon_dir = artifact_mgr.reconstruction_frames_dir();
        for i in 0..5 {
            assert!(recon_dir.join(format!("{:06}.png", i)).exists());
        }
    }

    #[test]
    fn test_phase6d_08_retry_reuses_valid_cached_frame_artifacts() {
        let (_model_dir, manifest) = setup_test_image_model();
        let frames_tmp = tempdir().unwrap();
        let ai_tmp = tempdir().unwrap();

        create_test_frame_pngs(frames_tmp.path(), 3);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: manifest.id.clone(),
            provider: None,
            preprocessing: PreprocessConfig {
                target_width: 2,
                target_height: 2,
                ..Default::default()
            },
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
            ..Default::default()
        };

        let artifact_mgr = AiArtifactManager::new(ai_tmp.path());

        // Run 1: Initial run
        {
            let runtime = get_global_ai_runtime();
            let mut r = runtime.lock().unwrap();
            let _ = r.unload_model();
        }
        let metrics1 = AiFrameExecutor::execute(
            frames_tmp.path(),
            &ai_config,
            &artifact_mgr,
            None,
            |_p, _m, _x| {},
        )
        .unwrap();
        assert_eq!(metrics1.frames_processed, 3);
        assert_eq!(metrics1.frames_reused, 0);

        // Run 2: Re-run with same configuration -> all 3 frames reused!
        let metrics2 = AiFrameExecutor::execute(
            frames_tmp.path(),
            &ai_config,
            &artifact_mgr,
            None,
            |_p, _m, _x| {},
        )
        .unwrap();
        assert_eq!(metrics2.frames_processed, 3);
        assert_eq!(metrics2.frames_reused, 3);
    }

    #[test]
    fn test_phase6d_09_partial_retry_corrupted_frame() {
        let (_model_dir, manifest) = setup_test_image_model();
        let frames_tmp = tempdir().unwrap();
        let ai_tmp = tempdir().unwrap();

        create_test_frame_pngs(frames_tmp.path(), 3);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: manifest.id.clone(),
            provider: None,
            preprocessing: PreprocessConfig {
                target_width: 2,
                target_height: 2,
                ..Default::default()
            },
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
            ..Default::default()
        };

        let artifact_mgr = AiArtifactManager::new(ai_tmp.path());

        // Run 1: Initial run
        {
            let runtime = get_global_ai_runtime();
            let mut r = runtime.lock().unwrap();
            let _ = r.unload_model();
        }
        let _ = AiFrameExecutor::execute(
            frames_tmp.path(),
            &ai_config,
            &artifact_mgr,
            None,
            |_p, _m, _x| {},
        )
        .unwrap();

        // Corrupt frame 1's artifact by deleting its JSON metadata
        let f1_json = artifact_mgr.frame_result_json_path(1);
        let _ = fs::remove_file(&f1_json);

        // Run 2: Re-run -> frames 0 and 2 reused, frame 1 re-processed!
        let metrics2 = AiFrameExecutor::execute(
            frames_tmp.path(),
            &ai_config,
            &artifact_mgr,
            None,
            |_p, _m, _x| {},
        )
        .unwrap();
        assert_eq!(metrics2.frames_processed, 3);
        assert_eq!(metrics2.frames_reused, 2); // 0 and 2 reused
    }

    #[test]
    fn test_phase6d_10_cancellation_during_frame_inference() {
        let (_model_dir, manifest) = setup_test_image_model();
        let frames_tmp = tempdir().unwrap();
        let ai_tmp = tempdir().unwrap();

        create_test_frame_pngs(frames_tmp.path(), 4);

        let ai_config = AiJobConfig {
            enabled: true,
            model_id: manifest.id.clone(),
            provider: None,
            preprocessing: PreprocessConfig {
                target_width: 2,
                target_height: 2,
                ..Default::default()
            },
            postprocessing: None,
            frame_sampling: FrameSamplingConfig::default(),
            output_mode: AiFrameOutputMode::Image,
            ..Default::default()
        };

        let artifact_mgr = AiArtifactManager::new(ai_tmp.path());
        let cancel_token = Arc::new(AtomicBool::new(false));

        let token_cloned = cancel_token.clone();
        let res = AiFrameExecutor::execute(
            frames_tmp.path(),
            &ai_config,
            &artifact_mgr,
            Some(cancel_token),
            |prog, _m, _x| {
                if prog >= 25.0 {
                    token_cloned.store(true, Ordering::SeqCst);
                }
            },
        );

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.code, crate::error::ErrorCode::Cancelled);
        assert!(err.message.to_lowercase().contains("cancel"));

        // Frame 0 completed artifact exists
        assert!(artifact_mgr.frame_output_png_path(0).exists());
    }
}
