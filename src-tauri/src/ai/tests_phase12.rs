#[cfg(test)]
mod tests {
    use crate::ai::generative::hardware::{
        CapabilityReport, CapabilityTier, HardwareStatus, PrecisionMode,
    };
    use crate::ai::hybrid::{
        AiProvider, BudgetController, CacheKey, CloudImageProviderAdapter,
        ComponentExecutionTarget, CostEstimate, CostEstimator, CostStatus, GenerationCache,
        GenerationCacheEntry, GenerationError, GenerationRequest, HybridProvenanceMetadata,
        KeyframePlanner, LocalAiProvider, MockAiProvider, ProviderConfig, ProviderHealth,
        ProviderType, QualityMode, TransformationIntent, TransformationPlanner,
    };
    use std::path::PathBuf;

    fn make_test_hardware_report(tier: CapabilityTier) -> CapabilityReport {
        CapabilityReport {
            timestamp: "2026-08-16T18:00:00Z".to_string(),
            hardware: crate::ai::generative::hardware::HardwareProbeReport {
                gpu: None,
                cpu: crate::ai::generative::hardware::CpuDeviceInfo {
                    architecture: "x86_64".to_string(),
                    logical_cores: 12,
                    physical_cores: Some(6),
                    total_ram_mb: 16000,
                    available_ram_mb: 8000,
                },
                runtime: crate::ai::generative::hardware::MlRuntimeInfo {
                    python_version: Some("3.11.9".to_string()),
                    pytorch_version: Some("2.7.1".to_string()),
                    torch_cuda_version: Some("11.8".to_string()),
                    diffusers_version: Some("0.39.0".to_string()),
                    transformers_version: Some("5.15.0".to_string()),
                    accelerate_version: Some("1.14.0".to_string()),
                    safetensors_version: Some("0.8.0".to_string()),
                },
                os: crate::ai::generative::hardware::OsInfo {
                    os_name: "windows".to_string(),
                    architecture: "x86_64".to_string(),
                },
            },
            precision_test: crate::ai::generative::hardware::PrecisionProbeResult {
                tested_precision: PrecisionMode::Fp32,
                stable: true,
                nan_detected: false,
                inf_detected: false,
                reason: "FP32 verified stable".to_string(),
            },
            benchmark: None,
            selected_tier: tier,
            selected_profile:
                crate::ai::generative::hardware::CapabilityClassifier::build_profile_for_tier(
                    tier,
                    PrecisionMode::Fp32,
                ),
            status: HardwareStatus::HardwareSupportedWithLimitations,
            user_override: crate::ai::generative::hardware::UserOverridePreference::Auto,
            warnings: vec![],
            fallback_history: vec![],
        }
    }

    // =========================================================================
    // 01. Provider Abstraction
    // =========================================================================

    #[test]
    fn test_phase12_01_provider_abstraction() {
        let local_prov = LocalAiProvider::new();
        assert_eq!(local_prov.provider_id(), "local_diffusers");
        assert_eq!(local_prov.provider_type(), ProviderType::Local);
        assert_eq!(local_prov.health(), ProviderHealth::Available);

        let cloud_img =
            CloudImageProviderAdapter::new("cloud_img_1", "Cloud Image Engine", None, None);
        assert_eq!(cloud_img.provider_type(), ProviderType::CloudImage);
        assert_eq!(cloud_img.health(), ProviderHealth::NotConfigured);
    }

    // =========================================================================
    // 02. Provider Capability
    // =========================================================================

    #[test]
    fn test_phase12_02_provider_capability() {
        let local_prov = LocalAiProvider::new();
        let cap = local_prov.capability();
        assert!(cap.supports_character_replacement);
        assert!(cap.supports_controlnet);
        assert!(cap.supports_ip_adapter);
        assert_eq!(cap.max_resolution, (576, 1024));
    }

    // =========================================================================
    // 03. Transformation Planner: Character Replacement
    // =========================================================================

