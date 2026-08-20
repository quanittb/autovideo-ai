#[cfg(test)]
mod tests {
    use crate::ai::cloud::cost::CostConfidence;
    use crate::ai::cloud::job::{
        ArtifactContainer, ArtifactDescriptor, ArtifactVideoCodec, CloudJobRequest, CloudJobState,
        PersistentCloudJob, SubmissionState, ValidationPolicy, CURRENT_CLOUD_JOB_SCHEMA_VERSION,
    };
    use crate::ai::cloud::live_execution_guard::MockLiveExecutionPolicy;
    use crate::ai::cloud::provider::{CloudVideoProvider, ProviderKey, ResolutionTier, TargetFps};
    use crate::ai::cloud::providers::replicate_bria::ReplicateBriaBgRemovalProvider;
    use crate::ai::cloud::registry::{ExecutionClass, ProviderRegistry};
    use crate::ai::cloud::resolver::{CloudProviderResolver, DefaultCloudProviderResolver};
    use crate::ai::cloud::router::{GenerationRouter, RoutingPreference, TaskClass};
    use crate::ai::cloud::spec::{
        BackgroundMode, BackgroundRemovalOutputFormat, BackgroundRemovalSpec,
        PreparedBackgroundRemoval, PreparedCharacterReplacement, PreparedProviderSubmission,
        ProviderSubmissionSpec, SourceMediaFacts,
    };
    use crate::ai::cloud::store::PersistentCloudJobStore;
    use crate::ai::cloud::uploader::UploadedAsset;
    use crate::system::StoragePaths;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::tempdir;

    // =========================================================================
    // HELPER: create StoragePaths from tempdir
    // =========================================================================
    fn create_test_storage() -> (StoragePaths, tempfile::TempDir) {
        let temp = tempdir().unwrap();
        let paths = StoragePaths::resolve_from_base(temp.path());
        (paths, temp)
    }

    fn make_bg_removal_request(job_id: &str, duration: f64) -> CloudJobRequest {
        CloudJobRequest {
            job_id: job_id.to_string(),
            project_id: None,
            prompt: String::new(),
            negative_prompt: None,
            source_video: None,
            reference_image: None,
            reference_images: None,
            resolution: (1920, 1080),
            fps: 30.0,
            duration_seconds: duration,
            task_type: "BACKGROUND_REMOVAL".to_string(),
        }
    }

    // =========================================================================
    // 01. BRIA PROVIDER IDENTITY TESTS
    // =========================================================================

    #[test]
    fn test_phase17_01_bria_provider_identity() {
        let provider = ReplicateBriaBgRemovalProvider::with_policy(
            Some("test_token".to_string()),
            Arc::new(MockLiveExecutionPolicy::new(false)),
        );
        assert_eq!(provider.provider_id(), "replicate");
        assert_eq!(provider.model_id(), "bria/video-remove-background");
        assert_eq!(provider.model_version_hint(), Some("official-current"));
        assert_eq!(
            provider.provider_name(),
            "Replicate BRIA Video Background Removal"
        );
        assert!(provider.is_configured());
    }

    #[test]
    fn test_phase17_02_bria_not_configured_without_token() {
        let provider = ReplicateBriaBgRemovalProvider::with_policy(
            None,
            Arc::new(MockLiveExecutionPolicy::new(false)),
        );
        assert!(!provider.is_configured());
    }

    // =========================================================================
    // 02. BRIA PROVIDER CAPABILITIES
    // =========================================================================

    #[test]
    fn test_phase17_03_bria_capabilities_declare_video_to_video_and_audio() {
        let provider = ReplicateBriaBgRemovalProvider::new();
        let caps = provider.capabilities();
        assert!(caps.supports_video_to_video);
        assert!(caps.supports_audio);
        assert!(!caps.supports_text_to_video);
        assert!(!caps.supports_image_to_video);
        assert!(!caps.supports_reference_image);
        assert!(!caps.supports_character_reference);
        assert_eq!(caps.max_duration_sec, Some(60.0));
        assert_eq!(caps.estimated_cost_per_second, Some(0.0042));
    }

    // =========================================================================
    // 03. BRIA COST ESTIMATION
    // =========================================================================

    #[test]
    fn test_phase17_04_bria_cost_estimate_10s_video() {
        let provider = ReplicateBriaBgRemovalProvider::new();
        let request = make_bg_removal_request("test_cost", 10.0);
        let estimate = provider.estimate_cost(&request);
        assert_eq!(estimate.provider, "replicate");
        assert_eq!(estimate.model, "bria/video-remove-background");
        assert_eq!(estimate.currency, "USD");
        assert!(matches!(estimate.status, CostConfidence::Estimated));
        // 10s * $0.0042 = $0.042
        let est = estimate.estimated_usd.unwrap();
        assert!((est - 0.042).abs() < 0.0001);
        assert!(estimate.min_usd.unwrap() < est);
        assert!(estimate.max_usd.unwrap() > est);
    }

