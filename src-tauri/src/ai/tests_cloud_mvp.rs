#[cfg(test)]
mod tests {
    use crate::ai::cloud::{
        CloudJobManager, CloudJobRequest, CloudJobState, CloudProviderError, CloudVideoProvider,
        CostBreakdown, CostConfidence, CostEstimate, CostGuard, CostStatus, ExecutionClass,
        GenerationRouter, LatencyTelemetry, ProviderRegistry, ReplicateProvider, RoutingPreference,
        RoutingTarget, SegmentPlanner, TaskClass, DEFAULT_STANDARD_JOB_BUDGET_USD,
    };
    use crate::ai::generative::hardware::{
        CapabilityReport, CapabilityTier, HardwareStatus, PrecisionMode,
    };
    use std::path::PathBuf;

    fn make_test_request(duration: f64) -> CloudJobRequest {
        CloudJobRequest {
            job_id: "test_job_1".to_string(),
            prompt: "A cinematic transformation of character in dramatic lighting".to_string(),
            negative_prompt: Some("blurry, low quality".to_string()),
            source_video: Some(PathBuf::from(
                r"C:\Users\quant\Dropbox\PC\Downloads\Douyin_1782229041.mp4",
            )),
            reference_image: Some(PathBuf::from(
                r"C:\Users\quant\Dropbox\PC\Downloads\QuanPH.png",
            )),
            duration_seconds: duration,
            fps: 30.0,
            resolution: (576, 1024),
            task_type: "CharacterReplacement".to_string(),
        }
    }