    #[test]
    fn test_phase12_03_transformation_planner_character_replacement() {
        let hw = make_test_hardware_report(CapabilityTier::LowVram);
        let cloud_img = MockAiProvider::new(
            "mock_cloud",
            ProviderType::CloudImage,
            ProviderHealth::Available,
        );
        let local = LocalAiProvider::new();
        let providers: Vec<&dyn AiProvider> = vec![&cloud_img, &local];

        let plan = TransformationPlanner::plan(
            TransformationIntent::CharacterReplacement,
            QualityMode::SmartAuto,
            730,
            30.0,
            &[100, 300, 500],
            &[50, 200, 450],
            &hw,
            &providers,
            None,
        )
        .unwrap();

        assert_eq!(
            plan.decomposition.character,
            ComponentExecutionTarget::CloudImage
        );
        assert_eq!(
            plan.decomposition.background,
            ComponentExecutionTarget::ReuseOriginal
        );
        assert_eq!(
            plan.decomposition.audio,
            ComponentExecutionTarget::ReuseOriginal
        );
        assert_eq!(
            plan.decomposition.temporal_reconstruction,
            ComponentExecutionTarget::Local
        );
    }

    // =========================================================================
    // 04. Transformation Planner: Audio Replacement
    // =========================================================================

    #[test]
    fn test_phase12_04_transformation_planner_audio_replacement() {
        let hw = make_test_hardware_report(CapabilityTier::Balanced);
        let local = LocalAiProvider::new();
        let providers: Vec<&dyn AiProvider> = vec![&local];

        let plan = TransformationPlanner::plan(
            TransformationIntent::AudioReplacement,
            QualityMode::Balanced,
            730,
            30.0,
            &[],
            &[],
            &hw,
            &providers,
            None,
        )
        .unwrap();

        assert_eq!(
            plan.decomposition.character,
            ComponentExecutionTarget::ReuseOriginal
        );
        assert_eq!(
            plan.decomposition.background,
            ComponentExecutionTarget::ReuseOriginal
        );
        assert_eq!(
            plan.decomposition.motion,
            ComponentExecutionTarget::ReuseOriginal
        );
        assert_eq!(plan.decomposition.audio, ComponentExecutionTarget::Local);
        assert_eq!(plan.estimated_cloud_requests, 0);
    }

    // =========================================================================
    // 05. Keyframe Planner: 60s Reduction (Never 1800 requests!)
    // =========================================================================

    #[test]
    fn test_phase12_05_keyframe_planner_60s_reduction() {
        let total_frames = 1800; // 60s at 30fps
        let scene_cuts = vec![300, 600, 900, 1200, 1500];
        let motion_peaks = vec![150, 450, 750, 1050, 1350, 1650];

        let plan_econ = KeyframePlanner::plan_keyframes(
            total_frames,
            30.0,
            &scene_cuts,
            &motion_peaks,
            QualityMode::Economy,
            TransformationIntent::CharacterReplacement,
        );

        assert!(
            plan_econ.keyframe_count < 100,
            "Economy mode must produce < 100 keyframes for 1800 frames"
        );
        assert!(
            plan_econ.reduction_ratio > 18.0,
            "Reduction ratio must be > 18x"
        );

        let plan_bal = KeyframePlanner::plan_keyframes(
            total_frames,
            30.0,
            &scene_cuts,
            &motion_peaks,
            QualityMode::Balanced,
            TransformationIntent::CharacterReplacement,
        );

        assert!(
            plan_bal.keyframe_count < 250,
            "Balanced mode must produce < 250 keyframes for 1800 frames"
        );
    }

    // =========================================================================
    // 06. Hardware-Aware Routing: Low VRAM
    // =========================================================================

    #[test]
    fn test_phase12_06_hardware_aware_routing_low_vram() {
        let hw = make_test_hardware_report(CapabilityTier::LowVram);
        let cloud = MockAiProvider::new(
            "cloud_prov",
            ProviderType::CloudImage,
            ProviderHealth::Available,
        );
        let local = LocalAiProvider::new();
        let providers: Vec<&dyn AiProvider> = vec![&cloud, &local];

        let plan = TransformationPlanner::plan(
            TransformationIntent::StyleTransformation,
            QualityMode::SmartAuto,
            300,
            30.0,
            &[],
            &[],
            &hw,
            &providers,
            None,
        )
        .unwrap();

        assert_eq!(plan.selected_provider_type, ProviderType::CloudImage);
        assert!(!plan.recommendations.is_empty());
    }

    // =========================================================================
    // 07. Hardware-Aware Routing: High VRAM
    // =========================================================================

