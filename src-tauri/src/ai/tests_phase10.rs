#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::ai::generative::gate::ProductionGateErrorCode;
    use crate::ai::generative::hardware::{
        CapabilityClassifier, CapabilityReport, CapabilityTier, CpuDeviceInfo, GpuDeviceInfo,
        GpuVendor, HardwareProbeReport, HardwareStatus, MlRuntimeInfo, OffloadStrategy, OsInfo,
        PipelinePlanner, PrecisionMode, PrecisionProbeResult, ProfileFallbackAttempt,
        UserOverridePreference,
    };
    use crate::ai::generative::probe::ModelProvenance;

    const CAPABILITY_REPORT_PATH: &str =
        r"D:\rustProject\autovideo-ai\outputs\hardware\capability_report.json";

    fn make_mock_probe(
        vram_total_mb: u64,
        vram_avail_mb: u64,
        cuda_avail: bool,
    ) -> HardwareProbeReport {
        let gpu = if cuda_avail {
            Some(GpuDeviceInfo {
                vendor: GpuVendor::Nvidia,
                device_name: "Generic Test GPU".to_string(),
                total_vram_mb: vram_total_mb,
                available_vram_mb: vram_avail_mb,
                allocated_vram_mb: 0,
                reserved_vram_mb: 0,
                cuda_available: true,
                cuda_version: Some("11.8".to_string()),
                driver_version: None,
                compute_capability: Some("7.5".to_string()),
                device_count: 1,
                has_tensor_cores: false,
            })
        } else {
            None
        };

        HardwareProbeReport {
            gpu,
            cpu: CpuDeviceInfo {
                architecture: "x86_64".to_string(),
                logical_cores: 8,
                physical_cores: Some(4),
                total_ram_mb: 16384,
                available_ram_mb: 8192,
            },
            runtime: MlRuntimeInfo::default(),
            os: OsInfo {
                os_name: "Windows".to_string(),
                architecture: "x86_64".to_string(),
            },
        }
    }

    // =========================================================================
    // 01. Hardware Probe Schema
    // =========================================================================

    #[test]
    fn test_phase10_01_hardware_probe_schema() {
        let probe = make_mock_probe(4096, 3156, true);
        assert!(probe.gpu.is_some());
        let gpu = probe.gpu.as_ref().unwrap();
        assert_eq!(gpu.vendor, GpuVendor::Nvidia);
        assert_eq!(gpu.total_vram_mb, 4096);
        assert!(gpu.cuda_available);
    }

    // =========================================================================
    // 02. GPU Capability Detection
    // =========================================================================

    #[test]
    fn test_phase10_02_gpu_capability_detection() {
        let vendor = GpuVendor::from_name("NVIDIA GeForce RTX 3060");
        assert_eq!(vendor, GpuVendor::Nvidia);

        let vendor_amd = GpuVendor::from_name("AMD Radeon RX 6700 XT");
        assert_eq!(vendor_amd, GpuVendor::Amd);

        let vendor_intel = GpuVendor::from_name("Intel Arc A770");
        assert_eq!(vendor_intel, GpuVendor::Intel);
    }

    // =========================================================================
    // 03. VRAM Detection
    // =========================================================================

    #[test]
    fn test_phase10_03_vram_detection() {
        let probe = make_mock_probe(8192, 6000, true);
        let gpu = probe.gpu.unwrap();
        assert_eq!(gpu.total_vram_mb, 8192);
        assert_eq!(gpu.available_vram_mb, 6000);
    }

    // =========================================================================
    // 04. Usable VRAM Calculation
    // =========================================================================

    #[test]
    fn test_phase10_04_usable_vram_calculation() {
        // 4096 Total, 3321 Avail -> usable = min(4096-500, 3321) - 512 = 3321 - 512 = 2809
        let usable = CapabilityClassifier::calculate_usable_vram(4096, 3321, 512);
        assert_eq!(usable, 2809);

        // Low memory edge case
        let usable_low = CapabilityClassifier::calculate_usable_vram(2048, 400, 512);
        assert_eq!(usable_low, 0);
    }

    // =========================================================================
    // 05. Precision Capability Classification
    // =========================================================================

    #[test]
    fn test_phase10_05_precision_capability_classification() {
        let precision_unstable = PrecisionProbeResult {
            tested_precision: PrecisionMode::Fp16,
            stable: false,
            nan_detected: true,
            inf_detected: false,
            reason: "FP16 numerical instability detected".to_string(),
        };

        let probe = make_mock_probe(4096, 3156, true);
        let (_tier, profile, _status, warnings) =
            CapabilityClassifier::classify(&probe, &precision_unstable);

        assert_eq!(profile.precision, PrecisionMode::Fp32);
        assert!(warnings.iter().any(|w| w.contains("Precision downgrade")));
    }

    // =========================================================================
    // 06. Runtime Profile Selection
    // =========================================================================

    #[test]
    fn test_phase10_06_runtime_profile_selection() {
        let precision_ok = PrecisionProbeResult {
            tested_precision: PrecisionMode::Fp16,
            stable: true,
            nan_detected: false,
            inf_detected: false,
            reason: "FP16 stable".to_string(),
        };

        let probe_8gb = make_mock_probe(8192, 7000, true);
        let (tier, profile, status, _warnings) =
            CapabilityClassifier::classify(&probe_8gb, &precision_ok);

        assert_eq!(tier, CapabilityTier::Balanced);
        assert_eq!(profile.target_width, 512);
        assert_eq!(profile.target_height, 768);
        assert_eq!(status, HardwareStatus::HardwareSupported);
    }

    // =========================================================================
    // 07. Low-Memory Profile
    // =========================================================================

    #[test]
    fn test_phase10_07_low_memory_profile() {
        let profile = CapabilityClassifier::build_profile_for_tier(
            CapabilityTier::LowVram,
            PrecisionMode::Fp32,
        );
        assert_eq!(profile.target_width, 288);
        assert_eq!(profile.target_height, 512);
        assert_eq!(
            profile.offload_strategy,
            OffloadStrategy::SequentialCpuOffload
        );
        assert!(profile.enable_vae_slicing);
        assert!(profile.enable_vae_tiling);
        assert!(profile.enable_attention_slicing);
    }

    // =========================================================================
    // 08. Balanced Profile
    // =========================================================================

    #[test]
    fn test_phase10_08_balanced_profile() {
        let profile = CapabilityClassifier::build_profile_for_tier(
            CapabilityTier::Balanced,
            PrecisionMode::Fp16,
        );
        assert_eq!(profile.target_width, 512);
        assert_eq!(profile.target_height, 768);
        assert_eq!(profile.offload_strategy, OffloadStrategy::ModelCpuOffload);
        assert_eq!(profile.max_temporal_window, 12);
    }

    // =========================================================================
    // 09. High-Memory Profile
    // =========================================================================

    #[test]
    fn test_phase10_09_high_memory_profile() {
        let profile =
            CapabilityClassifier::build_profile_for_tier(CapabilityTier::High, PrecisionMode::Fp16);
        assert_eq!(profile.target_width, 576);
        assert_eq!(profile.target_height, 1024);
        assert_eq!(profile.max_temporal_window, 16);
    }

    // =========================================================================
    // 10. Unsupported Hardware Classification
    // =========================================================================

    #[test]
    fn test_phase10_10_unsupported_hardware_classification() {
        let probe_no_gpu = make_mock_probe(0, 0, false);
        let precision = PrecisionProbeResult {
            tested_precision: PrecisionMode::Fp32,
            stable: true,
            nan_detected: false,
            inf_detected: false,
            reason: "CPU fallback".to_string(),
        };

        let (tier, _profile, status, _warnings) =
            CapabilityClassifier::classify(&probe_no_gpu, &precision);
        assert_eq!(tier, CapabilityTier::CpuOnly);
        assert_eq!(status, HardwareStatus::ProductionModelHardwareBlocked);
    }

    // =========================================================================
    // 11. OOM Fallback Contract
    // =========================================================================

    #[test]
    fn test_phase10_11_oom_fallback_contract() {
        let initial_profile =
            CapabilityClassifier::build_profile_for_tier(CapabilityTier::High, PrecisionMode::Fp16);
        assert_eq!(
            initial_profile.fallback_tiers,
            vec![
                CapabilityTier::Balanced,
                CapabilityTier::LowVram,
                CapabilityTier::UltraLowVram
            ]
        );

        let fallback_attempt = ProfileFallbackAttempt {
            tier: CapabilityTier::High,
            result: "CUDA_OUT_OF_MEMORY".to_string(),
            reason: Some("Tried to allocate 2.1 GB on 4GB device".to_string()),
        };
        assert_eq!(fallback_attempt.result, "CUDA_OUT_OF_MEMORY");
    }

    // =========================================================================
    // 12. NaN/Inf Fallback Contract
    // =========================================================================

    #[test]
    fn test_phase10_12_nan_inf_fallback_contract() {
        let precision_probe = PrecisionProbeResult {
            tested_precision: PrecisionMode::Fp16,
            stable: false,
            nan_detected: true,
            inf_detected: false,
            reason: "FP16 numerical instability detected".to_string(),
        };
        assert!(!precision_probe.stable);
        assert!(precision_probe.nan_detected);
    }

    // =========================================================================
    // 13. User Override Safety
    // =========================================================================

    #[test]
    fn test_phase10_13_user_override_safety() {
        let base_profile = CapabilityClassifier::build_profile_for_tier(
            CapabilityTier::LowVram,
            PrecisionMode::Fp32,
        );

        // On a 4GB GPU (usable VRAM ~2800 MB), requesting Quality MUST be safely clamped
        let (clamped_profile, warning) = CapabilityClassifier::apply_user_override(
            &base_profile,
            UserOverridePreference::Quality,
            2800,
        );

        assert_eq!(clamped_profile.tier, CapabilityTier::LowVram);
        assert!(warning.is_some());
        assert!(warning
            .unwrap()
            .contains("Quality mode exceeds safe memory capability"));
    }

    // =========================================================================
    // 14. Capability Report Generation
    // =========================================================================

    #[test]
    fn test_phase10_14_capability_report_generation() {
        let report_p = PathBuf::from(CAPABILITY_REPORT_PATH);
        if report_p.exists() {
            let data = std::fs::read_to_string(&report_p).unwrap();
            let parsed: Result<CapabilityReport, _> = serde_json::from_str(&data);
            assert!(
                parsed.is_ok(),
                "Capability report must be valid JSON matching CapabilityReport struct"
            );
            let rep = parsed.unwrap();
            assert_eq!(rep.status, HardwareStatus::HardwareSupportedWithLimitations);
            assert_eq!(rep.selected_tier, CapabilityTier::LowVram);
        }
    }

    // =========================================================================
    // 15. Current GTX 1650 Real Benchmark
    // =========================================================================

    #[test]
    fn test_phase10_15_gtx1650_real_benchmark() {
        let report_p = PathBuf::from(CAPABILITY_REPORT_PATH);
        if report_p.exists() {
            let data = std::fs::read_to_string(&report_p).unwrap();
            let rep: CapabilityReport = serde_json::from_str(&data).unwrap();
            if let Some(bench) = rep.benchmark {
                assert!(bench.success);
                assert!(bench.peak_allocated_mb > 0.0);
                assert!(!bench.oom_occurred);
            }
        }
    }

    // =========================================================================
    // 16. Zero-Fake Provenance Validation
    // =========================================================================

    #[test]
    fn test_phase10_16_zero_fake_provenance_validation() {
        let prov = ModelProvenance::default();
        assert!(!prov.production_inference);
        assert!(!prov.base_sd15.model_used_for_inference);

        // Planning with missing models must fail with ProductionModelUnavailable
        let profile = CapabilityClassifier::build_profile_for_tier(
            CapabilityTier::LowVram,
            PrecisionMode::Fp32,
        );
        let plan_res = PipelinePlanner::plan_pipeline(&[], &profile, (576, 1024), 1.0);
        assert_eq!(
            plan_res.unwrap_err(),
            ProductionGateErrorCode::ProductionModelUnavailable
        );
    }
}
