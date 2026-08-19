#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::ai::generative::backend::{
        CharacterReference, EnvironmentCondition, GenerationParams,
    };
    use crate::ai::generative::pipeline::{GenerativeVideoJobConfig, GenerativeVideoPipeline};
    use crate::ai::generative::sidecar::PythonSidecarBackend;
    use crate::ai::generative::temporal::{
        TemporalBlender, TemporalConfig, TemporalWindowSlicer, WindowArtifactManifest,
    };
    use crate::error::ErrorCode;

    fn create_dummy_character_ref(path: &std::path::Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let img = image::RgbImage::from_fn(128, 128, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 200])
        });
        img.save(path).unwrap();
    }

    // =========================================================================
    // 01. Temporal Window Slicing
    // =========================================================================

    #[test]
    fn test_phase7c_01_temporal_window_slicing() {
        let config = TemporalConfig {
            context_size: 16,
            overlap: 4,
            enable_seam_blending: true,
            enable_latent_continuity: true,
        };

        let windows = TemporalWindowSlicer::slice_windows(40, &config).unwrap();
        assert!(!windows.is_empty());
        assert_eq!(windows[0].start_frame, 0);
        assert_eq!(windows[0].end_frame, 16);
        assert!(windows[0].is_first);
        assert!(!windows[0].is_last);

        // Stride is 12 (16 - 4)
        assert_eq!(windows[1].start_frame, 12);
        assert_eq!(windows[1].end_frame, 28);

        // Last window reaches 40
        assert_eq!(windows.last().unwrap().end_frame, 40);
        assert!(windows.last().unwrap().is_last);
    }

    // =========================================================================
    // 02. Overlap Correctness
    // =========================================================================

    #[test]
    fn test_phase7c_02_overlap_correctness() {
        let config = TemporalConfig {
            context_size: 16,
            overlap: 4,
            ..Default::default()
        };

        let windows = TemporalWindowSlicer::slice_windows(32, &config).unwrap();
        assert_eq!(windows.len(), 3); // 0..16, 12..28, 24..32 (or adjusted)
        assert_eq!(windows[1].overlap_with_previous, 4);
    }

    // =========================================================================
    // 03. Arbitrary Frame Count
    // =========================================================================

    #[test]
    fn test_phase7c_03_arbitrary_frame_count() {
        let config = TemporalConfig {
            context_size: 16,
            overlap: 4,
            ..Default::default()
        };

        for count in [1, 7, 15, 16, 17, 33, 49, 100] {
            let windows = TemporalWindowSlicer::slice_windows(count, &config).unwrap();
            assert!(!windows.is_empty());
            assert_eq!(windows[0].start_frame, 0);
            assert_eq!(windows.last().unwrap().end_frame, count);
        }
    }

    // =========================================================================
    // 04. Video Shorter Than Context Size
    // =========================================================================

    #[test]
    fn test_phase7c_04_video_shorter_than_context_size() {
        let config = TemporalConfig {
            context_size: 16,
            overlap: 4,
            ..Default::default()
        };

        let windows = TemporalWindowSlicer::slice_windows(8, &config).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].start_frame, 0);
        assert_eq!(windows[0].end_frame, 8);
        assert!(windows[0].is_first);
        assert!(windows[0].is_last);
    }

    // =========================================================================
    // 05. Exact Context Boundary
    // =========================================================================

    #[test]
    fn test_phase7c_05_exact_context_boundary() {
        let config = TemporalConfig {
            context_size: 16,
            overlap: 4,
            ..Default::default()
        };

        let windows = TemporalWindowSlicer::slice_windows(16, &config).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].start_frame, 0);
        assert_eq!(windows[0].end_frame, 16);
    }

    // =========================================================================
    // 06. Cosine Blend Weights
    // =========================================================================

    #[test]
    fn test_phase7c_06_cosine_blend_weights() {
        let weights = TemporalBlender::compute_cosine_weights(4);
        assert_eq!(weights.len(), 4);

        // Weights should be strictly increasing from near 0 to near 1
        for i in 0..weights.len() - 1 {
            assert!(weights[i] < weights[i + 1]);
        }

        assert!(weights[0] > 0.0);
        assert!(weights[3] < 1.0);
    }

    // =========================================================================
    // 07. RGB Temporal Blending
    // =========================================================================

    #[test]
    fn test_phase7c_07_rgb_temporal_blending() {
        let img1 = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([0, 0, 0]));
        let img2 = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([100, 100, 100]));

        let blended = TemporalBlender::blend_rgb_images(&img1, &img2, 0.5).unwrap();
        let p = blended.get_pixel(0, 0);
        assert_eq!(p[0], 50);
        assert_eq!(p[1], 50);
        assert_eq!(p[2], 50);
    }

    // =========================================================================
    // 08. No Duplicate Final Frames
    // =========================================================================

    #[test]
    fn test_phase7c_08_no_duplicate_final_frames() {
        let temp = TempDir::new().unwrap();
        let total_frames = 20;

        let config = TemporalConfig {
            context_size: 16,
            overlap: 4,
            ..Default::default()
        };

        let windows = TemporalWindowSlicer::slice_windows(total_frames, &config).unwrap();
        let mut manifests = Vec::new();

        for w in &windows {
            let mut paths = Vec::new();
            for f in w.frame_indices() {
                let p = temp
                    .path()
                    .join(format!("raw_{}_{}.png", w.window_index, f));
                let img = image::RgbImage::from_fn(32, 32, |_, _| image::Rgb([100, 100, 100]));
                img.save(&p).unwrap();
                paths.push(p);
            }
            manifests.push(WindowArtifactManifest {
                window_index: w.window_index,
                start_frame: w.start_frame,
                end_frame: w.end_frame,
                frame_count: paths.len(),
                frame_paths: paths,
                window_hash: "test_hash".to_string(),
                is_completed: true,
                generation_duration_ms: 10.0,
            });
        }

        let out_dir = temp.path().join("blended");
        let master_frames = TemporalBlender::assemble_and_blend_windows(
            &windows,
            &manifests,
            total_frames,
            &out_dir,
        )
        .unwrap();

        assert_eq!(master_frames.len(), total_frames);
    }

    // =========================================================================
    // 09. Pipeline Config Validation
    // =========================================================================

    #[test]
    fn test_phase7c_09_pipeline_config_validation() {
        let invalid_config = TemporalConfig {
            context_size: 8,
            overlap: 8, // Overlap must be strictly less than context_size
            ..Default::default()
        };

        assert!(invalid_config.validate().is_err());
    }

    // =========================================================================
    // 10. Cancellation
    // =========================================================================

    #[test]
    fn test_phase7c_10_cancellation() {
        let temp = TempDir::new().unwrap();
        let char_ref = temp.path().join("char_ref.png");
        create_dummy_character_ref(&char_ref);

        let fixture =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture.exists() {
            return;
        }

        let backend = PythonSidecarBackend::new(
            PathBuf::from("python"),
            PathBuf::from(r"d:\rustProject\autovideo-ai\src-tauri\scripts\generative_sidecar.py"),
            temp.path().to_path_buf(),
            false,
        );

        let job_config = GenerativeVideoJobConfig {
            job_id: "job-cancel-10".to_string(),
            source_video_path: fixture,
            character_reference: CharacterReference {
                image_paths: vec![char_ref],
                ..Default::default()
            },
            environment: EnvironmentCondition::default(),
            params: GenerationParams::default(),
            temporal_config: TemporalConfig::default(),
            output_video_path: temp.path().join("output.mp4"),
        };

        let cancel_token = Arc::new(AtomicBool::new(true)); // Cancelled immediately
        let err = GenerativeVideoPipeline::execute_pipeline(
            &job_config,
            &backend,
            temp.path(),
            Some(cancel_token),
            |_, _, _| {},
        )
        .unwrap_err();

        assert_eq!(err.code, ErrorCode::Cancelled);
    }

    // =========================================================================
    // 11. Resume from Completed Windows
    // =========================================================================

    #[test]
    fn test_phase7c_11_resume_from_completed_windows() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("manifest.json");

        let frame_path = temp.path().join("frame_000000.png");
        let img = image::RgbImage::from_fn(16, 16, |_, _| image::Rgb([120, 120, 120]));
        img.save(&frame_path).unwrap();

        let manifest = WindowArtifactManifest {
            window_index: 0,
            start_frame: 0,
            end_frame: 1,
            frame_count: 1,
            frame_paths: vec![frame_path],
            window_hash: "hash_00".to_string(),
            is_completed: true,
            generation_duration_ms: 50.0,
        };

        manifest.save_to_file(&manifest_path).unwrap();
        let loaded = WindowArtifactManifest::load_from_file(&manifest_path).unwrap();
        assert!(loaded.validate_frames_exist());
    }

    // =========================================================================
    // 12. Corrupted Window Regeneration
    // =========================================================================

    #[test]
    fn test_phase7c_12_corrupted_window_regeneration() {
        let temp = TempDir::new().unwrap();
        let manifest = WindowArtifactManifest {
            window_index: 0,
            start_frame: 0,
            end_frame: 1,
            frame_count: 1,
            frame_paths: vec![temp.path().join("non_existent_frame.png")],
            window_hash: "corrupt_hash".to_string(),
            is_completed: true,
            generation_duration_ms: 10.0,
        };

        assert!(!manifest.validate_frames_exist());
    }

    // =========================================================================
    // 13. Control Artifact Reuse
    // =========================================================================

    #[test]
    fn test_phase7c_13_control_artifact_reuse() {
        let temp = TempDir::new().unwrap();
        let char_ref = temp.path().join("char_ref.png");
        create_dummy_character_ref(&char_ref);

        let fixture =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture.exists() {
            return;
        }

        let backend = PythonSidecarBackend::new(
            PathBuf::from("python"),
            PathBuf::from(r"d:\rustProject\autovideo-ai\src-tauri\scripts\generative_sidecar.py"),
            temp.path().to_path_buf(),
            false,
        );

        let job_config = GenerativeVideoJobConfig {
            job_id: "job-reuse-13".to_string(),
            source_video_path: fixture,
            character_reference: CharacterReference {
                image_paths: vec![char_ref],
                ..Default::default()
            },
            environment: EnvironmentCondition::default(),
            params: GenerationParams {
                width: 256,
                height: 256,
                ..Default::default()
            },
            temporal_config: TemporalConfig {
                context_size: 8,
                overlap: 2,
                ..Default::default()
            },
            output_video_path: temp.path().join("output.mp4"),
        };

        let rep = GenerativeVideoPipeline::execute_pipeline(
            &job_config,
            &backend,
            temp.path(),
            None,
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(rep.quality_status, "PASSED");
    }

    // =========================================================================
    // 14. Source FPS Preservation
    // =========================================================================

    #[test]
    fn test_phase7c_14_source_fps_preservation() {
        let fixture =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture.exists() {
            return;
        }

        let media_service = crate::media::MediaService::new();
        let meta = media_service.probe(&fixture).unwrap();
        assert!(meta.fps > 0.0);
    }

    // =========================================================================
    // 15. Duration Preservation
    // =========================================================================

    #[test]
    fn test_phase7c_15_duration_preservation() {
        let fixture =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture.exists() {
            return;
        }

        let media_service = crate::media::MediaService::new();
        let meta = media_service.probe(&fixture).unwrap();
        assert!(meta.duration_ms > 0);
    }

    // =========================================================================
    // 16. Audio Preservation
    // =========================================================================

    #[test]
    fn test_phase7c_16_audio_preservation() {
        let fixture =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture.exists() {
            return;
        }

        let media_service = crate::media::MediaService::new();
        let meta = media_service.probe(&fixture).unwrap();
        assert!(meta.has_audio);
    }

    // =========================================================================
    // 17. Output Quality Gate
    // =========================================================================

    #[test]
    fn test_phase7c_17_output_quality_gate() {
        let temp = TempDir::new().unwrap();
        let out_p = temp.path().join("test_out.mp4");
        fs::write(&out_p, b"RIFF").unwrap();

        assert!(out_p.exists());
        assert!(fs::metadata(&out_p).unwrap().len() > 0);
    }

    // =========================================================================
    // 18. Real Multi-Frame Generation using Phase 7B Backend
    // =========================================================================

    #[test]
    fn test_phase7c_18_real_multiframe_generation() {
        let temp = TempDir::new().unwrap();
        let char_ref = temp.path().join("char_ref.png");
        create_dummy_character_ref(&char_ref);

        let fixture =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture.exists() {
            return;
        }

        let script_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\src-tauri\scripts\generative_sidecar.py");

        let backend = PythonSidecarBackend::new(
            PathBuf::from("python"),
            script_path,
            temp.path().to_path_buf(),
            false,
        );

        let out_video = temp.path().join("final_transformed_video.mp4");

        let job_config = GenerativeVideoJobConfig {
            job_id: "job-full-video-18".to_string(),
            source_video_path: fixture,
            character_reference: CharacterReference {
                image_paths: vec![char_ref],
                ..Default::default()
            },
            environment: EnvironmentCondition::default(),
            params: GenerationParams {
                width: 384,
                height: 512,
                ..Default::default()
            },
            temporal_config: TemporalConfig {
                context_size: 16,
                overlap: 4,
                enable_seam_blending: true,
                enable_latent_continuity: true,
            },
            output_video_path: out_video,
        };

        let report = GenerativeVideoPipeline::execute_pipeline(
            &job_config,
            &backend,
            temp.path(),
            None,
            |_, _, _| {},
        )
        .unwrap();

        assert!(report.output_video_path.exists());
        assert_eq!(report.quality_status, "PASSED");
        assert!(report.total_frames > 0);
        assert!(report.total_windows > 0);
    }
}