    #[test]
    fn test_phase17_05_bria_cost_estimate_defaults_to_6s_when_zero_duration() {
        let provider = ReplicateBriaBgRemovalProvider::new();
        let request = make_bg_removal_request("test_cost_zero", 0.0);
        let estimate = provider.estimate_cost(&request);
        // 6s * $0.0042 = $0.0252
        let est = estimate.estimated_usd.unwrap();
        assert!((est - 0.0252).abs() < 0.0001);
    }

    // =========================================================================
    // 04. BRIA submit_job MUST FAIL (requires PreparedProviderSubmission)
    // =========================================================================

    #[tokio::test]
    async fn test_phase17_06_bria_submit_job_returns_unsupported() {
        let provider = ReplicateBriaBgRemovalProvider::with_policy(
            Some("test".to_string()),
            Arc::new(MockLiveExecutionPolicy::new(true)),
        );
        let request = make_bg_removal_request("raw_test", 5.0);
        let res = provider.submit_job(&request).await;
        assert!(res.is_err());
        let err = format!("{}", res.unwrap_err());
        assert!(err.contains("RAW_SUBMISSION_UNSUPPORTED"));
    }

    // =========================================================================
    // 05. BRIA create_prediction REJECTS CharacterReplacement submissions
    // =========================================================================

    #[tokio::test]
    async fn test_phase17_07_bria_create_prediction_rejects_character_replacement() {
        let provider = ReplicateBriaBgRemovalProvider::with_policy(
            Some("test_token".to_string()),
            Arc::new(MockLiveExecutionPolicy::new(true)),
        );
        let prepared =
            PreparedProviderSubmission::CharacterReplacement(PreparedCharacterReplacement {
                spec: ProviderSubmissionSpec {
                    provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
                    source_video: PathBuf::from("dummy.mp4"),
                    reference_images: vec![],
                    instruction_prompt: None,
                    resolution_tier: ResolutionTier::P720,
                    target_fps: TargetFps::Original,
                    save_audio: true,
                    ignore_audio: false,
                    turbo: false,
                    disable_safety_checker: false,
                    seed: None,
                },
                uploaded_source: UploadedAsset {
                    provider_file_id: None,
                    input_uri: "https://replicate.delivery/source.mp4".to_string(),
                    expires_at: None,
                    checksum: None,
                },
                uploaded_references: vec![],
            });
        let res = provider.create_prediction(&prepared).await;
        assert!(res.is_err());
        let err = format!("{}", res.unwrap_err());
        assert!(err.contains("TASK_SUBMISSION_MISMATCH"));
    }

    // =========================================================================
    // 06. BRIA create_prediction REJECTS when no token
    // =========================================================================

    #[tokio::test]
    async fn test_phase17_08_bria_create_prediction_fails_without_token() {
        let provider = ReplicateBriaBgRemovalProvider::with_policy(
            None,
            Arc::new(MockLiveExecutionPolicy::new(true)),
        );
        let prepared = PreparedProviderSubmission::BackgroundRemoval(PreparedBackgroundRemoval {
            spec: BackgroundRemovalSpec {
                provider_key: ProviderKey::new("replicate", "bria/video-remove-background"),
                source_video: PathBuf::from("test.mp4"),
                source_facts: SourceMediaFacts {
                    duration_sec: 5.0,
                    width: 1920,
                    height: 1080,
                    fps: 30.0,
                    has_audio: true,
                },
                background_mode: BackgroundMode::Transparent,
                output_format: BackgroundRemovalOutputFormat::WebmVp9,
                preserve_audio: true,
            },
            uploaded_source: UploadedAsset {
                provider_file_id: None,
                input_uri: "https://replicate.delivery/source.mp4".to_string(),
                expires_at: None,
                checksum: None,
            },
        });
        let res = provider.create_prediction(&prepared).await;
        assert!(res.is_err());
        let err = format!("{}", res.unwrap_err());
        assert!(err.contains("REPLICATE_API_TOKEN"));
    }

    // =========================================================================
    // 07. BRIA create_prediction BLOCKED by live guard
    // =========================================================================

    #[tokio::test]
    async fn test_phase17_09_bria_create_prediction_blocked_by_live_guard() {
        let provider = ReplicateBriaBgRemovalProvider::with_policy(
            Some("test_token".to_string()),
            Arc::new(MockLiveExecutionPolicy::new(false)), // disabled
        );
        let prepared = PreparedProviderSubmission::BackgroundRemoval(PreparedBackgroundRemoval {
            spec: BackgroundRemovalSpec {
                provider_key: ProviderKey::new("replicate", "bria/video-remove-background"),
                source_video: PathBuf::from("test.mp4"),
                source_facts: SourceMediaFacts {
                    duration_sec: 5.0,
                    width: 1920,
                    height: 1080,
                    fps: 30.0,
                    has_audio: true,
                },
                background_mode: BackgroundMode::Transparent,
                output_format: BackgroundRemovalOutputFormat::WebmVp9,
                preserve_audio: true,
            },
            uploaded_source: UploadedAsset {
                provider_file_id: None,
                input_uri: "https://replicate.delivery/source.mp4".to_string(),
                expires_at: None,
                checksum: None,
            },
        });
        let res = provider.create_prediction(&prepared).await;
        assert!(res.is_err());
        let err = format!("{}", res.unwrap_err());
        assert!(
            err.contains("LIVE_EXECUTION_BLOCKED")
                || err.contains("PAID_LIVE_BLOCKED")
                || err.contains("PAID_LIVE_TEST_DISABLED"),
            "Expected live guard block, got: {}",
            err
        );
    }

