#[cfg(test)]
mod tests {
    use crate::ai::cloud::{
        CloudJobManager, CloudJobRequest, CloudJobState, CloudProviderError, CloudVideoProvider,
        CostBreakdown, CostConfidence, CostEstimate, CostGuard, CostStatus, ExecutionClass,
        GenerationRouter, LatencyTelemetry, ProviderRegistry, ReplicateProvider, RoutingPreference,
        RoutingTarget, SegmentPlanner, TaskClass, DEFAULT_STANDARD_JOB_BUDGET_USD,
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

    // =========================================================================
    // 01. Provider Capabilities Truth Check
    // =========================================================================

    #[test]
    fn test_cloud_01_provider_capabilities() {
        let provider = ReplicateProvider::new();
        let caps = provider.capabilities();
        // Truthful capability declaration: Minimax adapter currently only implements text-to-video prompt serialization
        assert!(caps.supports_text_to_video);
        assert!(!caps.supports_image_to_video);
        assert!(!caps.supports_video_to_video);
        assert!(!caps.supports_reference_image);
        assert!(!caps.supports_character_reference);
        assert!(!caps.supports_audio);
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
    // 07. Phase 14 Remediation Test A: Local Tasks Cannot Submit Cloud Job
    // =========================================================================

    #[test]
    fn test_phase14_remediation_test_a_local_tasks_cannot_submit_cloud_job() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let req = make_test_request(6.0);
        let registry = ProviderRegistry::new();

        let local_tasks = [
            TaskClass::StyleFilter,
            TaskClass::BackgroundComposite,
            TaskClass::AudioTransformation,
        ];

        for task in local_tasks {
            let decision = GenerationRouter::route_with_registry(
                task,
                RoutingPreference::CostSaving,
                &req,
                &provider,
                None,
                &registry,
            );

            assert_eq!(
                decision.target,
                RoutingTarget::Local,
                "Task {:?} must route to Local",
                task
            );
            assert_eq!(decision.execution_class, ExecutionClass::LocalDeterministic);
            assert_eq!(decision.cost_breakdown.total_usd, Some(0.0));

            // Production submission check: routing target Local must be rejected if cloud submit is attempted
            let is_cloud_submittable = decision.target == RoutingTarget::Cloud;
            assert!(
                !is_cloud_submittable,
                "Task {:?} must not be submittable to cloud",
                task
            );
        }
    }

    // =========================================================================
    // 08. Phase 14 Remediation Test B: Character Replacement Blocked Until Real Adapter
    // =========================================================================

    #[test]
    fn test_phase14_remediation_test_b_character_replacement_blocked_until_real_adapter() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let req = make_test_request(6.0);
        let registry = ProviderRegistry::new();

        let decision = GenerationRouter::route_with_registry(
            TaskClass::CharacterReplacement,
            RoutingPreference::CostSaving,
            &req,
            &provider,
            None,
            &registry,
        );

        // Desired execution class is SpecializedVideoTransformation, but target is Unavailable because
        // current adapter lacks video-to-video / character reference serialization (deferred to Phase 16)
        assert_eq!(
            decision.execution_class,
            ExecutionClass::SpecializedVideoTransformation
        );
        assert_eq!(decision.target, RoutingTarget::Unavailable);
        assert!(!decision.auto_submit_allowed);
        assert!(decision.reason.contains("Phase 16"));
    }

    // =========================================================================
    // 09. Phase 14 Remediation Test C: Background Removal Blocked Until Real Adapter
    // =========================================================================

    #[test]
    fn test_phase14_remediation_test_c_background_removal_blocked_until_real_adapter() {
        let provider = ReplicateProvider::with_token("test_valid_token");
        let req = make_test_request(6.0);
        let registry = ProviderRegistry::new();

        let decision = GenerationRouter::route_with_registry(
            TaskClass::BackgroundRemoval,
            RoutingPreference::CostSaving,
            &req,
            &provider,
            None,
            &registry,
        );

        // Desired execution class is UtilityCloud, but target is Unavailable because
        // no executable adapter exists in providers/ (deferred to Phase 17)
        assert_eq!(decision.execution_class, ExecutionClass::UtilityCloud);
        assert_eq!(decision.target, RoutingTarget::Unavailable);
        assert!(!decision.auto_submit_allowed);
        assert!(decision.reason.contains("Phase 17"));
    }

    // =========================================================================
    // 10. Phase 14 Remediation Test D: Production Default Budget Is Exactly USD 3.00
    // =========================================================================

