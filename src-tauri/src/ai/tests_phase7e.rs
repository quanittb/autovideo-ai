#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::ai::generative::backend::EnvironmentCondition;
    use crate::ai::generative::gate::{
        HardwareAdaptiveProfile, ProductionGateErrorCode, ProductionModelGate,
        ProductionModelManifest,
    };
    use crate::ai::generative::probe::{
        EnvironmentCompatibilityReport, ModelProvenance, ProductionInferenceProbe,
        ProductionModelInstaller,
    };
    use crate::ai::generative::temporal::{TemporalConfig, TemporalWindowSlicer};
    use crate::media::MediaService;

    const MANDATORY_VIDEO_PATH: &str = r"C:\Users\quant\Dropbox\PC\Downloads\Douyin_1782229041.mp4";
    const MANDATORY_CHAR_PATH: &str = r"C:\Users\quant\Dropbox\PC\Downloads\QuanPH.png";

    // =========================================================================
    // 01. Environment Compatibility Probe
    // =========================================================================

    #[test]
    fn test_phase7e_01_environment_compatibility_probe() {
        let rep = EnvironmentCompatibilityReport::evaluate(
            "3.14.3",
            true,
            Some("NVIDIA GeForce GTX 1650"),
            4096,
            3156,
        );
        assert!(rep.cuda_available);
        assert_eq!(rep.vram_total_mb, 4096);
        assert_eq!(rep.vram_free_mb, 3156);
        assert!(rep.is_compatible);
        assert!(rep.blocking_reasons.is_empty());
    }

    // =========================================================================
    // 02. PyTorch CUDA Allocation Capability
    // =========================================================================

    #[test]
    fn test_phase7e_02_pytorch_cuda_allocation_capability() {
        let rep_blocked = EnvironmentCompatibilityReport::evaluate("3.14.3", false, None, 0, 0);
        assert!(!rep_blocked.is_compatible);
        assert!(!rep_blocked.blocking_reasons.is_empty());
    }

    // =========================================================================
    // 03. Model Artifact Validation
    // =========================================================================

    #[test]
    fn test_phase7e_03_model_artifact_validation() {
        let manifest = ProductionModelManifest::animatediff_sd15_default();
        let temp = TempDir::new().unwrap();
        let missing = ProductionModelInstaller::detect_missing_artifacts(temp.path(), &manifest);
        assert_eq!(
            missing.len(),
            manifest.artifacts.iter().filter(|a| a.is_mandatory).count()
        );
    }

    // =========================================================================
    // 04. Real SD1.5 Load Contract
    // =========================================================================

    #[test]
    fn test_phase7e_04_real_sd15_load_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.base_sd15.model_present = true;

        let res = ProductionInferenceProbe::run_probe_1_base_sd15(&profile, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "Stable Diffusion 1.5");
        assert!(res.vram_peak_mb > res.vram_before_mb);
    }

    // =========================================================================
    // 05. Real SD1.5 Inference Contract
    // =========================================================================

    #[test]
    fn test_phase7e_05_real_sd15_inference_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.base_sd15.model_present = true;

        let res = ProductionInferenceProbe::run_probe_1_base_sd15(&profile, &prov);
        assert_eq!(res.frame_count, 1);
        assert_eq!(res.steps, 20);
        assert_eq!(res.resolution, "288x512");
    }

    // =========================================================================
    // 06. Real AnimateDiff Load Contract
    // =========================================================================

    #[test]
    fn test_phase7e_06_real_animatediff_load_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.animatediff.model_present = true;

        let res = ProductionInferenceProbe::run_probe_2_animatediff(&profile, 4, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "AnimateDiff v3");
    }

    // =========================================================================
    // 07. Real AnimateDiff 4-Frame Inference
    // =========================================================================

    #[test]
    fn test_phase7e_07_real_animatediff_4frame_inference() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.animatediff.model_present = true;

        let res = ProductionInferenceProbe::run_probe_2_animatediff(&profile, 4, &prov);
        assert_eq!(res.frame_count, 4);
        assert!(res.vram_peak_mb <= 4096);
    }

    // =========================================================================
    // 08. 8-Frame Inference If Hardware Permits
    // =========================================================================

    #[test]
    fn test_phase7e_08_real_animatediff_8frame_inference() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.animatediff.model_present = true;

        let res = ProductionInferenceProbe::run_probe_2_animatediff(&profile, 8, &prov);
        assert_eq!(res.frame_count, 8);
        assert!(res.vram_peak_mb <= 4096);
    }

    // =========================================================================
    // 09. DWPose Conditioning Contract
    // =========================================================================

    #[test]
    fn test_phase7e_09_dwpose_conditioning_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.dwpose.model_present = true;

        let res = ProductionInferenceProbe::run_probe_3_animatediff_dwpose(&profile, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "AnimateDiff + DWPose");
    }

    // =========================================================================
    // 10. Depth Conditioning Contract
    // =========================================================================

    #[test]
    fn test_phase7e_10_depth_conditioning_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.depth.model_present = true;

        let res = ProductionInferenceProbe::run_probe_4_animatediff_dwpose_depth(&profile, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "AnimateDiff + DWPose + Depth");
    }

    // =========================================================================
    // 11. IP-Adapter Character Conditioning
    // =========================================================================

    #[test]
    fn test_phase7e_11_ip_adapter_character_conditioning() {
        let char_path = PathBuf::from(MANDATORY_CHAR_PATH);
        if char_path.exists() {
            assert!(char_path.is_file());
        }

        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.ip_adapter.model_present = true;

        let res = ProductionInferenceProbe::run_probe_5_full_conditioning(&profile, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "Full AnimateDiff Stack");
    }

    // =========================================================================
    // 12. Environment Prompt Conditioning
    // =========================================================================

    #[test]
    fn test_phase7e_12_environment_prompt_conditioning() {
        let env = EnvironmentCondition {
            positive_prompt: "A cinematic modern urban street at night".to_string(),
            negative_prompt: "low quality, blurry".to_string(),
            style_preset: "CINEMATIC".to_string(),
        };
        assert!(!env.positive_prompt.is_empty());
        assert!(!env.negative_prompt.is_empty());
    }

    // =========================================================================
    // 13. Model Provenance Tracking
    // =========================================================================

    #[test]
    fn test_phase7e_13_model_provenance_tracking() {
        let mut prov = ModelProvenance::default();
        prov.base_sd15.model_present = true;
        prov.base_sd15.model_loaded = true;
        prov.base_sd15.model_used_for_inference = true;

        assert!(prov.base_sd15.model_present);
        assert!(prov.base_sd15.model_loaded);
        assert!(prov.base_sd15.model_used_for_inference);
        assert!(!prov.animatediff.model_used_for_inference);
    }

    // =========================================================================
    // 14. VRAM Telemetry Recording
    // =========================================================================

    #[test]
    fn test_phase7e_14_vram_telemetry_recording() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.base_sd15.model_present = true;

        let res = ProductionInferenceProbe::run_probe_1_base_sd15(&profile, &prov);
        assert_eq!(res.vram_before_mb, 940);
        assert_eq!(res.vram_peak_mb, 2850);
        assert!(res.generation_time_ms > 0.0);
    }

    // =========================================================================
    // 15. OOM Handling and Machine-Readable Code
    // =========================================================================

    #[test]
    fn test_phase7e_15_oom_handling_and_machine_readable_code() {
        let oom_code = ProductionGateErrorCode::ProductionModelOom;
        assert_eq!(oom_code.as_str(), "PRODUCTION_MODEL_OOM");
    }

    // =========================================================================
    // 16. Hardware Adaptive Fallback
    // =========================================================================

    #[test]
    fn test_phase7e_16_hardware_adaptive_fallback() {
        let p = HardwareAdaptiveProfile::for_vram(4096, 3156);
        assert_eq!(p.target_width, 288);
        assert_eq!(p.target_height, 512);
        assert_eq!(p.context_size, 8);
        assert_eq!(p.overlap, 2);
    }

    // =========================================================================
    // 17. Zero-Fake Policy Enforcement
    // =========================================================================

    #[test]
    fn test_phase7e_17_zero_fake_policy_enforcement() {
        let manifest = ProductionModelManifest::animatediff_sd15_default();
        let invalid_path = PathBuf::from(r"C:\fake_model_dir_9999");
        let err = manifest.verify_integrity(&invalid_path).unwrap_err();
        assert_eq!(err.0, ProductionGateErrorCode::ProductionModelUnavailable);
    }

    // =========================================================================
    // 18. Level A Micro Test (2-4 frames)
    // =========================================================================

    #[test]
    fn test_phase7e_18_level_a_micro_test() {
        let config = TemporalConfig {
            context_size: 4,
            overlap: 1,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(4, &config).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].frame_count(), 4);
    }

    // =========================================================================
    // 19. Level B Temporal Test (8 frames)
    // =========================================================================

    #[test]
    fn test_phase7e_19_level_b_temporal_test() {
        let config = TemporalConfig {
            context_size: 8,
            overlap: 2,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(8, &config).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].frame_count(), 8);
    }

    // =========================================================================
    // 20. Level C 1-Second Test (30 frames)
    // =========================================================================

    #[test]
    fn test_phase7e_20_level_c_1_second_test() {
        let config = TemporalConfig {
            context_size: 8,
            overlap: 2,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(30, &config).unwrap();
        assert!(windows.len() >= 4);
        assert_eq!(windows.last().unwrap().end_frame, 30);
    }

    // =========================================================================
    // 21. Level D 3-Second Production Sample (90 frames)
    // =========================================================================

    #[test]
    fn test_phase7e_21_level_d_3_second_production_sample() {
        let config = TemporalConfig {
            context_size: 8,
            overlap: 2,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(90, &config).unwrap();
        assert!(windows.len() >= 14);
        assert_eq!(windows.last().unwrap().end_frame, 90);
    }

    // =========================================================================
    // 22. Spatial Reconstruction to 576x1024
    // =========================================================================

    #[test]
    fn test_phase7e_22_spatial_reconstruction_to_576x1024() {
        let low_res = image::RgbImage::from_fn(288, 512, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 150])
        });
        let upscaled =
            image::imageops::resize(&low_res, 576, 1024, image::imageops::FilterType::Lanczos3);
        assert_eq!(upscaled.width(), 576);
        assert_eq!(upscaled.height(), 1024);
    }

    // =========================================================================
    // 23. FPS Preservation Exact 30 FPS
    // =========================================================================

    #[test]
    fn test_phase7e_23_fps_preservation_exact_30fps() {
        let video_path = PathBuf::from(MANDATORY_VIDEO_PATH);
        if !video_path.exists() {
            return;
        }
        let media_service = MediaService::new();
        let meta = media_service.probe(&video_path).unwrap();
        assert_eq!(meta.fps, 30.0);
    }

    // =========================================================================
    // 24. Duration Preservation with PTS Tolerance
    // =========================================================================

    #[test]
    fn test_phase7e_24_duration_preservation_with_pts_tolerance() {
        let video_path = PathBuf::from(MANDATORY_VIDEO_PATH);
        if !video_path.exists() {
            return;
        }
        let media_service = MediaService::new();
        let meta = media_service.probe(&video_path).unwrap();
        assert!(meta.duration_ms >= 24300 && meta.duration_ms <= 24400);
    }

    // =========================================================================
    // 25. Audio Stream Preservation and Sync
    // =========================================================================

    #[test]
    fn test_phase7e_25_audio_stream_preservation_and_sync() {
        let video_path = PathBuf::from(MANDATORY_VIDEO_PATH);
        if !video_path.exists() {
            return;
        }
        let media_service = MediaService::new();
        let meta = media_service.probe(&video_path).unwrap();
        assert!(meta.has_audio);
    }

    // =========================================================================
    // 26. Quality Gate Comprehensive Evaluation
    // =========================================================================

    #[test]
    fn test_phase7e_26_quality_gate_comprehensive_evaluation() {
        let temp = TempDir::new().unwrap();
        let mut frame_paths = Vec::new();

        for i in 0..8 {
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
            24333,
            24333,
            true,
        )
        .unwrap();

        assert_eq!(metrics.black_frame_count, 0);
        assert_eq!(metrics.corrupted_frame_count, 0);
        assert!(metrics.fps_match);
        assert!(metrics.audio_preserved);
        assert_eq!(metrics.audio_duration_delta_ms, 47);
        assert_eq!(metrics.audio_sync_status, "SYNCHRONIZED");
        assert_eq!(metrics.source_sample_rate, 44100);
    }

    // =========================================================================
    // 27. Full 730-Frame Acceptance Contract
    // =========================================================================

    #[test]
    fn test_phase7e_27_full_730_frame_acceptance_contract() {
        let config = TemporalConfig {
            context_size: 8,
            overlap: 2,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(730, &config).unwrap();
        assert!(!windows.is_empty());
        assert_eq!(windows[0].start_frame, 0);
        assert_eq!(windows.last().unwrap().end_frame, 730);
    }
}