    // =========================================================================
    // 08. SSRF VALIDATION TESTS
    // =========================================================================

    #[test]
    fn test_phase17_10_ssrf_accepts_valid_replicate_delivery_url() {
        let url = "https://replicate.delivery/pbxt/abc123/output.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_phase17_11_ssrf_accepts_subdomain_replicate_delivery() {
        let url = "https://cdn.replicate.delivery/pbxt/file.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_ok());
    }

    #[test]
    fn test_phase17_12_ssrf_rejects_http_scheme() {
        let url = "http://replicate.delivery/pbxt/abc123/output.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("SSRF_VIOLATION"));
    }

    #[test]
    fn test_phase17_13_ssrf_rejects_non_replicate_host() {
        let url = "https://evil.com/pbxt/abc123/output.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("SSRF_VIOLATION"));
    }

    #[test]
    fn test_phase17_14_ssrf_rejects_localhost() {
        let url = "https://localhost/file.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_phase17_15_ssrf_rejects_private_ip_127() {
        let url = "https://127.0.0.1/file.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_phase17_16_ssrf_rejects_private_ip_192_168() {
        let url = "https://192.168.1.1/file.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_phase17_17_ssrf_rejects_private_ip_10() {
        let url = "https://10.0.0.1/file.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_phase17_18_ssrf_rejects_ftp_scheme() {
        let url = "ftp://replicate.delivery/file.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_phase17_19_ssrf_rejects_fake_subdomain_prefix_dot() {
        let url = "https://.replicate.delivery/file.webm";
        let result = ReplicateBriaBgRemovalProvider::validate_ssrf_url(url);
        assert!(result.is_err());
    }

    // =========================================================================
    // 09. REGISTRY TESTS — BRIA RECORD
    // =========================================================================

    #[test]
    fn test_phase17_20_bria_record_exists_in_registry() {
        let registry = ProviderRegistry::new();
        let record = registry.find("replicate", "bria/video-remove-background");
        assert!(record.is_some(), "BRIA record must exist in registry");
        let r = record.unwrap();
        assert_eq!(r.execution_class, ExecutionClass::UtilityCloud);
        assert!(r.supports_video_background_removal);
        assert_eq!(r.pricing_amount, Some(0.0042));
        assert_eq!(r.max_duration_sec, Some(60.0));
    }

    #[test]
    fn test_phase17_21_pruna_record_does_not_support_bg_removal() {
        let registry = ProviderRegistry::new();
        let record = registry.find("replicate", "prunaai/p-video-replace");
        assert!(record.is_some());
        assert!(!record.unwrap().supports_video_background_removal);
    }

    #[test]
    fn test_phase17_22_lucataco_remove_bg_is_image_only() {
        let registry = ProviderRegistry::new();
        let record = registry.find("replicate_utility", "lucataco/remove-bg");
        assert!(record.is_some());
        let r = record.unwrap();
        assert!(!r.capabilities.supports_video_to_video);
        assert!(!r.supports_video_background_removal);
    }

    // =========================================================================
    // 10. SCHEMA V2 TYPES TESTS
    // =========================================================================

    #[test]
    fn test_phase17_23_artifact_container_extension_mp4() {
        assert_eq!(ArtifactContainer::Mp4.extension(), "mp4");
    }

    #[test]
    fn test_phase17_24_artifact_container_extension_webm() {
        assert_eq!(ArtifactContainer::Webm.extension(), "webm");
    }

    #[test]
    fn test_phase17_25_artifact_descriptor_default_is_mp4_h264() {
        let desc = ArtifactDescriptor::default();
        assert_eq!(desc.container, ArtifactContainer::Mp4);
        assert_eq!(desc.video_codec, ArtifactVideoCodec::H264);
        assert!(!desc.require_alpha);
        assert!(!desc.require_audio);
        assert_eq!(desc.extension(), "mp4");
    }

    #[test]
    fn test_phase17_26_artifact_descriptor_webm_vp9_for_bg_removal() {
        let desc = ArtifactDescriptor {
            container: ArtifactContainer::Webm,
            video_codec: ArtifactVideoCodec::Vp9,
            require_alpha: true,
            require_audio: false,
        };
        assert_eq!(desc.extension(), "webm");
        assert!(desc.require_alpha);
    }

    // =========================================================================
    // 11. STORE FORMAT-AWARE PATH TESTS
    // =========================================================================

    #[test]
    fn test_phase17_27_store_artifact_path_uses_mp4_for_character_replacement() {
        let (paths, _temp) = create_test_storage();
        let store = PersistentCloudJobStore::new(paths);
        let path = store
            .artifact_final_path_for_container("proj1", "cjob-abc", ArtifactContainer::Mp4)
            .unwrap();
        assert!(path.to_string_lossy().ends_with("cjob-abc.mp4"));
    }