    #[test]
    fn test_phase14_remediation_test_d_default_budget_is_3_usd() {
        assert_eq!(DEFAULT_STANDARD_JOB_BUDGET_USD, 3.00);

        let default_guard = CostGuard::standard_job_guard();
        assert_eq!(default_guard.max_cost_per_job, 3.00);

        let breakdown_exact = CostBreakdown {
            total_usd: Some(3.00),
            confidence: CostConfidence::Exact,
            ..Default::default()
        };
        assert!(default_guard.check_breakdown(&breakdown_exact).is_ok());

        let breakdown_over = CostBreakdown {
            total_usd: Some(3.01),
            confidence: CostConfidence::Estimated,
            ..Default::default()
        };
        let err = default_guard.check_breakdown(&breakdown_over);
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
    // 11. Phase 14 Remediation Test E: Unknown Price Blocks Submission
    // =========================================================================

    #[test]
    fn test_phase14_remediation_test_e_unknown_price_blocks_submission() {
        let guard = CostGuard::standard_job_guard();

        // 1. None total cost
        let breakdown_none = CostBreakdown {
            total_usd: None,
            confidence: CostConfidence::Unknown,
            ..Default::default()
        };
        assert!(guard.check_breakdown(&breakdown_none).is_err());

        // 2. Unknown confidence
        let breakdown_unknown_conf = CostBreakdown {
            total_usd: Some(0.0),
            confidence: CostConfidence::Unknown,
            ..Default::default()
        };
        assert!(guard.check_breakdown(&breakdown_unknown_conf).is_err());
    }

    // =========================================================================
    // 12. Phase 14 Remediation Test F: Invalid Budget Values Rejected
    // =========================================================================

    #[test]
    fn test_phase14_remediation_test_f_invalid_budget_values_rejected() {
        assert!(CostGuard::validate_budget(f64::NAN).is_err());
        assert!(CostGuard::validate_budget(f64::INFINITY).is_err());
        assert!(CostGuard::validate_budget(f64::NEG_INFINITY).is_err());
        assert!(CostGuard::validate_budget(-1.0).is_err());
        assert_eq!(CostGuard::validate_budget(2.50).unwrap(), 2.50);
        assert_eq!(CostGuard::validate_budget(0.0).unwrap(), 0.0);
    }

    // =========================================================================
    // 13. Phase 14 Remediation Test G: Replicate Adapter Truthful Capability Contract
    // =========================================================================

    #[test]
    fn test_phase14_remediation_test_g_replicate_adapter_truthful_capabilities() {
        let provider = ReplicateProvider::new();
        let caps = provider.capabilities();

        // Verify only serialized capabilities are claimed
        assert!(caps.supports_text_to_video);
        assert!(!caps.supports_video_to_video);
        assert!(!caps.supports_image_to_video);
        assert!(!caps.supports_reference_image);
        assert!(!caps.supports_character_reference);
        assert!(!caps.supports_audio);
    }

    // =========================================================================
    // 14. Phase 14 Remediation Test H: Provider Registry Adapter Verification
    // =========================================================================

    #[test]
    fn test_phase14_remediation_test_h_provider_registry_adapter_verification() {
        let registry = ProviderRegistry::new();

        assert!(registry.has_executable_adapter("local_ffmpeg"));
        assert!(registry.has_executable_adapter("replicate"));
        assert!(registry.has_executable_adapter("local_diffusers"));

        // Unimplemented adapter must return false
        assert!(!registry.has_executable_adapter("replicate_utility"));
        assert!(!registry.has_executable_adapter("nonexistent_provider"));
    }

    // =========================================================================
    // 15. Phase 14 Remediation: Complete Historical Project Fixture Deserialization
    // =========================================================================

    #[test]
    fn test_phase14_remediation_historical_project_fixture_deserialization() {
        // Complete project JSON matching actual Project structure
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

    // =========================================================================
    // 16. Phase 14 Remediation: TaskClass & Mode String Aliases
    // =========================================================================

    #[test]
    fn test_phase14_remediation_task_class_string_aliases() {
        assert_eq!(
            TaskClass::from_str_or_default("CharacterReplacement"),
            TaskClass::CharacterReplacement
        );
        assert_eq!(
            TaskClass::from_str_or_default("style_filter"),
            TaskClass::StyleFilter
        );
        assert_eq!(
            TaskClass::from_str_or_default("REMOVE_BG"),
            TaskClass::BackgroundRemoval
        );
        assert_eq!(
            TaskClass::from_str_or_default("audio_mux"),
            TaskClass::AudioTransformation
        );
    }

    // =========================================================================
    // 17. Phase 14 Remediation: Dynamic Price Refresh
    // =========================================================================

    #[test]
    fn test_phase14_remediation_dynamic_price_refresh() {
        let mut registry = ProviderRegistry::new();
        assert_eq!(
            registry.find_by_id("replicate").unwrap().pricing_amount,
            Some(0.04)
        );

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
    }
}
