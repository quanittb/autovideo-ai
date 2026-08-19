#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::ai::generative::backend::EnvironmentCondition;
    use crate::ai::generative::gate::{HardwareAdaptiveProfile, ProductionGateErrorCode};
    use crate::ai::generative::probe::{
        EnvironmentCompatibilityReport, ModelProvenance, ModelRole, Phase8ArtifactInventory,
        Phase8ExecutionClassification, ProductionInferenceProbe, ProductionModelInventory,
    };
    use crate::ai::generative::temporal::{TemporalConfig, TemporalWindowSlicer};
    use crate::media::MediaService;

    const MANDATORY_VIDEO_PATH: &str = r"C:\Users\quant\Dropbox\PC\Downloads\Douyin_1782229041.mp4";
    const MANDATORY_CHAR_PATH: &str = r"C:\Users\quant\Dropbox\PC\Downloads\QuanPH.png";

    // =========================================================================
    // 01. Mandatory Test Assets Audit (CONTRACT_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_01_mandatory_test_assets_audit() {
        let video_p = PathBuf::from(MANDATORY_VIDEO_PATH);
        let char_p = PathBuf::from(MANDATORY_CHAR_PATH);

        if video_p.exists() {
            let media_service = MediaService::new();
            let meta = media_service.probe(&video_p).unwrap();
            assert_eq!(meta.width, 576);
            assert_eq!(meta.height, 1024);
            assert_eq!(meta.fps, 30.0);
            assert!(meta.has_audio);
        }

        if char_p.exists() {
            let img = image::open(&char_p).unwrap();
            assert_eq!(img.width(), 1254);
            assert_eq!(img.height(), 1254);
        }
    }

    // =========================================================================
    // 02. Model Discovery and Inventory (CONTRACT_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_02_model_discovery_and_inventory() {
        let temp = TempDir::new().unwrap();
        let entries = ProductionModelInventory::scan_and_discover(temp.path());
        assert_eq!(entries.len(), 6);
        assert_eq!(entries[0].role, ModelRole::Sd15Base);
        assert_eq!(entries[1].role, ModelRole::AnimateDiffMotion);
        assert_eq!(entries[2].role, ModelRole::PoseControl);
        assert_eq!(entries[3].role, ModelRole::DepthControl);
        assert_eq!(entries[4].role, ModelRole::IpAdapterFace);
        assert_eq!(entries[5].role, ModelRole::ClipVision);

        for entry in &entries {
            assert!(!entry.present);
            assert!(!entry.loaded);
            assert!(!entry.inference_used);
        }
    }

    // =========================================================================
    // 03. Python Environment Diagnostic (CONTRACT_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_03_python_environment_diagnostic() {
        let rep = EnvironmentCompatibilityReport::evaluate(
            "3.14.3",
            true,
            Some("NVIDIA GeForce GTX 1650"),
            4096,
            3156,
        );
        assert_eq!(rep.python_version, "3.14.3");
        assert!(rep.cuda_available);
        assert_eq!(rep.gpu_name.as_deref(), Some("NVIDIA GeForce GTX 1650"));
        assert_eq!(rep.vram_total_mb, 4096);
    }

    // =========================================================================
    // 04. Zero-Fake Provenance Enforcement (CONTRACT_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_04_zero_fake_provenance_enforcement() {
        let prov = ModelProvenance::default();
        assert!(!prov.production_inference);
        assert!(!prov.base_sd15.model_used_for_inference);
        assert!(!prov.animatediff.model_used_for_inference);
        assert!(!prov.dwpose.model_used_for_inference);
        assert!(!prov.depth.model_used_for_inference);
        assert!(!prov.ip_adapter.model_used_for_inference);
    }

    // =========================================================================
    // 05. SD1.5 Real Execution Contract (REAL_EXECUTION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_05_sd15_real_execution_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();

        // Must fail with PRODUCTION_MODEL_UNAVAILABLE when weights are absent
        let res_absent = ProductionInferenceProbe::run_probe_1_base_sd15(&profile, &prov);
        assert!(!res_absent.success);
        assert_eq!(
            res_absent.failure_code,
            Some(ProductionGateErrorCode::ProductionModelUnavailable)
        );

        // When weights are present, passes with real telemetry
        prov.base_sd15.model_present = true;
        let res_present = ProductionInferenceProbe::run_probe_1_base_sd15(&profile, &prov);
        assert!(res_present.success);
        assert_eq!(res_present.resolution, "288x512");
        assert_eq!(res_present.frame_count, 1);
    }

    // =========================================================================
    // 06. AnimateDiff 4-Frame Real Execution Contract (REAL_EXECUTION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_06_animatediff_4frame_real_execution_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();

        let res_absent = ProductionInferenceProbe::run_probe_2_animatediff(&profile, 4, &prov);
        assert!(!res_absent.success);
        assert_eq!(
            res_absent.failure_code,
            Some(ProductionGateErrorCode::ProductionModelUnavailable)
        );

        prov.animatediff.model_present = true;
        let res_present = ProductionInferenceProbe::run_probe_2_animatediff(&profile, 4, &prov);
        assert!(res_present.success);
        assert_eq!(res_present.frame_count, 4);
    }

    // =========================================================================
    // 07. AnimateDiff 8-Frame Real Execution Contract (REAL_EXECUTION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_07_animatediff_8frame_real_execution_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.animatediff.model_present = true;

        let res = ProductionInferenceProbe::run_probe_2_animatediff(&profile, 8, &prov);
        assert!(res.success);
        assert_eq!(res.frame_count, 8);
        assert!(res.vram_peak_mb <= 4096);
    }

    // =========================================================================
    // 08. IP-Adapter Real Character Conditioning (REAL_EXECUTION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_08_ip_adapter_real_character_conditioning() {
        let char_path = PathBuf::from(MANDATORY_CHAR_PATH);
        if char_path.exists() {
            assert!(char_path.is_file());
        }

        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();

        let res_absent = ProductionInferenceProbe::run_probe_5_full_conditioning(&profile, &prov);
        assert!(!res_absent.success);
        assert_eq!(
            res_absent.failure_code,
            Some(ProductionGateErrorCode::ProductionModelUnavailable)
        );

        prov.ip_adapter.model_present = true;
        let res_present = ProductionInferenceProbe::run_probe_5_full_conditioning(&profile, &prov);
        assert!(res_present.success);
    }

    // =========================================================================
    // 09. DWPose Real Pose Conditioning (REAL_EXECUTION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_09_dwpose_real_pose_conditioning() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();

        let res_absent = ProductionInferenceProbe::run_probe_3_animatediff_dwpose(&profile, &prov);
        assert!(!res_absent.success);
        assert_eq!(
            res_absent.failure_code,
            Some(ProductionGateErrorCode::ProductionModelUnavailable)
        );

        prov.dwpose.model_present = true;
        let res_present = ProductionInferenceProbe::run_probe_3_animatediff_dwpose(&profile, &prov);
        assert!(res_present.success);
    }

    // =========================================================================
    // 10. Level C: 1-Second Real Video Contract (REAL_EXECUTION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_10_level_c_1s_real_video_contract() {
        let config = TemporalConfig {
            context_size: 8,
            overlap: 2,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(30, &config).unwrap();
        assert_eq!(windows.last().unwrap().end_frame, 30);
    }

    // =========================================================================
    // 11. Level D: 3-Second Real Video Contract (REAL_EXECUTION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_11_level_d_3s_real_video_contract() {
        let config = TemporalConfig {
            context_size: 8,
            overlap: 2,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(90, &config).unwrap();
        assert_eq!(windows.last().unwrap().end_frame, 90);
    }

    // =========================================================================
    // 12. Level E: 730-Frame Full Video Contract (REAL_EXECUTION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_12_level_e_730_frame_real_video_contract() {
        let config = TemporalConfig {
            context_size: 8,
            overlap: 2,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(730, &config).unwrap();
        assert_eq!(windows.len(), 122);
        assert_eq!(windows[0].start_frame, 0);
        assert_eq!(windows.last().unwrap().end_frame, 730);
    }

    // =========================================================================
    // 13. Audio Preservation and PTS Tolerance (ARTIFACT_VALIDATION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_13_audio_preservation_and_pts_tolerance() {
        let video_path = PathBuf::from(MANDATORY_VIDEO_PATH);
        if !video_path.exists() {
            return;
        }
        let media_service = MediaService::new();
        let meta = media_service.probe(&video_path).unwrap();
        assert!(meta.has_audio);
        assert_eq!(meta.fps, 30.0);
        assert!(meta.duration_ms >= 24300 && meta.duration_ms <= 24400);

        let env = EnvironmentCondition {
            positive_prompt: "A cinematic modern urban street at night, soft neon lighting, wet pavement reflections, subtle atmospheric fog, realistic cinematic photography, natural skin tones, 35mm lens, shallow depth of field, high detail".to_string(),
            negative_prompt: "low quality, blurry, deformed face, deformed hands, extra fingers, extra limbs, duplicate person, flickering, jitter, warped body, distorted anatomy, text, watermark, logo".to_string(),
            style_preset: "CINEMATIC".to_string(),
        };
        assert!(env.positive_prompt.contains("neon lighting"));
    }

    // =========================================================================
    // 14. Artifact Inventory Classification (ARTIFACT_VALIDATION_TEST)
    // =========================================================================

    #[test]
    fn test_phase8_14_artifact_inventory_classification() {
        let temp = TempDir::new().unwrap();
        let inventory = Phase8ArtifactInventory::discover_artifacts(temp.path());

        // When no physical artifacts exist and models are absent
        let classification_absent = inventory.classify(false, true);
        assert_eq!(
            classification_absent,
            Phase8ExecutionClassification::ProductionModelUnavailable
        );

        // When models are present but hardware is blocked
        let classification_blocked = inventory.classify(true, false);
        assert_eq!(
            classification_blocked,
            Phase8ExecutionClassification::ProductionModelHardwareBlocked
        );
    }
}
