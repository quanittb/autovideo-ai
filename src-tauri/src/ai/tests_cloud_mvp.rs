#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use crate::ai::cloud::job::CloudJobManager;
    use crate::ai::cloud::{
        validate_and_prepare_cloud_submission, CloudJobRequest, CloudJobState, CloudJobStatus,
        CloudProviderError, CloudVideoProvider, CostBreakdown, CostConfidence, CostEstimate,
        CostGuard, CostStatus, LatencyTelemetry, ProviderRegistry, ReplicateProvider,
        SegmentPlanner,
    };
    use std::path::PathBuf;

    fn make_test_request(duration: f64, task: &str) -> CloudJobRequest {
        CloudJobRequest {
            job_id: "test_job_1".to_string(),
            project_id: Some("default_project".to_string()),
            prompt: "A cinematic transformation in dramatic lighting".to_string(),
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
            task_type: task.to_string(),
        }
    }

    // =========================================================================
    // 01. Provider Capabilities Truth Check
    // =========================================================================

    #[test]
    fn test_cloud_01_provider_capabilities() {
        let provider = ReplicateProvider::new();
        let caps = provider.capabilities();
        // Truthful capability declaration: Minimax adapter currently only implements text prompt serialization
        assert!(caps.supports_text_to_video);
        assert!(!caps.supports_image_to_video);
        assert!(!caps.supports_video_to_video);
        assert!(!caps.supports_reference_image);
        assert!(!caps.supports_character_reference);
        assert!(!caps.supports_audio);
        assert_eq!(caps.estimated_cost_per_second, None);
    }

    // =========================================================================
    // 02. Cost Estimation Uses ProviderRegistry (Single Source of Truth)
    // =========================================================================

    #[test]
    fn test_cloud_02_cost_estimation_deterministic() {
        let provider = ReplicateProvider::with_token("test_dummy_token");
        let req = make_test_request(6.0, "CharacterReplacement");
        let est = provider.estimate_cost(&req);

        // Minimax Video-01 official Replicate price is $0.50 per prediction output run
        assert_eq!(est.model, "minimax/video-01");
        assert_eq!(est.estimated_usd, Some(0.50));
        assert_eq!(est.currency, "USD");
        assert_eq!(est.status, CostStatus::Estimated);
        assert!(est.breakdown.contains("replicate"));
    }

    // =========================================================================
    // 03. Cost Guard Budget Limit Check
    // =========================================================================

    #[test]
    fn test_cloud_03_cost_guard_budget_limit() {
        let guard = CostGuard::new(0.40); // $0.40 max budget (below $0.50)

        let est_over = CostEstimate {
            estimated_usd: Some(0.50),
            status: CostConfidence::Estimated,
            ..Default::default()
        };
        let err = guard.check(&est_over);
        assert!(err.is_err());
        match err.unwrap_err() {
            CloudProviderError::CostLimitExceeded { estimated, limit } => {
                assert_eq!(estimated, 0.50);
                assert_eq!(limit, 0.40);
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
        let req = make_test_request(4.0, "CharacterReplacement");

        manager.register_job("job_101", &req, None);
        let s0 = manager.get_status("job_101").unwrap();
        assert_eq!(s0.state, CloudJobState::Created);

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
    // 07. Production Submission Guard: Test 1 — Local Deterministic Tasks Rejected
    // =========================================================================

    #[test]
    fn test_phase14_guard_test_1_local_tasks_rejected() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let registry = ProviderRegistry::new();

        let local_tasks = [
            "STYLE_FILTER",
            "BACKGROUND_COMPOSITE",
            "AUDIO_TRANSFORMATION",
        ];

        for task_name in local_tasks {
            let req = make_test_request(6.0, task_name);
            let result = validate_and_prepare_cloud_submission(&req, None, &provider, &registry);

            assert!(
                result.is_err(),
                "Task {} must be rejected by submission guard",
                task_name
            );
            let err_msg = format!("{}", result.unwrap_err());
            assert!(
                err_msg.contains("TASK_ROUTES_TO_LOCAL_EXECUTION"),
                "Expected TASK_ROUTES_TO_LOCAL_EXECUTION, got: {}",
                err_msg
            );
        }
    }

    // =========================================================================
    // 08. Production Submission Guard: Test 2 — Character Replacement Blocked
    // =========================================================================

    #[test]
    fn test_phase14_guard_test_2_character_replacement_blocked() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let registry = ProviderRegistry::new();
        let req = make_test_request(6.0, "CHARACTER_REPLACEMENT");

        let result = validate_and_prepare_cloud_submission(&req, None, &provider, &registry);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("ROUTING_UNAVAILABLE") && err_msg.contains("Phase 16"),
            "Expected ROUTING_UNAVAILABLE with Phase 16 reason, got: {}",
            err_msg
        );
    }

    // =========================================================================
    // 09. Production Submission Guard: Test 3 — Background Removal Blocked
    // =========================================================================

    #[test]
    fn test_phase14_guard_test_3_background_removal_blocked() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let registry = ProviderRegistry::new();
        let req = make_test_request(6.0, "BACKGROUND_REMOVAL");

        let result = validate_and_prepare_cloud_submission(&req, None, &provider, &registry);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("ROUTING_UNAVAILABLE") && err_msg.contains("Phase 17"),
            "Expected ROUTING_UNAVAILABLE with Phase 17 reason, got: {}",
            err_msg
        );
    }

    // =========================================================================
    // 10. Production Submission Guard: Test 4 — Default Budget ($3.00 pass, $3.01 fail)
    // =========================================================================

    #[test]
    fn test_phase14_guard_test_4_default_budget_enforcement() {
        let guard = CostGuard::standard_job_guard();
        assert_eq!(guard.max_cost_per_job, 3.00);

        let breakdown_exact = CostBreakdown {
            total_usd: Some(3.00),
            confidence: CostConfidence::Exact,
            ..Default::default()
        };
        assert!(guard.check_breakdown(&breakdown_exact).is_ok());

        let breakdown_over = CostBreakdown {
            total_usd: Some(3.01),
            confidence: CostConfidence::Estimated,
            ..Default::default()
        };
        let err = guard.check_breakdown(&breakdown_over);
        assert!(err.is_err());
        match err.unwrap_err() {
            CloudProviderError::CostLimitExceeded { estimated, limit } => {
                assert!((estimated - 3.01).abs() < 0.001);
                assert!((limit - 3.00).abs() < 0.001);
            }
            other => panic!("Expected CostLimitExceeded, got {:?}", other),
        }
    }

    // =========================================================================
    // 11. Production Submission Guard: Test 5 — Unknown Price Blocks Submission
    // =========================================================================

    #[test]
    fn test_phase14_guard_test_5_unknown_price_blocks_submission() {
        let guard = CostGuard::standard_job_guard();

        let breakdown_none = CostBreakdown {
            total_usd: None,
            confidence: CostConfidence::Unknown,
            ..Default::default()
        };
        assert!(guard.check_breakdown(&breakdown_none).is_err());

        let breakdown_unknown = CostBreakdown {
            total_usd: Some(0.0),
            confidence: CostConfidence::Unknown,
            ..Default::default()
        };
        assert!(guard.check_breakdown(&breakdown_unknown).is_err());
    }

    // =========================================================================
    // 12. Production Submission Guard: Test 6 — Invalid User Budget Values
    // =========================================================================

    #[test]
    fn test_phase14_guard_test_6_invalid_user_budgets() {
        assert!(CostGuard::validate_budget(f64::NAN).is_err());
        assert!(CostGuard::validate_budget(f64::INFINITY).is_err());
        assert!(CostGuard::validate_budget(f64::NEG_INFINITY).is_err());
        assert!(CostGuard::validate_budget(-0.01).is_err());
        assert_eq!(CostGuard::validate_budget(3.00).unwrap(), 3.00);
    }

    // =========================================================================
    // 13. Production Submission Guard: Test 7 — Nonexistent Adapter Check
    // =========================================================================

    #[test]
    fn test_phase14_guard_test_7_nonexistent_adapter_rejected() {
        let registry = ProviderRegistry::new();
        assert!(!registry.has_executable_adapter("nonexistent_cloud_model"));
        assert!(!registry.has_executable_adapter("replicate_utility"));
    }

    // =========================================================================
    // 14. Rust ↔ TypeScript Serialization Contract Tests (Blocker C)
    // =========================================================================

    #[test]
    fn test_phase14_ipc_contract_serialization_camel_case() {
        // 1. CloudJobRequest camelCase contract
        let req = make_test_request(6.0, "CharacterReplacement");
        let json_req = serde_json::to_value(&req).unwrap();
        assert!(json_req.get("jobId").is_some());
        assert!(json_req.get("negativePrompt").is_some());
        assert!(json_req.get("sourceVideo").is_some());
        assert!(json_req.get("referenceImage").is_some());
        assert!(json_req.get("durationSeconds").is_some());
        assert!(json_req.get("taskType").is_some());
        assert!(json_req.get("job_id").is_none());

        // 2. CloudJobStatus camelCase contract
        let status = CloudJobStatus {
            job_id: "job_99".to_string(),
            state: CloudJobState::Processing,
            progress_pct: 50.0,
            remote_id: Some("rem_1".to_string()),
            remote_status: Some("processing".to_string()),
            error_message: None,
            output_url: Some("https://replicate.delivery/out.mp4".to_string()),
            elapsed_seconds: 2.5,
            cost_estimate: None,
            actual_cost: Some(0.50),
        };
        let json_status = serde_json::to_value(&status).unwrap();
        assert!(json_status.get("jobId").is_some());
        assert!(json_status.get("progressPct").is_some());
        assert!(json_status.get("remoteId").is_some());
        assert!(json_status.get("remoteStatus").is_some());
        assert!(json_status.get("outputUrl").is_some());
        assert!(json_status.get("elapsedSeconds").is_some());
        assert!(json_status.get("actualCost").is_some());
        assert!(json_status.get("job_id").is_none());

        // 3. CostEstimate camelCase contract
        let est = CostEstimate {
            provider: "replicate".to_string(),
            model: "minimax/video-01".to_string(),
            estimated_usd: Some(0.50),
            min_usd: Some(0.45),
            max_usd: Some(0.60),
            confidence: 0.85,
            currency: "USD".to_string(),
            status: CostConfidence::Estimated,
            breakdown: "1 prediction @ $0.50".to_string(),
        };
        let json_est = serde_json::to_value(&est).unwrap();
        assert!(json_est.get("estimatedUsd").is_some());
        assert!(json_est.get("minUsd").is_some());
        assert!(json_est.get("maxUsd").is_some());
        assert!(json_est.get("estimated_usd").is_none());

        // 4. CostBreakdown camelCase contract
        let breakdown = CostBreakdown::default();
        let json_breakdown = serde_json::to_value(&breakdown).unwrap();
        assert!(json_breakdown.get("providerId").is_some());
        assert!(json_breakdown.get("modelId").is_some());
        assert!(json_breakdown.get("billableDurationSec").is_some());
        assert!(json_breakdown.get("segmentCount").is_some());
        assert!(json_breakdown.get("totalUsd").is_some());

        // 5. Frontend camelCase payload deserialization into Rust
        let frontend_payload = r#"{
            "jobId": "fe_job_123",
            "prompt": "Test prompt from frontend",
            "negativePrompt": "low quality",
            "sourceVideo": "C:\\videos\\test.mp4",
            "referenceImage": null,
            "durationSeconds": 6.0,
            "fps": 30.0,
            "resolution": [720, 1280],
            "taskType": "STYLE_FILTER"
        }"#;
        let parsed_req: CloudJobRequest = serde_json::from_str(frontend_payload).unwrap();
        assert_eq!(parsed_req.job_id, "fe_job_123");
        assert_eq!(parsed_req.duration_seconds, 6.0);
        assert_eq!(parsed_req.task_type, "STYLE_FILTER");
    }

    // =========================================================================
    // 15. Dynamic Price Refresh Updates All Estimates
    // =========================================================================

    #[test]
    fn test_phase14_dynamic_price_refresh_updates_estimates() {
        let mut registry = ProviderRegistry::new();
        assert_eq!(
            registry.find_by_id("replicate").unwrap().pricing_amount,
            Some(0.50)
        );

        // Price update to $0.45 without modifying routing code
        let updated = registry.update_price(
            "replicate",
            Some(0.45),
            "https://replicate.com/minimax/video-01",
            "2026-08-19",
        );
        assert!(updated);
        assert_eq!(
            registry.find_by_id("replicate").unwrap().pricing_amount,
            Some(0.45)
        );
    }

    // =========================================================================
    // 16. Complete Historical Project Fixture Deserialization
    // =========================================================================

    #[test]
    fn test_phase14_historical_project_fixture_deserialization() {
        let project_json = r#"{
            "schemaVersion": 1,
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "Sample Test Project",
            "createdAt": "2026-08-16T12:00:00Z",
            "updatedAt": "2026-08-16T12:05:00Z",
            "status": "READY",
            "sourceMedia": {
                "mediaId": "med_12345",
                "originalFileName": "sample_video.mp4",
                "sourcePath": "C:\\videos\\sample_video.mp4",
                "durationMs": 10000,
                "width": 1920,
                "height": 1080,
                "fps": 30.0,
                "fileSizeBytes": 15000000,
                "container": "mp4",
                "videoCodec": "h264",
                "audioCodec": "aac",
                "hasAudio": true
            },
            "transformationConfig": {
                "category": "character",
                "detectedCharacter": "Fox",
                "originalCharacter": "Fox",
                "replacementCharacter": "White Rabbit",
                "referenceImageUri": null,
                "prompt": "A cute white rabbit wearing a warm knitted scarf",
                "negativePrompt": "blurry, low quality",
                "preservation": {
                    "preserveMotion": true,
                    "preserveCamera": true,
                    "preserveComposition": true,
                    "preserveOriginalAudio": true
                },
                "seed": 42
            },
            "transformationPlan": null,
            "outputs": [],
            "editorState": null,
            "isFixture": false
        }"#;

        let project: crate::projects::Project = serde_json::from_str(project_json)
            .expect("Historical project fixture must deserialize cleanly");

        assert_eq!(project.name, "Sample Test Project");
        assert_eq!(project.status, crate::projects::ProjectStatus::Ready);
        let src = project.source_media.unwrap();
        assert_eq!(src.width, 1920);
        assert_eq!(src.height, 1080);
        assert_eq!(src.fps, 30.0);
        assert_eq!(project.transformation_config.category, "character");
        assert!(project.transformation_config.preservation.preserve_motion);
    }
}