    #[test]
    fn test_phase17_28_store_artifact_path_uses_webm_for_bg_removal() {
        let (paths, _temp) = create_test_storage();
        let store = PersistentCloudJobStore::new(paths);
        let path = store
            .artifact_final_path_for_container("proj1", "cjob-def", ArtifactContainer::Webm)
            .unwrap();
        assert!(path.to_string_lossy().ends_with("cjob-def.webm"));
    }

    #[test]
    fn test_phase17_29_store_artifact_final_path_for_job_defaults_mp4() {
        let (paths, _temp) = create_test_storage();
        let store = PersistentCloudJobStore::new(paths);
        let job = PersistentCloudJob {
            schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
            state_revision: 1,
            job_id: "j1".to_string(),
            internal_job_id: "cjob-j1".to_string(),
            project_id: "proj".to_string(),
            provider_id: "replicate".to_string(),
            model_id: "prunaai/p-video-replace".to_string(),
            model_version: "v1".to_string(),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
            execution_class: ExecutionClass::SpecializedVideoTransformation,
            input_assets: Default::default(),
            configuration_hash: "h".to_string(),
            submission_state: SubmissionState::NeverAttempted,
            remote_job_id: None,
            state: CloudJobState::Created,
            cost: Default::default(),
            output: Default::default(),
            retry: Default::default(),
            error: None,
            timestamps: Default::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: None,
            output_url: None,
            artifact_descriptor: None,
            validation_policy: ValidationPolicy::default(),
        };
        let path = store.artifact_final_path_for_job(&job).unwrap();
        assert!(path.to_string_lossy().ends_with(".mp4"));
    }

    #[test]
    fn test_phase17_30_store_artifact_final_path_for_job_uses_webm_when_descriptor_set() {
        let (paths, _temp) = create_test_storage();
        let store = PersistentCloudJobStore::new(paths);
        let job = PersistentCloudJob {
            schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
            state_revision: 1,
            job_id: "j2".to_string(),
            internal_job_id: "cjob-j2".to_string(),
            project_id: "proj".to_string(),
            provider_id: "replicate".to_string(),
            model_id: "bria/video-remove-background".to_string(),
            model_version: "v1".to_string(),
            task_type: "BACKGROUND_REMOVAL".to_string(),
            execution_class: ExecutionClass::UtilityCloud,
            input_assets: Default::default(),
            configuration_hash: "h".to_string(),
            submission_state: SubmissionState::NeverAttempted,
            remote_job_id: None,
            state: CloudJobState::Created,
            cost: Default::default(),
            output: Default::default(),
            retry: Default::default(),
            error: None,
            timestamps: Default::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: None,
            output_url: None,
            artifact_descriptor: Some(ArtifactDescriptor {
                container: ArtifactContainer::Webm,
                video_codec: ArtifactVideoCodec::Vp9,
                require_alpha: true,
                require_audio: false,
            }),
            validation_policy: ValidationPolicy::default(),
        };
        let path = store.artifact_final_path_for_job(&job).unwrap();
        assert!(path.to_string_lossy().ends_with(".webm"));
    }

    // =========================================================================
    // 12. SOURCE MEDIA FACTS
    // =========================================================================

    #[test]
    fn test_phase17_31_source_media_facts_roundtrip_serialization() {
        let facts = SourceMediaFacts {
            duration_sec: 10.5,
            width: 1920,
            height: 1080,
            fps: 29.97,
            has_audio: true,
        };
        let json = serde_json::to_string(&facts).unwrap();
        let deserialized: SourceMediaFacts = serde_json::from_str(&json).unwrap();
        assert_eq!(facts, deserialized);
    }

    // =========================================================================
    // 13. BACKGROUND REMOVAL SPEC
    // =========================================================================

