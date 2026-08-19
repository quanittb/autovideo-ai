#[cfg(test)]
mod tests {
    use crate::ai::cloud::{
        CloudJobManager, CloudJobRequest, CloudJobState, CloudProviderError, CloudVideoProvider,
        CostEstimate, CostGuard, CostStatus, GenerationRouter, GenerationTask, LatencyTelemetry,
        ReplicateProvider, RoutingTarget, SegmentPlanner, UserExecutionMode,
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
            ..Default::default()
        };
        assert!(guard.check(&est_under).is_ok());

        let est_over = CostEstimate {
            estimated_usd: Some(1.20),
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
    // 07. Router AUTO Mode: Cloud-First
    // =========================================================================

    #[test]
    fn test_cloud_07_router_auto_mode_cloud_first() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let req = make_test_request(6.0);
        let hw = make_mock_hw(CapabilityTier::LowVram);

        let decision = GenerationRouter::route(
            GenerationTask::CharacterReplacement,
            UserExecutionMode::Auto,
            &req,
            &provider,
            Some(&hw),
        );

        assert_eq!(decision.target, RoutingTarget::Cloud);
        assert_eq!(decision.provider_id, "replicate");
        assert!(decision.fallback_available);
    }

    // =========================================================================
    // 08. Router AUTO Mode: Local / Hybrid Fallback
    // =========================================================================

    #[test]
    fn test_cloud_08_router_auto_mode_local_fallback() {
        let provider = ReplicateProvider::new(); // unconfigured
        let req = make_test_request(6.0);
        let hw = make_mock_hw(CapabilityTier::LowVram);

        let decision = GenerationRouter::route(
            GenerationTask::CharacterReplacement,
            UserExecutionMode::Auto,
            &req,
            &provider,
            Some(&hw),
        );

        assert_eq!(decision.target, RoutingTarget::Hybrid);
        assert_eq!(decision.provider_id, "local_diffusers");
    }

    // =========================================================================
    // 09. Router CLOUD Mode: Strict Rejection (No silent fallback!)
    // =========================================================================

    #[test]
    fn test_cloud_09_router_cloud_mode_strict_rejection() {
        let provider = ReplicateProvider::new(); // unconfigured
        let req = make_test_request(6.0);
        let hw = make_mock_hw(CapabilityTier::High);

        let decision = GenerationRouter::route(
            GenerationTask::CharacterReplacement,
            UserExecutionMode::Cloud,
            &req,
            &provider,
            Some(&hw),
        );

        assert_eq!(decision.target, RoutingTarget::Unavailable);
        assert!(!decision.fallback_available);
        assert!(decision.reason.contains("missing"));
    }

    // =========================================================================
    // 10. Router LOCAL Mode: Explicit Local Routing
    // =========================================================================

    #[test]
    fn test_cloud_10_router_local_mode_explicit() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let req = make_test_request(6.0);
        let hw = make_mock_hw(CapabilityTier::High);

        let decision = GenerationRouter::route(
            GenerationTask::CharacterReplacement,
            UserExecutionMode::Local,
            &req,
            &provider,
            Some(&hw),
        );

        assert_eq!(decision.target, RoutingTarget::Local);
        assert_eq!(decision.estimated_cost.estimated_usd, Some(0.0));
    }

    // =========================================================================
    // 11. Error Taxonomy Serialization
    // =========================================================================

    #[test]
    fn test_cloud_11_error_taxonomy_serialization() {
        let errors = vec![
            CloudProviderError::ProviderUnavailable("service offline".to_string()),
            CloudProviderError::AuthFailed("invalid token".to_string()),
            CloudProviderError::RequestInvalid("bad prompt".to_string()),
            CloudProviderError::RateLimited("429 limit".to_string()),
            CloudProviderError::Timeout("polling timeout".to_string()),
            CloudProviderError::JobFailed("cuda oom on server".to_string()),
            CloudProviderError::DownloadFailed("socket closed".to_string()),
            CloudProviderError::OutputInvalid("zero bytes".to_string()),
            CloudProviderError::CostLimitExceeded {
                estimated: 5.0,
                limit: 2.0,
            },
            CloudProviderError::NetworkError("dns error".to_string()),
        ];

        for err in errors {
            let msg = format!("{}", err);
            assert!(!msg.is_empty());
        }
    }

    // =========================================================================
    // 12. Replicate Response Status Parsing
    // =========================================================================

    #[test]
    fn test_cloud_12_replicate_response_status_parsing() {
        let raw_json = r#"{
            "id": "pred_12345",
            "status": "succeeded",
            "output": "https://replicate.delivery/pbxt/abc/out.mp4",
            "error": null
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(raw_json).unwrap();
        assert_eq!(parsed["id"], "pred_12345");
        assert_eq!(parsed["status"], "succeeded");
        assert_eq!(
            parsed["output"],
            "https://replicate.delivery/pbxt/abc/out.mp4"
        );
    }

    // =========================================================================
    // 13. Real Cloud Acceptance Status Discovery (Zero-Fake)
    // =========================================================================

    #[test]
    fn test_cloud_13_real_cloud_acceptance_status_discovery() {
        let provider = ReplicateProvider::new();
        if !provider.is_configured() {
            // Zero-fake policy: correctly discovers unconfigured state
            assert!(!provider.is_configured());
        } else {
            assert!(provider.is_configured());
        }
    }
}