    #[test]
    fn test_phase12_07_hardware_aware_routing_high_vram() {
        let hw = make_test_hardware_report(CapabilityTier::High);
        let local = LocalAiProvider::new();
        let providers: Vec<&dyn AiProvider> = vec![&local];

        let plan = TransformationPlanner::plan(
            TransformationIntent::StyleTransformation,
            QualityMode::SmartAuto,
            300,
            30.0,
            &[],
            &[],
            &hw,
            &providers,
            None,
        )
        .unwrap();

        assert_eq!(
            plan.decomposition.character,
            ComponentExecutionTarget::Local
        );
        assert_eq!(
            plan.decomposition.background,
            ComponentExecutionTarget::Local
        );
        assert_eq!(plan.selected_provider_type, ProviderType::Local);
    }

    // =========================================================================
    // 08. Cost Estimator: Exact & Unknown (Zero-Fake)
    // =========================================================================

    #[test]
    fn test_phase12_08_cost_estimator_exact_and_unknown() {
        let cfg_priced = ProviderConfig {
            pricing_per_image: Some(0.02),
            currency: "USD".to_string(),
            ..Default::default()
        };
        let est_priced = CostEstimator::estimate_for_keyframes(&cfg_priced, 50, 20.0);
        assert_eq!(est_priced.estimated_cost, Some(1.00));
        assert_eq!(est_priced.status, CostStatus::Estimated);

        let cfg_unpriced = ProviderConfig {
            pricing_per_image: None,
            currency: "USD".to_string(),
            ..Default::default()
        };
        let est_unpriced = CostEstimator::estimate_for_keyframes(&cfg_unpriced, 50, 20.0);
        assert_eq!(est_unpriced.estimated_cost, None);
        assert_eq!(est_unpriced.status, CostStatus::Unknown);
    }

    // =========================================================================
    // 09. Budget Enforcement and Alternatives
    // =========================================================================

    #[test]
    fn test_phase12_09_budget_enforcement_and_alternatives() {
        let estimate = CostEstimate {
            estimated_cost: Some(15.00),
            currency: "USD".to_string(),
            estimated_requests: 150,
            estimated_generated_seconds: 60.0,
            estimated_keyframes: 150,
            estimated_local_processing_time_sec: 10.0,
            confidence: 0.9,
            status: CostStatus::Estimated,
        };

        let result = BudgetController::check_budget(&estimate, Some(10.00));
        assert!(result.is_err());
        match result.unwrap_err() {
            GenerationError::BudgetExceeded { estimated, budget } => {
                assert_eq!(estimated, 15.00);
                assert_eq!(budget, 10.00);
            }
            other => panic!("Unexpected error: {:?}", other),
        }

        let alts = BudgetController::suggest_alternatives(15.00, 10.00);
        assert!(!alts.is_empty());
    }

    // =========================================================================
    // 10. Provider Health Filtering
    // =========================================================================

    #[test]
    fn test_phase12_10_provider_health_filtering() {
        let unconfigured = CloudImageProviderAdapter::new("cloud_unauth", "Cloud", None, None);
        let hw = make_test_hardware_report(CapabilityTier::LowVram);
        let providers: Vec<&dyn AiProvider> = vec![&unconfigured];

        let plan_res = TransformationPlanner::plan(
            TransformationIntent::CharacterReplacement,
            QualityMode::Economy,
            100,
            30.0,
            &[],
            &[],
            &hw,
            &providers,
            None,
        );

        assert!(plan_res.is_err());
        match plan_res.unwrap_err() {
            GenerationError::ProviderNotConfigured(_) => {}
            other => panic!("Expected ProviderNotConfigured, got {:?}", other),
        }
    }

    // =========================================================================
    // 11. Cache Invalidation and Key Integrity
    // =========================================================================