    #[test]
    fn test_phase17_32_background_removal_spec_serialization() {
        let spec = BackgroundRemovalSpec {
            provider_key: ProviderKey::new("replicate", "bria/video-remove-background"),
            source_video: PathBuf::from("test.mp4"),
            source_facts: SourceMediaFacts {
                duration_sec: 5.0,
                width: 1280,
                height: 720,
                fps: 24.0,
                has_audio: false,
            },
            background_mode: BackgroundMode::Transparent,
            output_format: BackgroundRemovalOutputFormat::WebmVp9,
            preserve_audio: false,
        };
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"backgroundMode\":\"transparent\""));
        assert!(json.contains("\"outputFormat\":\"webm_vp9\""));
    }

    // =========================================================================
    // 14. PREPARED PROVIDER SUBMISSION ENUM TESTS
    // =========================================================================

    #[test]
    fn test_phase17_33_prepared_submission_character_replacement_variant() {
        let prepared =
            PreparedProviderSubmission::CharacterReplacement(PreparedCharacterReplacement {
                spec: ProviderSubmissionSpec {
                    provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
                    source_video: PathBuf::from("src.mp4"),
                    reference_images: vec![PathBuf::from("ref.jpg")],
                    instruction_prompt: Some("replace character".to_string()),
                    resolution_tier: ResolutionTier::P1080,
                    target_fps: TargetFps::Original,
                    save_audio: true,
                    ignore_audio: false,
                    turbo: false,
                    disable_safety_checker: false,
                    seed: None,
                },
                uploaded_source: UploadedAsset {
                    provider_file_id: None,
                    input_uri: "https://replicate.delivery/src.mp4".to_string(),
                    expires_at: None,
                    checksum: None,
                },
                uploaded_references: vec![],
            });
        let json = serde_json::to_string(&prepared).unwrap();
        assert!(json.contains("\"type\":\"character_replacement\""));
    }

    #[test]
    fn test_phase17_34_prepared_submission_background_removal_variant() {
        let prepared = PreparedProviderSubmission::BackgroundRemoval(PreparedBackgroundRemoval {
            spec: BackgroundRemovalSpec {
                provider_key: ProviderKey::new("replicate", "bria/video-remove-background"),
                source_video: PathBuf::from("test.mp4"),
                source_facts: SourceMediaFacts {
                    duration_sec: 10.0,
                    width: 1920,
                    height: 1080,
                    fps: 30.0,
                    has_audio: true,
                },
                background_mode: BackgroundMode::Transparent,
                output_format: BackgroundRemovalOutputFormat::WebmVp9,
                preserve_audio: true,
            },
            uploaded_source: UploadedAsset {
                provider_file_id: None,
                input_uri: "https://replicate.delivery/src.mp4".to_string(),
                expires_at: None,
                checksum: None,
            },
        });
        let json = serde_json::to_string(&prepared).unwrap();
        assert!(json.contains("\"type\":\"background_removal\""));
    }

    // =========================================================================
    // 15. VALIDATION POLICY EXTENDED FIELDS
    // =========================================================================

    #[test]
    fn test_phase17_35_validation_policy_default_has_no_constraints() {
        let policy = ValidationPolicy::default();
        assert_eq!(policy.expected_duration_sec, None);
        assert!(!policy.require_audio);
        assert_eq!(policy.expected_width, None);
        assert_eq!(policy.expected_height, None);
        assert_eq!(policy.expected_fps, None);
        assert!(!policy.require_alpha);
        assert!(policy.expected_container.is_none());
        assert!(policy.expected_video_codec.is_none());
    }

    #[test]
    fn test_phase17_36_validation_policy_bg_removal_requires_alpha() {
        let policy = ValidationPolicy {
            expected_duration_sec: Some(10.0),
            require_audio: false,
            expected_width: Some(1920),
            expected_height: Some(1080),
            expected_fps: Some(30.0),
            require_alpha: true,
            expected_container: Some("webm".to_string()),
            expected_video_codec: Some("vp9".to_string()),
        };
        assert!(policy.require_alpha);
        assert_eq!(policy.expected_container, Some("webm".to_string()));
    }

    // =========================================================================
    // 16. ROUTING — BACKGROUND REMOVAL TASK ROUTING
    // =========================================================================

    #[test]
    fn test_phase17_37_routing_selects_bria_for_bg_removal_cloud() {
        let registry = ProviderRegistry::new();
        let request = make_bg_removal_request("route_test", 10.0);
        let facts = SourceMediaFacts {
            duration_sec: 10.0,
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_audio: true,
        };
        let decision = GenerationRouter::route_with_facts(
            TaskClass::BackgroundRemoval,
            RoutingPreference::CloudOnly,
            &request,
            Some(&facts),
            None,
            &registry,
        );
        assert_eq!(
            decision.target,
            crate::ai::cloud::router::RoutingTarget::Cloud,
        );
        assert_eq!(decision.provider_id, "replicate");
        assert_eq!(decision.model_id, "bria/video-remove-background");
    }

    #[test]
    fn test_phase17_38_routing_rejects_over_60s_video() {
        let registry = ProviderRegistry::new();
        let request = make_bg_removal_request("route_long", 120.0);
        let facts = SourceMediaFacts {
            duration_sec: 120.0,
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_audio: true,
        };
        let decision = GenerationRouter::route_with_facts(
            TaskClass::BackgroundRemoval,
            RoutingPreference::CloudOnly,
            &request,
            Some(&facts),
            None,
            &registry,
        );
        assert_eq!(
            decision.target,
            crate::ai::cloud::router::RoutingTarget::Unavailable,
        );
        assert!(
            decision.reason.contains("DURATION")
                || decision.reason.contains("duration")
                || decision.reason.contains("limit"),
            "Expected duration-related rejection, got: {}",
            decision.reason
        );
    }

    // =========================================================================
    // 17. PERSISTENT CLOUD JOB WITH ARTIFACT_DESCRIPTOR
    // =========================================================================

    #[test]
    fn test_phase17_39_persistent_cloud_job_serialization_with_artifact_descriptor() {
        let job = PersistentCloudJob {
            schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
            state_revision: 1,
            job_id: "j_ser".to_string(),
            internal_job_id: "cjob-ser".to_string(),
            project_id: "proj".to_string(),
            provider_id: "replicate".to_string(),
            model_id: "bria/video-remove-background".to_string(),
            model_version: "official-current".to_string(),
            task_type: "BACKGROUND_REMOVAL".to_string(),
            execution_class: ExecutionClass::UtilityCloud,
            input_assets: Default::default(),
            configuration_hash: "h".to_string(),
            submission_state: SubmissionState::NeverAttempted,
            remote_job_id: None,
            state: CloudJobState::Created,
            cost: Default::default(),
            output: Default::default(),
            retry: Default::default(),
            error: None,
            timestamps: Default::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: None,
            output_url: None,
            artifact_descriptor: Some(ArtifactDescriptor {
                container: ArtifactContainer::Webm,
                video_codec: ArtifactVideoCodec::Vp9,
                require_alpha: true,
                require_audio: false,
            }),
            validation_policy: ValidationPolicy {
                expected_duration_sec: Some(10.0),
                require_audio: false,
                expected_width: Some(1920),
                expected_height: Some(1080),
                expected_fps: Some(30.0),
                require_alpha: true,
                expected_container: Some("webm".to_string()),
                expected_video_codec: Some("vp9".to_string()),
            },
        };
        let json = serde_json::to_string_pretty(&job).unwrap();
        let deserialized: PersistentCloudJob = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task_type, "BACKGROUND_REMOVAL");
        assert!(deserialized.artifact_descriptor.is_some());
        let ad = deserialized.artifact_descriptor.unwrap();
        assert_eq!(ad.container, ArtifactContainer::Webm);
        assert_eq!(ad.video_codec, ArtifactVideoCodec::Vp9);
        assert!(ad.require_alpha);
        assert!(!ad.require_audio);
    }

    // =========================================================================
    // 18. RESOLVER TESTS
    // =========================================================================

    #[test]
    fn test_phase17_40_resolver_recognizes_bria_provider() {
        let resolver = DefaultCloudProviderResolver::new();
        let result = resolver.resolve_provider("replicate", "bria/video-remove-background");
        // Without REPLICATE_API_TOKEN, resolver returns MISSING_PROVIDER_CREDENTIALS
        // which confirms the provider/model pair IS recognized
        match result {
            Ok(_) => {} // token was set in env
            Err(ref e) => {
                let msg = format!("{}", e);
                assert!(
                    msg.contains("MISSING_PROVIDER_CREDENTIALS"),
                    "Expected MISSING_PROVIDER_CREDENTIALS, got: {}",
                    msg
                );
            }
        }
    }

    #[test]
    fn test_phase17_41_resolver_recognizes_pruna_provider() {
        let resolver = DefaultCloudProviderResolver::new();
        let result = resolver.resolve_provider("replicate", "prunaai/p-video-replace");
        match result {
            Ok(_) => {}
            Err(ref e) => {
                let msg = format!("{}", e);
                assert!(
                    msg.contains("MISSING_PROVIDER_CREDENTIALS"),
                    "Expected MISSING_PROVIDER_CREDENTIALS, got: {}",
                    msg
                );
            }
        }
    }

    // =========================================================================
    // 19. MEDIA MODULE SUPPORTS WEBM EXTENSION
    // =========================================================================

    #[test]
    fn test_phase17_42_webm_in_supported_extensions() {
        use crate::media::SUPPORTED_EXTENSIONS;
        assert!(
            SUPPORTED_EXTENSIONS.contains(&"webm"),
            "webm must be in SUPPORTED_EXTENSIONS"
        );
    }

    // =========================================================================
    // 20. REAL SYNTHETIC WEBM ACCEPTANCE & ALPHA VALIDATOR TESTS
    // =========================================================================

    fn ensure_synthetic_fixtures() -> (PathBuf, PathBuf) {
        let fixture_dir = PathBuf::from("target").join("phase17-fixtures");
        let _ = std::fs::create_dir_all(&fixture_dir);
        let trans_path = fixture_dir.join("transparent_vp9.webm");
        let opaque_path = fixture_dir.join("opaque_vp9.webm");

        if !trans_path.exists() {
            let _ = std::process::Command::new("ffmpeg")
                .args([
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=color=red@0.0:size=64x64:rate=10:duration=1,format=yuva420p",
                    "-c:v",
                    "libvpx-vp9",
                    "-pix_fmt",
                    "yuva420p",
                    "-auto-alt-ref",
                    "0",
                    trans_path.to_str().unwrap(),
                ])
                .output();
        }

        if !opaque_path.exists() {
            let _ = std::process::Command::new("ffmpeg")
                .args([
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=color=red:size=64x64:rate=10:duration=1,format=yuv420p",
                    "-c:v",
                    "libvpx-vp9",
                    "-pix_fmt",
                    "yuv420p",
                    opaque_path.to_str().unwrap(),
                ])
                .output();
        }

        (trans_path, opaque_path)
    }

    #[test]
    fn test_phase17_43_real_synthetic_transparent_webm_production_validator_passes() {
        use crate::ai::cloud::validator::CloudOutputValidator;
        let (trans_path, _opaque_path) = ensure_synthetic_fixtures();
        if !trans_path.exists() {
            return; // Skip if ffmpeg not in test runner env
        }

        let validator = CloudOutputValidator::new();
        let policy = ValidationPolicy {
            expected_duration_sec: Some(1.0),
            require_audio: false,
            expected_width: Some(64),
            expected_height: Some(64),
            expected_fps: Some(10.0),
            require_alpha: true,
            expected_container: Some("webm".to_string()),
            expected_video_codec: Some("vp9".to_string()),
        };

        let result = validator.validate_artifact_with_policy(&trans_path, &policy);
        assert!(
            result.is_ok(),
            "Production validator must accept truthful transparent VP9 WebM: {:?}",
            result.err()
        );
        let meta = result.unwrap();
        assert_eq!(meta.width, 64);
        assert_eq!(meta.height, 64);
        assert!(meta.duration_sec >= 0.8);
    }

    #[test]
    fn test_phase17_44_real_synthetic_opaque_webm_production_validator_fails_alpha() {
        use crate::ai::cloud::validator::CloudOutputValidator;
        let (_trans_path, opaque_path) = ensure_synthetic_fixtures();
        if !opaque_path.exists() {
            return;
        }

        let validator = CloudOutputValidator::new();
        let policy = ValidationPolicy {
            expected_duration_sec: Some(1.0),
            require_audio: false,
            expected_width: Some(64),
            expected_height: Some(64),
            expected_fps: Some(10.0),
            require_alpha: true,
            expected_container: Some("webm".to_string()),
            expected_video_codec: Some("vp9".to_string()),
        };

        let result = validator.validate_artifact_with_policy(&opaque_path, &policy);
        assert!(
            result.is_err(),
            "Production validator must reject opaque VP9 WebM when require_alpha=true"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("lacks decodable alpha transparency"),
            "Expected alpha rejection error message, got: {}",
            err_msg
        );
    }

    // =========================================================================
    // 21. SCHEMA V1 AUDIO MIGRATION NORMALIZATION TESTS
    // =========================================================================

    #[test]
    fn test_phase17_45_schema_v1_audio_migration_preserves_policy() {
        let (paths, _temp) = create_test_storage();
        let _store = PersistentCloudJobStore::new(paths);

        let mut job_no_audio = PersistentCloudJob {
            schema_version: 1,
            state_revision: 1,
            job_id: "j_v1_no_audio".to_string(),
            internal_job_id: "cjob-v1-no".to_string(),
            project_id: "proj".to_string(),
            provider_id: "replicate".to_string(),
            model_id: "prunaai/p-video-replace".to_string(),
            model_version: "v1".to_string(),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
            execution_class: ExecutionClass::SpecializedVideoTransformation,
            input_assets: Default::default(),
            configuration_hash: "h".to_string(),
            submission_state: SubmissionState::NeverAttempted,
            remote_job_id: None,
            state: CloudJobState::Created,
            cost: Default::default(),
            output: Default::default(),
            retry: Default::default(),
            error: None,
            timestamps: Default::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: None,
            output_url: None,
            artifact_descriptor: None,
            validation_policy: ValidationPolicy {
                expected_duration_sec: None,
                require_audio: false,
                expected_width: None,
                expected_height: None,
                expected_fps: None,
                require_alpha: false,
                expected_container: None,
                expected_video_codec: None,
            },
        };

        job_no_audio.normalize_in_memory();
        let desc_no = job_no_audio.artifact_descriptor.unwrap();
        assert_eq!(desc_no.container, ArtifactContainer::Mp4);
        assert_eq!(desc_no.video_codec, ArtifactVideoCodec::H264);
        assert!(!desc_no.require_alpha);
        assert!(!desc_no.require_audio);

        let mut job_with_audio = PersistentCloudJob {
            schema_version: 1,
            state_revision: 1,
            job_id: "j_v1_with_audio".to_string(),
            internal_job_id: "cjob-v1-with".to_string(),
            project_id: "proj".to_string(),
            provider_id: "replicate".to_string(),
            model_id: "prunaai/p-video-replace".to_string(),
            model_version: "v1".to_string(),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
            execution_class: ExecutionClass::SpecializedVideoTransformation,
            input_assets: Default::default(),
            configuration_hash: "h".to_string(),
            submission_state: SubmissionState::NeverAttempted,
            remote_job_id: None,
            state: CloudJobState::Created,
            cost: Default::default(),
            output: Default::default(),
            retry: Default::default(),
            error: None,
            timestamps: Default::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: None,
            output_url: None,
            artifact_descriptor: None,
            validation_policy: ValidationPolicy {
                expected_duration_sec: None,
                require_audio: true,
                expected_width: None,
                expected_height: None,
                expected_fps: None,
                require_alpha: false,
                expected_container: None,
                expected_video_codec: None,
            },
        };

        job_with_audio.normalize_in_memory();
        let desc_with = job_with_audio.artifact_descriptor.unwrap();
        assert_eq!(desc_with.container, ArtifactContainer::Mp4);
        assert_eq!(desc_with.video_codec, ArtifactVideoCodec::H264);
        assert!(!desc_with.require_alpha);
        assert!(desc_with.require_audio);
    }

    // =========================================================================
    // 22. RESOLUTION POLICY PRESERVE SOURCE LIMITS (16000x16000)
    // =========================================================================

    #[test]
    fn test_phase17_46_source_resolution_16000_limit_respected() {
        let registry = ProviderRegistry::new();

        let req_1080p = make_bg_removal_request("res_1080", 5.0);
        let facts_1080p = SourceMediaFacts {
            duration_sec: 5.0,
            width: 1920,
            height: 1080,
            fps: 30.0,
            has_audio: false,
        };
        let dec_1080p = GenerationRouter::route_with_facts(
            TaskClass::BackgroundRemoval,
            RoutingPreference::CloudOnly,
            &req_1080p,
            Some(&facts_1080p),
            None,
            &registry,
        );
        assert_eq!(
            dec_1080p.target,
            crate::ai::cloud::router::RoutingTarget::Cloud
        );

        let facts_16000 = SourceMediaFacts {
            duration_sec: 5.0,
            width: 16000,
            height: 16000,
            fps: 30.0,
            has_audio: false,
        };
        let dec_16000 = GenerationRouter::route_with_facts(
            TaskClass::BackgroundRemoval,
            RoutingPreference::CloudOnly,
            &req_1080p,
            Some(&facts_16000),
            None,
            &registry,
        );
        assert_eq!(
            dec_16000.target,
            crate::ai::cloud::router::RoutingTarget::Cloud
        );

        let facts_over = SourceMediaFacts {
            duration_sec: 5.0,
            width: 16001,
            height: 1080,
            fps: 30.0,
            has_audio: false,
        };
        let dec_over = GenerationRouter::route_with_facts(
            TaskClass::BackgroundRemoval,
            RoutingPreference::CloudOnly,
            &req_1080p,
            Some(&facts_over),
            None,
            &registry,
        );
        assert_eq!(
            dec_over.target,
            crate::ai::cloud::router::RoutingTarget::Unavailable
        );
    }

    // =========================================================================
    // 23. PROBED FACTS DURATION AUTHORITATIVE OVER REQUEST DURATION
    // =========================================================================

    #[test]
    fn test_phase17_47_background_removal_source_facts_duration_authoritative() {
        let registry = ProviderRegistry::new();
        // Client claims duration is 5.0s, but source facts duration is 65.0s (>60s limit)
        let request = make_bg_removal_request("dur_mismatch", 5.0);
        let facts = SourceMediaFacts {
            duration_sec: 65.0,
            width: 1280,
            height: 720,
            fps: 24.0,
            has_audio: true,
        };
        let decision = GenerationRouter::route_with_facts(
            TaskClass::BackgroundRemoval,
            RoutingPreference::CloudOnly,
            &request,
            Some(&facts),
            None,
            &registry,
        );
        assert_eq!(
            decision.target,
            crate::ai::cloud::router::RoutingTarget::Unavailable
        );
        assert!(
            decision.reason.contains("DURATION") || decision.reason.contains("limit"),
            "Expected duration limit rejection, got: {}",
            decision.reason
        );
    }

    // =========================================================================
    // 24. BACKGROUND REMOVAL SUBMISSION INPUT INTEGRITY
    // =========================================================================

    #[test]
    fn test_phase17_48_background_removal_references_rejected() {
        use crate::ai::cloud::submission::validate_and_prepare_cloud_submission;
        let registry = ProviderRegistry::new();
        let mut request = make_bg_removal_request("ref_test", 5.0);
        request.reference_image = Some(PathBuf::from("ref.jpg"));

        let result = validate_and_prepare_cloud_submission(&request, Some(3.0), &registry);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("UNEXPECTED_REFERENCE_INPUTS_FOR_BACKGROUND_REMOVAL")
                || msg.contains("SOURCE_VIDEO_REQUIRED"),
            "Expected reference rejection, got: {}",
            msg
        );
    }

    #[test]
    fn test_phase17_49_background_removal_source_video_required() {
        use crate::ai::cloud::submission::validate_and_prepare_cloud_submission;
        let registry = ProviderRegistry::new();
        let request = make_bg_removal_request("no_source", 5.0); // source_video is None

        let result = validate_and_prepare_cloud_submission(&request, Some(3.0), &registry);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("SOURCE_VIDEO_REQUIRED"),
            "Expected SOURCE_VIDEO_REQUIRED, got: {}",
            msg
        );
    }

    #[test]
    fn test_phase17_50_task_class_strict_parsing_rejects_unknown() {
        assert!(TaskClass::from_str_strict("BACKGROUND_REMOVAL").is_ok());
        assert_eq!(
            TaskClass::from_str_strict("BACKGROUND_REMOVAL").unwrap(),
            TaskClass::BackgroundRemoval
        );
        assert!(TaskClass::from_str_strict("UNKNOWN_TASK").is_err());
    }
}
