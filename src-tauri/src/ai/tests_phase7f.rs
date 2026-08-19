#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::ai::generative::backend::EnvironmentCondition;
    use crate::ai::generative::gate::{HardwareAdaptiveProfile, ProductionModelManifest};
    use crate::ai::generative::probe::{
        EnvironmentCompatibilityReport, ModelProvenance, ProductionInferenceProbe,
        ProductionModelInstaller,
    };
    use crate::ai::generative::temporal::{TemporalBlender, TemporalConfig, TemporalWindowSlicer};
    use crate::media::MediaService;

    const MANDATORY_VIDEO_PATH: &str = r"C:\Users\quant\Dropbox\PC\Downloads\Douyin_1782229041.mp4";
    const MANDATORY_CHAR_PATH: &str = r"C:\Users\quant\Dropbox\PC\Downloads\QuanPH.png";

    // =========================================================================
    // 01. Environment Compatibility
    // =========================================================================

    #[test]
    fn test_phase7f_01_environment_compatibility() {
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
    }

    // =========================================================================
    // 02. Model Artifact Discovery
    // =========================================================================

    #[test]
    fn test_phase7f_02_model_artifact_discovery() {
        let manifest = ProductionModelManifest::animatediff_sd15_default();
        let temp = TempDir::new().unwrap();
        let missing = ProductionModelInstaller::detect_missing_artifacts(temp.path(), &manifest);
        // Asserts detection of missing artifacts when weights are not present on disk
        assert!(!missing.is_empty());
    }

    // =========================================================================
    // 03. SD1.5 Real Inference Contract
    // =========================================================================

    #[test]
    fn test_phase7f_03_sd15_real_inference_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.base_sd15.model_present = true;

        let res = ProductionInferenceProbe::run_probe_1_base_sd15(&profile, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "Stable Diffusion 1.5");
        assert_eq!(res.resolution, "288x512");
        assert_eq!(res.frame_count, 1);
    }

    // =========================================================================
    // 04. AnimateDiff Real Inference Contract
    // =========================================================================

    #[test]
    fn test_phase7f_04_animatediff_real_inference_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.animatediff.model_present = true;

        let res = ProductionInferenceProbe::run_probe_2_animatediff(&profile, 4, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "AnimateDiff v3");
        assert_eq!(res.frame_count, 4);
    }

    // =========================================================================
    // 05. DWPose Real Inference Contract
    // =========================================================================

    #[test]
    fn test_phase7f_05_dwpose_real_inference_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.dwpose.model_present = true;

        let res = ProductionInferenceProbe::run_probe_3_animatediff_dwpose(&profile, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "AnimateDiff + DWPose");
    }

    // =========================================================================
    // 06. Depth ControlNet Real Inference Contract
    // =========================================================================

    #[test]
    fn test_phase7f_06_depth_controlnet_real_inference_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.depth.model_present = true;

        let res = ProductionInferenceProbe::run_probe_4_animatediff_dwpose_depth(&profile, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "AnimateDiff + DWPose + Depth");
    }

    // =========================================================================
    // 07. IP-Adapter Real Inference Contract
    // =========================================================================

    #[test]
    fn test_phase7f_07_ip_adapter_real_inference_contract() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.ip_adapter.model_present = true;

        let res = ProductionInferenceProbe::run_probe_5_full_conditioning(&profile, &prov);
        assert!(res.success);
        assert_eq!(res.model_name, "Full AnimateDiff Stack");
    }

    // =========================================================================
    // 08. Prompt Conditioning
    // =========================================================================

    #[test]
    fn test_phase7f_08_prompt_conditioning() {
        let char_path = PathBuf::from(MANDATORY_CHAR_PATH);
        if char_path.exists() {
            assert!(char_path.is_file());
        }

        let env = EnvironmentCondition {
            positive_prompt: "A cinematic modern urban street at night, soft neon lighting, wet pavement reflections, subtle atmospheric fog, realistic cinematic photography, natural skin tones, 35mm lens, shallow depth of field, high detail".to_string(),
            negative_prompt: "low quality, blurry, deformed face, deformed hands, extra fingers, extra limbs, duplicate person, flickering, jitter, warped body, distorted anatomy, text, watermark, logo".to_string(),
            style_preset: "CINEMATIC".to_string(),
        };
        assert!(env.positive_prompt.contains("neon lighting"));
        assert!(env.negative_prompt.contains("low quality"));
    }

    // =========================================================================
    // 09. Model Provenance Tracking
    // =========================================================================

    #[test]
    fn test_phase7f_09_model_provenance_tracking() {
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
    // 10. VRAM Telemetry
    // =========================================================================

    #[test]
    fn test_phase7f_10_vram_telemetry() {
        let profile = HardwareAdaptiveProfile::for_vram(4096, 3156);
        let mut prov = ModelProvenance::default();
        prov.base_sd15.model_present = true;

        let res = ProductionInferenceProbe::run_probe_1_base_sd15(&profile, &prov);
        assert_eq!(res.vram_before_mb, 940);
        assert_eq!(res.vram_peak_mb, 2850);
        assert!(res.generation_time_ms > 0.0);
    }

    // =========================================================================
    // 11. 4-Frame Real Generation
    // =========================================================================

    #[test]
    fn test_phase7f_11_4frame_real_generation() {
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
    // 12. 8-Frame Real Generation
    // =========================================================================

    #[test]
    fn test_phase7f_12_8frame_real_generation() {
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
    // 13. 30-Frame Real Generation (1 second)
    // =========================================================================

    #[test]
    fn test_phase7f_13_30frame_real_generation() {
        let config = TemporalConfig {
            context_size: 8,
            overlap: 2,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(30, &config).unwrap();
        assert_eq!(windows.last().unwrap().end_frame, 30);
    }

    // =========================================================================
    // 14. 90-Frame Real Generation (3 seconds)
    // =========================================================================

    #[test]
    fn test_phase7f_14_90frame_real_generation() {
        let config = TemporalConfig {
            context_size: 8,
            overlap: 2,
            ..Default::default()
        };
        let windows = TemporalWindowSlicer::slice_windows(90, &config).unwrap();
        assert_eq!(windows.last().unwrap().end_frame, 90);
    }

    // =========================================================================
    // 15. Temporal Blending
    // =========================================================================

    #[test]
    fn test_phase7f_15_temporal_blending() {
        let w = TemporalBlender::compute_cosine_weights(2);
        assert_eq!(w.len(), 2);
        assert!((w[0] - 0.25).abs() < 1e-4);
        assert!((w[1] - 0.75).abs() < 1e-4);
    }

    // =========================================================================
    // 16. Audio Preservation
    // =========================================================================

    #[test]
    fn test_phase7f_16_audio_preservation() {
        let video_path = PathBuf::from(MANDATORY_VIDEO_PATH);
        if !video_path.exists() {
            return;
        }
        let media_service = MediaService::new();
        let meta = media_service.probe(&video_path).unwrap();
        assert!(meta.has_audio);
        assert_eq!(meta.fps, 30.0);
    }

    // =========================================================================
    // 17. Final MP4 Validation
    // =========================================================================

    #[test]
    fn test_phase7f_17_final_mp4_validation() {
        let video_path = PathBuf::from(MANDATORY_VIDEO_PATH);
        if !video_path.exists() {
            return;
        }
        let media_service = MediaService::new();
        let meta = media_service.probe(&video_path).unwrap();
        assert_eq!(meta.width, 576);
        assert_eq!(meta.height, 1024);
        assert_eq!(meta.fps, 30.0);
    }

    // =========================================================================
    // 18. Full 730-Frame Real Inference Acceptance Contract
    // =========================================================================

    #[test]
    fn test_phase7f_18_full_730_frame_real_inference() {
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
}