    fn make_mock_hw(tier: CapabilityTier) -> CapabilityReport {
        CapabilityReport {
            timestamp: "2026-08-16T20:00:00Z".to_string(),
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
                reason: "FP32 stable".to_string(),
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
    // 01. Provider Capabilities
    // =========================================================================

    #[test]
    fn test_cloud_01_provider_capabilities() {
        let provider = ReplicateProvider::new();
        let caps = provider.capabilities();
        assert!(caps.supports_text_to_video);
        assert!(caps.supports_image_to_video);
        assert!(caps.supports_video_to_video);
        assert_eq!(caps.estimated_cost_per_second, Some(0.04));
    }

    // =========================================================================
    // 02. Cost Estimation Deterministic
    // =========================================================================

    #[test]
    fn test_cloud_02_cost_estimation_deterministic() {
        let provider = ReplicateProvider::with_token("test_dummy_token");
        let req = make_test_request(6.0);
        let est = provider.estimate_cost(&req);

        assert_eq!(est.estimated_usd, Some(0.24));
        assert_eq!(est.currency, "USD");
        assert_eq!(est.status, CostStatus::Estimated);
        assert!(est.breakdown.contains("$0.0400/sec x 6.0s"));
    }

    // =========================================================================
    // 03. Cost Guard Budget Limit
    // =========================================================================

    #[test]
    fn test_cloud_03_cost_guard_budget_limit() {
        let guard = CostGuard::new(0.50); // $0.50 max budget

        let est_under = CostEstimate {
            estimated_usd: Some(0.24),
            status: CostConfidence::Estimated,
            ..Default::default()
        };
        assert!(guard.check(&est_under).is_ok());

        let est_over = CostEstimate {
            estimated_usd: Some(1.20),
            status: CostConfidence::Estimated,
            ..Default::default()
        };
        let err = guard.check(&est_over);
        assert!(err.is_err());
        match err.unwrap_err() {
            CloudProviderError::CostLimitExceeded { estimated, limit } => {
                assert_eq!(estimated, 1.20);
                assert_eq!(limit, 0.50);
            }
            other => panic!("Expected CostLimitExceeded, got {:?}", other),
        }
    }

    // =========================================================================
    // 04. Latency Telemetry Tracking
    // =========================================================================

    #[test]
    fn test_cloud_04_latency_telemetry_tracking() {
        let mut telemetry = LatencyTelemetry::start();
        assert!(telemetry.t0_request_started_ms > 0);

        telemetry.mark_submitted();
        assert!(telemetry.t1_job_submitted_ms.is_some());
        assert!(telemetry.submit_latency_sec.is_some());

        telemetry.mark_processing();
        assert!(telemetry.t2_provider_processing_ms.is_some());

        telemetry.mark_completed();
        assert!(telemetry.t3_provider_completed_ms.is_some());
        assert!(telemetry.generation_latency_sec.is_some());

        telemetry.mark_downloaded();
        assert!(telemetry.t4_download_completed_ms.is_some());
        assert!(telemetry.download_latency_sec.is_some());

        telemetry.mark_validated();
        assert!(telemetry.t5_validation_completed_ms.is_some());
        assert!(telemetry.total_latency_sec >= 0.0);
    }

    // =========================================================================
    // 05. Job State Machine Transitions
    // =========================================================================

    #[test]
    fn test_cloud_05_job_state_machine_transitions() {
        let manager = CloudJobManager::new();
        let req = make_test_request(4.0);

        manager.register_job("job_101", &req, None);
        let s0 = manager.get_status("job_101").unwrap();
        assert_eq!(s0.state, CloudJobState::Queued);

        manager.update_state("job_101", CloudJobState::Processing, 25.0);
        manager.set_remote_info("job_101", "rem_xyz", "processing", None);
        let s1 = manager.get_status("job_101").unwrap();
        assert_eq!(s1.state, CloudJobState::Processing);
        assert_eq!(s1.remote_id, Some("rem_xyz".to_string()));

        manager.set_remote_info(
            "job_101",
            "rem_xyz",
            "succeeded",
            Some("https://replicate.delivery/out.mp4"),
        );
        manager.update_state("job_101", CloudJobState::Completed, 100.0);
        let s2 = manager.get_status("job_101").unwrap();
        assert_eq!(s2.state, CloudJobState::Completed);
        assert_eq!(
            s2.output_url,
            Some("https://replicate.delivery/out.mp4".to_string())
        );

        manager.mark_cancelled("job_101");
        let s3 = manager.get_status("job_101").unwrap();
        assert_eq!(s3.state, CloudJobState::Cancelled);
    }

    // =========================================================================
    // 06. Video Segment Planner
    // =========================================================================

    #[test]
    fn test_cloud_06_video_segment_planner() {
        let src = PathBuf::from("input.mp4");
        let segments =
            SegmentPlanner::plan_segments(&src, 24.333, 6.0, "Transform character", None);

        assert_eq!(segments.len(), 5);
        assert_eq!(segments[0].start_sec, 0.0);
        assert_eq!(segments[0].end_sec, 6.0);
        assert_eq!(segments[4].start_sec, 24.0);
        assert!((segments[4].end_sec - 24.333).abs() < 0.01);
    }

    // =========================================================================
    // 07. Phase 14: Task Routing to Expected Execution Classes
    // =========================================================================

    #[test]
    fn test_phase14_01_task_execution_classes() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let req = make_test_request(6.0);
        let hw = make_mock_hw(CapabilityTier::LowVram);

        // 1. Style Filter -> LocalDeterministic
        let d_style = GenerationRouter::route(
            TaskClass::StyleFilter,
            RoutingPreference::CostSaving,
            &req,
            &provider,
            Some(&hw),
        );
        assert_eq!(d_style.execution_class, ExecutionClass::LocalDeterministic);
        assert_eq!(d_style.target, RoutingTarget::Local);
        assert_eq!(d_style.cost_breakdown.total_usd, Some(0.0));

        // 2. Background Removal -> UtilityCloud
        let d_bg_rem = GenerationRouter::route(
            TaskClass::BackgroundRemoval,
            RoutingPreference::CostSaving,
            &req,
            &provider,
            Some(&hw),
        );
        assert_eq!(d_bg_rem.execution_class, ExecutionClass::UtilityCloud);
        assert_eq!(d_bg_rem.target, RoutingTarget::Cloud);

        // 3. Character Replacement -> SpecializedVideoTransformation
        let d_char = GenerationRouter::route(
            TaskClass::CharacterReplacement,
            RoutingPreference::CostSaving,
            &req,
            &provider,
            Some(&hw),
        );
        assert_eq!(
            d_char.execution_class,
            ExecutionClass::SpecializedVideoTransformation
        );
        assert_eq!(d_char.target, RoutingTarget::Cloud);
    }

    // =========================================================================
    // 08. Phase 14: Local Tasks Never Route to Paid Providers in Cost-Saving
    // =========================================================================

    #[test]
    fn test_phase14_02_local_tasks_never_route_to_paid_providers_in_cost_saving() {
        let provider = ReplicateProvider::with_token("test_valid_token"); // Paid token is configured!
        let req = make_test_request(6.0);
        let hw = make_mock_hw(CapabilityTier::High);

        let tasks = [
            TaskClass::StyleFilter,
            TaskClass::BackgroundComposite,
            TaskClass::AudioTransformation,
        ];

        for task in tasks {
            let decision = GenerationRouter::route(
                task,
                RoutingPreference::CostSaving,
                &req,
                &provider,
                Some(&hw),
            );
            assert_eq!(
                decision.execution_class,
                ExecutionClass::LocalDeterministic,
                "Task {:?} should route to LocalDeterministic",
                task
            );
            assert_eq!(decision.target, RoutingTarget::Local);
            assert_eq!(decision.cost_breakdown.total_usd, Some(0.0));
        }
    }

    // =========================================================================
    // 09. Phase 14: Capability & Resolution Mismatch Rejected
    // =========================================================================

    #[test]
    fn test_phase14_03_capability_resolution_mismatch_rejected() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let mut req = make_test_request(6.0);
        req.resolution = (3840, 2160); // 4K resolution unsupported by Minimax Video-01
        let hw = make_mock_hw(CapabilityTier::High);

        let decision = GenerationRouter::route(
            TaskClass::CharacterReplacement,
            RoutingPreference::CostSaving,
            &req,
            &provider,
            Some(&hw),
        );

        assert_eq!(decision.target, RoutingTarget::Unavailable);
        assert!(!decision.auto_submit_allowed);
        assert!(decision.reason.contains("not supported"));
    }

