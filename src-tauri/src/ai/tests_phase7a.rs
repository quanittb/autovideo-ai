#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::ai::control::depth::{DepthExtractor, DepthExtractorConfig};
    use crate::ai::control::extractor::{ControlExtractionConfig, ControlExtractor};
    use crate::ai::control::package::{ControlArtifactPaths, VideoControlPackage};
    use crate::ai::control::pose::{Keypoint2D, PoseExtractor, PoseExtractorConfig};
    use crate::ai::control::segmentation::{SegmentationExtractor, SegmentationExtractorConfig};
    use crate::ai::frame_pipeline::reconstruct::RationalFps;
    use crate::error::ErrorCode;

    // =========================================================================
    // 1. Pose Extractor & Skeleton Rendering
    // =========================================================================

    #[test]
    fn test_phase7a_01_pose_extractor_specs_and_rendering() {
        let temp = TempDir::new().unwrap();
        let config = PoseExtractorConfig::default();
        let extractor = PoseExtractor::new(config.clone());

        let keypoints = vec![
            Keypoint2D {
                x: 0.5,
                y: 0.2,
                score: 0.95,
            },
            Keypoint2D {
                x: 0.5,
                y: 0.3,
                score: 0.95,
            },
            Keypoint2D {
                x: 0.4,
                y: 0.3,
                score: 0.90,
            },
            Keypoint2D {
                x: 0.3,
                y: 0.5,
                score: 0.10,
            }, // Below threshold
        ];

        let out_path = temp.path().join("pose_000000.png");
        let res = extractor
            .extract_frame(0, 384, 288, &keypoints, &out_path)
            .unwrap();

        assert!(out_path.exists());
        assert_eq!(res.keypoints_detected, 3);
        assert!(!res.is_reused);

        let img = image::open(&out_path).unwrap().to_rgb8();
        assert_eq!(img.width(), 384);
        assert_eq!(img.height(), 288);
    }

    // =========================================================================
    // 2. Depth Extractor & Metric Normalization
    // =========================================================================

    #[test]
    fn test_phase7a_02_depth_extractor_metric_normalization() {
        let temp = TempDir::new().unwrap();
        let config = DepthExtractorConfig::default();
        let extractor = DepthExtractor::new(config);

        let raw_depth = vec![10.0f32, 20.0f32, 30.0f32, 40.0f32];
        let out_path = temp.path().join("depth_000000.png");

        let res = extractor
            .extract_frame(0, &raw_depth, 2, 2, &out_path)
            .unwrap();

        assert!(out_path.exists());
        assert_eq!(res.min_depth, 10.0);
        assert_eq!(res.max_depth, 40.0);
        assert_eq!(res.mean_depth, 25.0);

        let img = image::open(&out_path).unwrap().to_luma8();
        assert_eq!(img.get_pixel(0, 0)[0], 0); // min -> 0
        assert_eq!(img.get_pixel(1, 1)[0], 255); // max -> 255
    }

    // =========================================================================
    // 3. Segmentation Extractor & Alpha Matte
    // =========================================================================

    #[test]
    fn test_phase7a_03_segmentation_extractor_alpha_matte() {
        let temp = TempDir::new().unwrap();
        let config = SegmentationExtractorConfig {
            threshold: 0.5,
            binary_mask: true,
            ..Default::default()
        };
        let extractor = SegmentationExtractor::new(config);

        let raw_probs = vec![0.1f32, 0.4f32, 0.6f32, 0.9f32];
        let out_path = temp.path().join("mask_000000.png");

        let res = extractor
            .extract_frame(0, &raw_probs, 2, 2, &out_path)
            .unwrap();

        assert!(out_path.exists());
        assert_eq!(res.foreground_ratio, 0.5); // 2 out of 4 pixels >= 0.5
        assert_eq!(res.mean_probability, 0.5);

        let img = image::open(&out_path).unwrap().to_luma8();
        assert_eq!(img.get_pixel(0, 0)[0], 0); // 0.1 -> 0
        assert_eq!(img.get_pixel(1, 1)[0], 255); // 0.9 -> 255
    }

    // =========================================================================
    // 4. VideoControlPackage Manifest & Hashing
    // =========================================================================

    #[test]
    fn test_phase7a_04_control_package_manifest_and_hashing() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("control_package.json");

        let pose_dir = temp.path().join("poses");
        let depth_dir = temp.path().join("depths");
        let mask_dir = temp.path().join("masks");
        fs::create_dir_all(&pose_dir).unwrap();
        fs::create_dir_all(&depth_dir).unwrap();
        fs::create_dir_all(&mask_dir).unwrap();

        let artifacts = ControlArtifactPaths {
            pose_frames_dir: Some(pose_dir),
            depth_frames_dir: Some(depth_dir),
            mask_frames_dir: Some(mask_dir),
            audio_file_path: None,
        };

        let pkg = VideoControlPackage::new(
            "job-ctrl-04",
            "C:/Videos/source.mp4",
            "sha256_source_video",
            1080,
            1920,
            RationalFps::new(30, 1),
            90,
            3000,
            artifacts,
            Some("pose_hash_abc".to_string()),
            Some("depth_hash_def".to_string()),
            Some("mask_hash_ghi".to_string()),
            None,
        );

        assert!(!pkg.package_hash.is_empty());
        assert!(pkg.validate_artifacts().is_ok());

        // Test persistence roundtrip
        pkg.save_to_file(&manifest_path).unwrap();
        let loaded = VideoControlPackage::load_from_file(&manifest_path).unwrap();
        assert_eq!(pkg.job_id, loaded.job_id);
        assert_eq!(pkg.package_hash, loaded.package_hash);
        assert_eq!(pkg.total_frames, 90);
    }

    // =========================================================================
    // 5. Independent Cache Invalidation
    // =========================================================================

    #[test]
    fn test_phase7a_05_independent_cache_invalidation() {
        let pose_cfg1 = PoseExtractorConfig::default();
        let depth_cfg1 = DepthExtractorConfig {
            invert: false,
            ..Default::default()
        };
        let depth_cfg2 = DepthExtractorConfig {
            invert: true,
            ..Default::default()
        }; // Modified
        let mask_cfg1 = SegmentationExtractorConfig::default();

        let hash_pose1 = pose_cfg1.compute_hash();
        let hash_depth1 = depth_cfg1.compute_hash();
        let hash_depth2 = depth_cfg2.compute_hash();
        let hash_mask1 = mask_cfg1.compute_hash();

        assert_ne!(hash_depth1, hash_depth2);

        let pkg_hash1 = VideoControlPackage::compute_package_hash(
            "src_hash",
            Some(&hash_pose1),
            Some(&hash_depth1),
            Some(&hash_mask1),
            None,
        );

        let pkg_hash2 = VideoControlPackage::compute_package_hash(
            "src_hash",
            Some(&hash_pose1),
            Some(&hash_depth2), // Only depth changed
            Some(&hash_mask1),
            None,
        );

        assert_ne!(pkg_hash1, pkg_hash2);
    }

    // =========================================================================
    // 6. Real Sample Video Control Extraction
    // =========================================================================

    #[test]
    fn test_phase7a_06_real_sample_video_control_extraction() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let extractor = ControlExtractor::new(
            ControlExtractionConfig::default(),
            temp.path().to_path_buf(),
        );

        let (pkg, report) = extractor
            .extract_package("job-real-06", &fixture_path, None, |_, _, _| {})
            .unwrap();

        assert!(pkg.is_valid);
        assert_eq!(report.job_id, "job-real-06");
        assert!(pkg.total_frames > 0);
        assert!(pkg.artifacts.pose_frames_dir.is_some());
        assert!(pkg.artifacts.depth_frames_dir.is_some());
        assert!(pkg.artifacts.mask_frames_dir.is_some());
        assert!(pkg.validate_artifacts().is_ok());
    }

    // =========================================================================
    // 7. Audio Stream Preservation in Package
    // =========================================================================

    #[test]
    fn test_phase7a_07_audio_stream_preservation_in_package() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let extractor = ControlExtractor::new(
            ControlExtractionConfig {
                preserve_audio: true,
                ..Default::default()
            },
            temp.path().to_path_buf(),
        );

        let (pkg, _) = extractor
            .extract_package("job-audio-07", &fixture_path, None, |_, _, _| {})
            .unwrap();

        assert!(pkg.audio_hash.is_some());
        assert!(pkg.artifacts.audio_file_path.is_some());
        assert!(pkg.artifacts.audio_file_path.as_ref().unwrap().exists());
    }

    // =========================================================================
    // 8. Cancellation During Control Extraction
    // =========================================================================

    #[test]
    fn test_phase7a_08_cancellation_during_control_extraction() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let extractor = ControlExtractor::new(
            ControlExtractionConfig::default(),
            temp.path().to_path_buf(),
        );

        let cancel_token = Arc::new(AtomicBool::new(true)); // Cancelled immediately
        let err = extractor
            .extract_package(
                "job-cancel-08",
                &fixture_path,
                Some(cancel_token),
                |_, _, _| {},
            )
            .unwrap_err();

        assert_eq!(err.code, ErrorCode::Cancelled);
    }

    // =========================================================================
    // 9. Resumption with 100% Cache Hit
    // =========================================================================

    #[test]
    fn test_phase7a_09_resumption_with_100_percent_cache_hit() {
        let fixture_path =
            PathBuf::from(r"d:\rustProject\autovideo-ai\.autovideo_data\sample_portrait_video.mp4");
        if !fixture_path.exists() {
            return;
        }

        let temp = TempDir::new().unwrap();
        let extractor = ControlExtractor::new(
            ControlExtractionConfig::default(),
            temp.path().to_path_buf(),
        );

        // First pass: extract all
        let (_, r1) = extractor
            .extract_package("job-cache-09", &fixture_path, None, |_, _, _| {})
            .unwrap();
        assert_eq!(r1.cache_hits_count, 0);

        // Second pass: full cache hit
        let (_, r2) = extractor
            .extract_package("job-cache-09", &fixture_path, None, |_, _, _| {})
            .unwrap();
        assert!(r2.cache_hits_count > 0);
    }

    // =========================================================================
    // 10. Corrupted Control Artifact Detection
    // =========================================================================

    #[test]
    fn test_phase7a_10_corrupted_control_artifact_detection() {
        let temp = TempDir::new().unwrap();
        let non_existent_pose = temp.path().join("missing_poses_dir");

        let artifacts = ControlArtifactPaths {
            pose_frames_dir: Some(non_existent_pose),
            depth_frames_dir: None,
            mask_frames_dir: None,
            audio_file_path: None,
        };

        let pkg = VideoControlPackage::new(
            "job-corrupt-10",
            "C:/Videos/source.mp4",
            "src_hash",
            1080,
            1920,
            RationalFps::new(30, 1),
            30,
            1000,
            artifacts,
            None,
            None,
            None,
            None,
        );

        let err = pkg.validate_artifacts().unwrap_err();
        assert_eq!(err.code, ErrorCode::FileNotFound);
    }
}