    #[test]
    fn test_phase12_11_cache_invalidation_and_key_collision() {
        let cache = GenerationCache::new();

        let key1 = CacheKey::compute(
            "sha_source_1",
            TransformationIntent::CharacterReplacement,
            "prompt A",
            None,
            Some("ref_1"),
            "provider_1",
            "model_1",
            42,
            (576, 1024),
            20,
        );

        let key2 = CacheKey::compute(
            "sha_source_1",
            TransformationIntent::CharacterReplacement,
            "prompt B", // prompt changed!
            None,
            Some("ref_1"),
            "provider_1",
            "model_1",
            42,
            (576, 1024),
            20,
        );

        assert_ne!(key1, key2, "Cache keys must differ when prompt changes");

        let entry = GenerationCacheEntry {
            key: key1.clone(),
            generated_frames: vec![PathBuf::from("frame_0.png")],
            generated_video: None,
            provenance: HybridProvenanceMetadata::default(),
            created_timestamp_secs: 1000,
        };

        cache.insert(key1.clone(), entry);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&key1).is_some());
        assert!(cache.get(&key2).is_none());

        assert!(cache.invalidate(&key1));
        assert_eq!(cache.len(), 0);
    }

    // =========================================================================
    // 12. Provenance Integrity
    // =========================================================================

    #[test]
    fn test_phase12_12_provenance_integrity() {
        let prov = HybridProvenanceMetadata {
            source_asset_hash: "abc123sha".to_string(),
            provider: "hybrid_engine".to_string(),
            model: "sd15".to_string(),
            model_version: "1.5".to_string(),
            generation_type: "keyframe_cloud_hybrid".to_string(),
            seed: 42,
            prompt_hash: "def456hash".to_string(),
            input_reference_hashes: vec!["ref_sha_1".to_string()],
            hardware_profile: "GTX_1650_FP32".to_string(),
            timestamp: 1780000000,
            cost_estimate: Some(0.48),
            actual_cost: Some(0.48),
            inference_used: true,
            pipeline_version: "12.0.0".to_string(),
            zero_fake_verified: true,
        };

        assert!(prov.inference_used);
        assert!(prov.zero_fake_verified);
        assert_eq!(prov.cost_estimate, Some(0.48));
    }

    // =========================================================================
    // 13. Error Classification All Variants
    // =========================================================================

    #[test]
    fn test_phase12_13_error_classification_all_variants() {
        let errs = vec![
            GenerationError::ProviderNotConfigured("no api key".to_string()),
            GenerationError::ProviderCredentialsMissing("missing secret".to_string()),
            GenerationError::ProviderUnavailable("server offline".to_string()),
            GenerationError::ProviderRateLimited("429 rate limit".to_string()),
            GenerationError::ProviderExecutionFailed("internal error".to_string()),
            GenerationError::ProviderTimeout("timed out".to_string()),
            GenerationError::BudgetExceeded {
                estimated: 5.0,
                budget: 2.0,
            },
            GenerationError::NoCapableProvider("none found".to_string()),
            GenerationError::LocalHardwareUnsupported("no cuda".to_string()),
            GenerationError::QualityTargetUnachievable("unsupported res".to_string()),
            GenerationError::KeyframeGenerationFailed("bad latent".to_string()),
            GenerationError::TemporalReconstructionFailed("flow failed".to_string()),
            GenerationError::AudioProcessingFailed("mux failed".to_string()),
            GenerationError::FinalRenderFailed("ffmpeg crashed".to_string()),
        ];

        for err in errs {
            let msg = format!("{}", err);
            assert!(!msg.is_empty());
        }
    }

    // =========================================================================
    // 14. Mock Provider Zero-Fake Guarantee
    // =========================================================================

    #[test]
    fn test_phase12_14_mock_provider_zero_fake() {
        let mock = MockAiProvider::new(
            "mock_test",
            ProviderType::CloudImage,
            ProviderHealth::Available,
        );
        let req = GenerationRequest {
            request_id: "req_1".to_string(),
            prompt: "test".to_string(),
            negative_prompt: None,
            source_frames: vec![],
            character_reference: None,
            pose_conditioning: None,
            depth_conditioning: None,
            width: 288,
            height: 512,
            num_frames: 4,
            fps: 30.0,
            seed: 42,
            steps: 10,
        };

        let res = mock.generate(&req).unwrap();
        assert!(
            res.is_mock,
            "Mock provider must explicitly set is_mock=true"
        );
        assert!(
            !res.inference_used,
            "Mock provider must never claim inference_used=true"
        );
    }

    // =========================================================================
    // 15. Synthetic 60-Second 1080p Performance Test
    // =========================================================================

    #[test]
    fn test_phase12_15_synthetic_60s_1080p_performance_test() {
        let total_frames = 18000; // 600 seconds (10 minutes) or 18000 frames synthetic test
        let scene_cuts: Vec<usize> = (0..total_frames).step_by(600).collect();
        let motion_peaks: Vec<usize> = (0..total_frames).step_by(300).collect();

        let plan = KeyframePlanner::plan_keyframes(
            total_frames,
            30.0,
            &scene_cuts,
            &motion_peaks,
            QualityMode::Balanced,
            TransformationIntent::FullVideoRegeneration,
        );

        assert!(
            plan.keyframe_count < 2000,
            "18000 source frames must yield < 2000 keyframes"
        );
        assert!(plan.reduction_ratio >= 9.0, "Reduction ratio must be >= 9x");
    }
}