    // =========================================================================
    // 10. Phase 14: Unsupported FPS Rejected
    // =========================================================================

    #[test]
    fn test_phase14_04_unsupported_fps_rejected() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let mut req = make_test_request(6.0);
        req.fps = 120.0; // 120 FPS unsupported by video provider (supports max 30)
        let hw = make_mock_hw(CapabilityTier::High);

        let decision = GenerationRouter::route(
            TaskClass::CharacterReplacement,
            RoutingPreference::CostSaving,
            &req,
            &provider,
            Some(&hw),
        );

        assert_eq!(decision.target, RoutingTarget::Unavailable);
        assert!(!decision.auto_submit_allowed);
        assert!(decision.reason.contains("Requested frame rate"));
    }

    // =========================================================================
    // 11. Phase 14: Exact Budget Boundary Passes ($3.00 on $3.00)
    // =========================================================================

    #[test]
    fn test_phase14_05_exact_budget_boundary_passes() {
        let guard = CostGuard::new(3.00); // Standard $3.00 budget
        let breakdown = CostBreakdown {
            total_usd: Some(3.00),
            confidence: CostConfidence::Exact,
            ..Default::default()
        };

        assert!(guard.check_breakdown(&breakdown).is_ok());
    }

    // =========================================================================
    // 12. Phase 14: One Cent Over Budget Fails ($3.01 on $3.00)
    // =========================================================================

    #[test]
    fn test_phase14_06_one_cent_over_budget_fails() {
        let guard = CostGuard::new(3.00); // Standard $3.00 budget
        let breakdown = CostBreakdown {
            total_usd: Some(3.01),
            confidence: CostConfidence::Estimated,
            ..Default::default()
        };

        let res = guard.check_breakdown(&breakdown);
        assert!(res.is_err());
        match res.unwrap_err() {
            CloudProviderError::CostLimitExceeded { estimated, limit } => {
                assert!((estimated - 3.01).abs() < 0.001);
                assert!((limit - 3.00).abs() < 0.001);
            }
            other => panic!("Expected CostLimitExceeded, got {:?}", other),
        }
    }

    // =========================================================================
    // 13. Phase 14: Unknown Price Blocks Submission
    // =========================================================================

    #[test]
    fn test_phase14_07_unknown_price_blocks_submission() {
        let guard = CostGuard::new(DEFAULT_STANDARD_JOB_BUDGET_USD);
        let breakdown = CostBreakdown {
            total_usd: None,
            confidence: CostConfidence::Unknown,
            ..Default::default()
        };

        let res = guard.check_breakdown(&breakdown);
        assert!(res.is_err());
        match res.unwrap_err() {
            CloudProviderError::RequestInvalid(msg) => {
                assert!(msg.contains("Unknown cost"));
            }
            other => panic!("Expected RequestInvalid for unknown cost, got {:?}", other),
        }
    }

    // =========================================================================
    // 14. Phase 14: Disabled Full-Generative Blocks Submission in Cost-Saving
    // =========================================================================

    #[test]
    fn test_phase14_08_disabled_full_generative_blocks_submission_in_cost_saving() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let req = make_test_request(6.0);
        let hw = make_mock_hw(CapabilityTier::High);

        let decision = GenerationRouter::route(
            TaskClass::FullGenerativeTransformation,
            RoutingPreference::CostSaving,
            &req,
            &provider,
            Some(&hw),
        );

        assert_eq!(decision.target, RoutingTarget::Unavailable);
        assert!(!decision.auto_submit_allowed);
        assert!(decision.reason.contains("COST_SAVING"));
    }

    // =========================================================================
    // 15. Phase 14: Serialized Project Data Backward Compatibility
    // =========================================================================

    #[test]
    fn test_phase14_09_serialized_project_data_backward_compatibility() {
        // Deserializing legacy string values
        let legacy_json = r#"{"task": "CharacterReplacement", "mode": "Auto"}"#;
        #[derive(serde::Deserialize)]
        struct LegacyPayload {
            task: TaskClass,
            mode: RoutingPreference,
        }

        let parsed: LegacyPayload = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(parsed.task, TaskClass::CharacterReplacement);
        assert_eq!(parsed.mode, RoutingPreference::CostSaving);

        let screaming_json = r#"{"task": "BACKGROUND_COMPOSITE", "mode": "LOCAL_ONLY"}"#;
        let parsed_screaming: LegacyPayload = serde_json::from_str(screaming_json).unwrap();
        assert_eq!(parsed_screaming.task, TaskClass::BackgroundComposite);
        assert_eq!(parsed_screaming.mode, RoutingPreference::LocalOnly);
    }

    // =========================================================================
    // 16. Phase 14: Provider Registry Dynamic Price Refresh
    // =========================================================================

    #[test]
    fn test_phase14_10_provider_registry_price_refresh() {
        let mut registry = ProviderRegistry::new();
        assert_eq!(
            registry.find_by_id("replicate").unwrap().pricing_amount,
            Some(0.04)
        );

        // Dynamic price update without code changes
        let updated = registry.update_price(
            "replicate",
            Some(0.035),
            "https://replicate.com/minimax/video-01/pricing",
            "2026-08-19",
        );
        assert!(updated);
        assert_eq!(
            registry.find_by_id("replicate").unwrap().pricing_amount,
            Some(0.035)
        );
        assert_eq!(
            registry.find_by_id("replicate").unwrap().observed_at,
            "2026-08-19"
        );
    }
}
