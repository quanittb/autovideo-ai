#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::ai::generative::gate::{
        compute_sha256, GenerationTelemetry, HardwareAdaptiveProfile, ModelArtifactSpec,
        ProductionGateErrorCode, ProductionModelGate, ProductionModelManifest,
    };
    use crate::media::MediaService;

    const MANDATORY_VIDEO_PATH: &str = r"C:\Users\quant\Dropbox\PC\Downloads\Douyin_1782229041.mp4";
    const MANDATORY_CHAR_PATH: &str = r"C:\Users\quant\Dropbox\PC\Downloads\QuanPH.png";

    // =========================================================================
    // 01. Mandatory Source Video Audit
    // =========================================================================

    #[test]
    fn test_phase7d_01_mandatory_source_video_audit() {
        let video_path = PathBuf::from(MANDATORY_VIDEO_PATH);
        if !video_path.exists() {
            eprintln!("Skipping test: mandatory video fixture not found");
            return;
        }

        let media_service = MediaService::new();
        let meta = media_service.probe(&video_path).unwrap();

        assert_eq!(meta.width, 576);
        assert_eq!(meta.height, 1024);
        assert!((meta.fps - 30.0).abs() < 0.1);
        assert!(meta.duration_ms > 24000 && meta.duration_ms < 25000);
        assert!(meta.has_audio);

        let hash = compute_sha256(&video_path).unwrap();
        assert_eq!(
            hash.to_lowercase(),
            "8910cbd03d94c742f37551c118d390ad10dd3a2b2b5a6239fa80764e8482daad"
        );
    }

    // =========================================================================
    // 02. Mandatory Character Reference Audit
    // =========================================================================

    #[test]
    fn test_phase7d_02_mandatory_character_ref_audit() {
        let char_path = PathBuf::from(MANDATORY_CHAR_PATH);
        if !char_path.exists() {
            eprintln!("Skipping test: mandatory character fixture not found");
            return;
        }

        let img = image::open(&char_path).unwrap();
        assert_eq!(img.width(), 1254);
        assert_eq!(img.height(), 1254);

        let hash = compute_sha256(&char_path).unwrap();
        assert_eq!(
            hash.to_lowercase(),
            "037918d8c85a88d656ba4d2641f93374bffe6b246fbcbce96ea26a9a2faa2386"
        );
    }

    // =========================================================================
    // 03. Hardware Adaptive Profile Selection
    // =========================================================================

    #[test]
    fn test_phase7d_03_hardware_adaptive_profile_selection() {
        // 4GB VRAM (GTX 1650)
        let p4 = HardwareAdaptiveProfile::for_vram(4096, 3200);
        assert_eq!(p4.profile_name, "Profile4GB");
        assert_eq!(p4.target_width, 288);
        assert_eq!(p4.target_height, 512);
        assert_eq!(p4.context_size, 8);
        assert!(p4.enable_cpu_offload);
        assert!(p4.enable_vae_slicing);

        // 8GB VRAM (RTX 3070/4060)
        let p8 = HardwareAdaptiveProfile::for_vram(8192, 6000);
        assert_eq!(p8.profile_name, "Profile6To8GB");
        assert_eq!(p8.target_width, 512);
        assert_eq!(p8.target_height, 768);
        assert_eq!(p8.context_size, 16);

        // 16GB VRAM (RTX 4080/4090)
        let p16 = HardwareAdaptiveProfile::for_vram(16384, 12000);
        assert_eq!(p16.profile_name, "Profile12GBPlus");
        assert_eq!(p16.target_width, 576);
        assert_eq!(p16.target_height, 1024);
        assert_eq!(p16.context_size, 16);
    }

    // =========================================================================
    // 04. Production Model Manifest Defaults
    // =========================================================================

    #[test]
    fn test_phase7d_04_production_model_manifest_defaults() {
        let manifest = ProductionModelManifest::animatediff_sd15_default();
        assert_eq!(manifest.model_id, "animatediff_sd15_v3");
        assert_eq!(manifest.version, "3.0.0");
        assert!(!manifest.artifacts.is_empty());
        assert!(manifest
            .artifacts
            .iter()
            .any(|a| a.name.contains("Base SD1.5")));
        assert!(manifest
            .artifacts
            .iter()
            .any(|a| a.name.contains("Motion Module")));
        assert!(manifest
            .artifacts
            .iter()
            .any(|a| a.name.contains("OpenPose")));
        assert!(manifest
            .artifacts
            .iter()
            .any(|a| a.name.contains("IP-Adapter")));
    }

    // =========================================================================
    // 05. Gate Rejection: Missing Artifacts
    // =========================================================================

    #[test]
    fn test_phase7d_05_gate_rejection_missing_artifacts() {
        let temp = TempDir::new().unwrap();
        let manifest = ProductionModelManifest::animatediff_sd15_default();

        let (err_code, msg) = manifest.verify_integrity(temp.path()).unwrap_err();
        assert_eq!(
            err_code,
            ProductionGateErrorCode::ProductionModelUnavailable
        );
        assert!(msg.contains("missing at"));
    }

    // =========================================================================
    // 06. Gate Rejection: Insufficient Hardware
    // =========================================================================

    #[test]
    fn test_phase7d_06_gate_rejection_insufficient_hardware() {
        let profile = HardwareAdaptiveProfile::for_vram(16384, 12000);

        // No CUDA
        let (err_code, _) =
            ProductionModelGate::validate_hardware(false, None, None, None, &profile).unwrap_err();
        assert_eq!(
            err_code,
            ProductionGateErrorCode::ProductionModelHardwareBlocked
        );

        // Insufficient VRAM for 12GB+ profile
        let (err_code, msg) = ProductionModelGate::validate_hardware(
            true,
            Some("NVIDIA GeForce GTX 1650"),
            Some(4096),
            Some(3000),
            &profile,
        )
        .unwrap_err();
        assert_eq!(
            err_code,
            ProductionGateErrorCode::ProductionModelHardwareBlocked
        );
        assert!(msg.contains("below profile requirement"));
    }

    // =========================================================================
    // 07. Gate Rejection: SHA-256 Mismatch
    // =========================================================================

    #[test]
    fn test_phase7d_07_gate_rejection_sha256_mismatch() {
        let temp = TempDir::new().unwrap();
        let dummy_model_path = temp.path().join("sd15/v1-5-pruned-emaonly.safetensors");
        fs::create_dir_all(dummy_model_path.parent().unwrap()).unwrap();
        fs::write(&dummy_model_path, b"corrupted_weights").unwrap();

        let manifest = ProductionModelManifest {
            model_id: "test".to_string(),
            version: "1.0.0".to_string(),
            base_model: "sd15".to_string(),
            motion_module: "mm".to_string(),
            pose_controlnet: "pose".to_string(),
            depth_controlnet: None,
            ip_adapter: "ip".to_string(),
            face_encoder: "enc".to_string(),
            vae: "vae".to_string(),
            text_encoder: "text".to_string(),
            precision: "fp16".to_string(),
            expected_vram_mb: 4000,
            supported_resolutions: vec![[512, 512]],
            supported_context_sizes: vec![16],
            artifacts: vec![ModelArtifactSpec {
                name: "Corrupt Checkpoint".to_string(),
                relative_path: PathBuf::from("sd15/v1-5-pruned-emaonly.safetensors"),
                expected_sha256: Some(
                    "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                ),
                size_bytes: Some(17),
                is_mandatory: true,
            }],
        };

        let (err_code, msg) = manifest.verify_integrity(temp.path()).unwrap_err();
        assert_eq!(
            err_code,
            ProductionGateErrorCode::ProductionModelIntegrityFailed
        );
        assert!(msg.contains("SHA-256 mismatch"));
    }

    // =========================================================================
    // 08. Motion, Identity & Temporal Quality Metrics Evaluation
    // =========================================================================

    #[test]
    fn test_phase7d_08_motion_identity_temporal_metrics_evaluation() {
        let temp = TempDir::new().unwrap();
        let mut frame_paths = Vec::new();

        for i in 0..16 {
            let p = temp.path().join(format!("frame_{:06}.png", i));
            let img = image::RgbImage::from_fn(64, 64, |x, y| {
                image::Rgb([(x * 2) as u8, (y * 2) as u8, 150])
            });
            img.save(&p).unwrap();
            frame_paths.push(p);
        }

        let char_p = temp.path().join("char.png");
        let char_img = image::RgbImage::from_fn(64, 64, |_, _| image::Rgb([200, 200, 200]));
        char_img.save(&char_p).unwrap();

        let metrics = ProductionModelGate::evaluate_quality_metrics(
            &frame_paths,
            &frame_paths,
            &char_p,
            30.0,
            30.0,
            500,
            500,
            true,
        )
        .unwrap();

        assert_eq!(metrics.black_frame_count, 0);
        assert_eq!(metrics.corrupted_frame_count, 0);
        assert!(metrics.fps_match);
        assert!(metrics.audio_preserved);
        assert_eq!(metrics.duration_delta_ms, 0);
        assert!(metrics.motion_preservation_score > 0.8);
        assert!(metrics.character_identity_score > 0.8);
        assert!(metrics.temporal_consistency_score > 0.8);
    }

    // =========================================================================
    // 09. Telemetry Serialization
    // =========================================================================

    #[test]
    fn test_phase7d_09_telemetry_serialization() {
        let telemetry = GenerationTelemetry {
            model_name: "AnimateDiff-SD15-v3".to_string(),
            model_version: "3.0.0".to_string(),
            gpu_name: "NVIDIA GeForce GTX 1650".to_string(),
            vram_total_mb: 4096,
            vram_peak_mb: 3450,
            cuda_version: "11.7".to_string(),
            precision: "fp16".to_string(),
            resolution: "384x512".to_string(),
            context_frames: 8,
            overlap_frames: 2,
            frames_generated: 16,
            generation_fps: 2.5,
            model_load_duration_ms: 1200.0,
            inference_duration_ms: 6400.0,
            motion_preservation_score: 0.92,
            character_identity_score: 0.88,
            temporal_consistency_score: 0.94,
        };

        let json = serde_json::to_string_pretty(&telemetry).unwrap();
        assert!(json.contains("AnimateDiff-SD15-v3"));
        assert!(json.contains("NVIDIA GeForce GTX 1650"));
        assert!(json.contains("motionPreservationScore"));
    }

    // =========================================================================
    // 10. Zero-Fake Policy Enforcement
    // =========================================================================

    #[test]
    fn test_phase7d_10_zero_fake_policy_enforcement() {
        let manifest = ProductionModelManifest::animatediff_sd15_default();
        let non_existent_models_dir = PathBuf::from(r"C:\non_existent_models_path_12345");

        let result = manifest.verify_integrity(&non_existent_models_dir);
        assert!(result.is_err());
        let (code, msg) = result.unwrap_err();
        assert_eq!(code, ProductionGateErrorCode::ProductionModelUnavailable);
        assert!(!msg.is_empty());
    }
}
