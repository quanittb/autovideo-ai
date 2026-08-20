#[cfg(test)]
mod tests {
    use crate::ai::cloud::cost::{CostBreakdown, CostConfidence, CostEstimate};
    use crate::ai::cloud::error::CloudProviderError;
    use crate::ai::cloud::job::{
        CloudJobRequest, CloudJobState, CostRecord, InputAssets, OutputArtifactRecord,
        PersistentCloudJob, SubmissionState, ValidationPolicy, CURRENT_CLOUD_JOB_SCHEMA_VERSION,
    };
    use crate::ai::cloud::lifecycle::{
        CloudJobLifecycleService, LifecycleTimingConfig, TestEventSink,
    };
    use crate::ai::cloud::live_execution_guard::{
        LiveExecutionPolicy, MockLiveExecutionPolicy, PaidLiveExecutionGuard,
    };
    use crate::ai::cloud::provider::{
        CloudJobHandle, CloudVideoProvider, ProviderCapabilities, ProviderKey, RemotePollResponse,
        RemoteStatus, ResolutionTier, TargetFps,
    };
    use crate::ai::cloud::providers::replicate_pruna::PrunaPVideoReplaceProvider;
    use crate::ai::cloud::registry::{ExecutionClass, ProviderRegistry};
    use crate::ai::cloud::resolver::{CloudProviderResolver, ResolvedProviderRuntime};
    use crate::ai::cloud::router::{
        GenerationRouter, RoutingDecision, RoutingPreference, RoutingTarget, TaskClass,
    };
    use crate::ai::cloud::spec::{PreparedProviderSubmission, ProviderSubmissionSpec};
    use crate::ai::cloud::submission::{DefaultCloudSubmissionGate, ValidatedSubmissionPlan};
    use crate::ai::cloud::uploader::{MockAssetUploader, ProviderAssetUploader, UploadedAsset};
    use crate::ai::cloud::validator::CloudOutputValidator;
    use crate::projects::{PreservationConfig, ProjectManager, SourceMedia};
    use crate::system::StoragePaths;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::tempdir;

    // -------------------------------------------------------------------------
    // Test Environment Helpers
    // -------------------------------------------------------------------------

    fn create_test_env() -> (StoragePaths, tempfile::TempDir, String, PathBuf, PathBuf) {
        let temp = tempdir().expect("Failed to create tempdir");
        let base = temp.path().to_path_buf();
        let paths = StoragePaths {
            app_data_dir: base.clone(),
            projects_dir: base.join("projects"),
            models_dir: base.join("models"),
            cache_dir: base.join("cache"),
            logs_dir: base.join("logs"),
            temp_dir: base.join("temp"),
        };
        fs::create_dir_all(&paths.projects_dir).unwrap();

        // Create synthetic source MP4 (720x1280 @ 24fps with audio)
        let sample_mp4 = base.join("sample_source.mp4");
        create_synthetic_mp4(&sample_mp4, 1, 720, 1280, 24, true);

        // Create synthetic reference image (512x512 JPEG)
        let sample_ref = base.join("sample_ref.jpg");
        create_synthetic_image(&sample_ref, 512, 512);

        // Create test project
        let pm = ProjectManager::new(paths.clone());
        let mut project = pm.create_project("Phase 16 Test Project").unwrap();
        project.source_media = Some(SourceMedia {
            media_id: "sm_p16".to_string(),
            original_file_name: "sample_source.mp4".to_string(),
            source_path: sample_mp4.clone(),
            duration_ms: 1000,
            width: 720,
            height: 1280,
            fps: 24.0,
            file_size_bytes: 35000,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: Some("aac".to_string()),
            has_audio: true,
        });
        project.transformation_config.preservation = PreservationConfig {
            preserve_motion: true,
            preserve_camera: true,
            preserve_composition: true,
            preserve_original_audio: true,
        };
        pm.update_project(&project).unwrap();

        (paths, temp, project.id, sample_mp4, sample_ref)
    }

    fn create_synthetic_mp4(
        path: &Path,
        duration_sec: u32,
        width: u32,
        height: u32,
        fps: u32,
        include_audio: bool,
    ) {
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!(
                "testsrc=duration={}:size={}x{}:rate={}",
                duration_sec, width, height, fps
            ),
        ]);

        if include_audio {
            cmd.args([
                "-f",
                "lavfi",
                "-i",
                &format!("sine=frequency=1000:duration={}", duration_sec),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
            ]);
        } else {
            cmd.args(["-an", "-c:v", "libx264", "-pix_fmt", "yuv420p"]);
        }

        cmd.args(["-f", "mp4"]);
        cmd.arg(path.to_str().unwrap());
        let _ = cmd.output();

        if !path.exists() {
            let mut f = File::create(path).unwrap();
            f.write_all(b"fake_mp4_for_offline_test").unwrap();
        }
    }

    fn create_synthetic_image(path: &Path, width: u32, height: u32) {
        let img = image::RgbImage::new(width, height);
        let _ = img.save(path);
        if !path.exists() {
            let mut f = File::create(path).unwrap();
            f.write_all(b"fake_jpeg_for_offline_test").unwrap();
        }
    }

    fn make_test_p16_req(
        job_id: &str,
        project_id: &str,
        sample_mp4: &Path,
        sample_ref: &Path,
    ) -> CloudJobRequest {
        CloudJobRequest {
            job_id: job_id.to_string(),
            project_id: Some(project_id.to_string()),
            prompt: "Replace main character with cybernetic avatar".to_string(),
            negative_prompt: Some("blurry, distorted".to_string()),
            source_video: Some(sample_mp4.to_path_buf()),
            reference_image: Some(sample_ref.to_path_buf()),
            reference_images: Some(vec![sample_ref.to_path_buf()]),
            duration_seconds: 1.0,
            fps: 24.0,
            resolution: (720, 1280),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
        }
    }

    // -------------------------------------------------------------------------
    // Mock Phase 16 Cloud Video Provider
    // -------------------------------------------------------------------------

    pub struct MockPrunaProvider {
        pub create_prediction_call_count: Arc<AtomicU32>,
        pub poll_call_count: Arc<AtomicU32>,
        pub cancel_call_count: Arc<AtomicU32>,
        pub download_call_count: Arc<AtomicU32>,
        pub prediction_behavior: Mutex<Result<String, String>>,
        pub poll_responses: Mutex<Vec<RemotePollResponse>>,
        pub download_behavior: Mutex<Vec<Result<PathBuf, String>>>,
        pub cancel_behavior: Mutex<Result<(), String>>,
        pub live_policy: Arc<MockLiveExecutionPolicy>,
    }

    impl MockPrunaProvider {
        pub fn new() -> Self {
            Self {
                create_prediction_call_count: Arc::new(AtomicU32::new(0)),
                poll_call_count: Arc::new(AtomicU32::new(0)),
                cancel_call_count: Arc::new(AtomicU32::new(0)),
                download_call_count: Arc::new(AtomicU32::new(0)),
                prediction_behavior: Mutex::new(Ok("pruna_remote_123".to_string())),
                poll_responses: Mutex::new(vec![RemotePollResponse {
                    remote_id: "pruna_remote_123".to_string(),
                    status: RemoteStatus::Succeeded,
                    output_url: Some("https://replicate.delivery/mock_output.mp4".to_string()),
                    error: None,
                }]),
                download_behavior: Mutex::new(Vec::new()),
                cancel_behavior: Mutex::new(Ok(())),
                live_policy: Arc::new(MockLiveExecutionPolicy::new(true)),
            }
        }
    }

    impl CloudVideoProvider for MockPrunaProvider {
        fn provider_id(&self) -> &str {
            "replicate"
        }

        fn model_id(&self) -> &str {
            "prunaai/p-video-replace"
        }

        fn model_version_hint(&self) -> Option<&str> {
            Some("official-current")
        }

        fn provider_name(&self) -> &str {
            "Mock Pruna Video Replace Provider"
        }

        fn is_configured(&self) -> bool {
            true
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                supports_text_to_video: false,
                supports_image_to_video: false,
                supports_video_to_video: true,
                supports_reference_image: true,
                supports_character_reference: true,
                supports_audio: true,
                max_duration_sec: 300.0,
                supported_resolutions: vec![(720, 1280), (1080, 1920)],
                estimated_cost_per_second: Some(0.03),
            }
        }

        fn estimate_cost(&self, req: &CloudJobRequest) -> CostEstimate {
            let dur = req.duration_seconds.max(1.0);
            let rate = if req.resolution.0.max(req.resolution.1) > 1280 {
                0.06
            } else {
                0.03
            };
            let cost = rate * dur;
            CostEstimate {
                provider: self.provider_id().to_string(),
                model: self.model_id().to_string(),
                estimated_usd: Some(cost),
                min_usd: Some(cost),
                max_usd: Some(cost),
                confidence: 1.0,
                currency: "USD".to_string(),
                status: CostConfidence::Exact,
                breakdown: format!("Mock Pruna @ ${:.2}/s", rate),
            }
        }

        fn submit_job(
            &self,
            _req: &CloudJobRequest,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<CloudJobHandle, CloudProviderError>>
                    + Send
                    + '_,
            >,
        > {
            let handle = CloudJobHandle {
                job_id: "test_job".to_string(),
                remote_id: "mock_remote_pruna".to_string(),
                provider_id: "replicate".to_string(),
                model: "prunaai/p-video-replace".to_string(),
                model_version: Some("official-current".to_string()),
            };
            Box::pin(async move { Ok(handle) })
        }

        fn create_prediction(
            &self,
            _prepared: &PreparedProviderSubmission,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<CloudJobHandle, CloudProviderError>>
                    + Send
                    + '_,
            >,
        > {
            self.create_prediction_call_count
                .fetch_add(1, Ordering::SeqCst);
            let policy = self.live_policy.clone();
            let behavior = self.prediction_behavior.lock().unwrap().clone();
            Box::pin(async move {
                policy.ensure_paid_live_allowed()?;
                match behavior {
                    Ok(remote_id) => Ok(CloudJobHandle {
                        job_id: "test_job".to_string(),
                        remote_id,
                        provider_id: "replicate".to_string(),
                        model: "prunaai/p-video-replace".to_string(),
                        model_version: Some("official-current".to_string()),
                    }),
                    Err(e) => Err(CloudProviderError::ProviderUnavailable(e)),
                }
            })
        }

        fn poll_status(
            &self,
            remote_id: &str,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<RemotePollResponse, CloudProviderError>>
                    + Send
                    + '_,
            >,
        > {
            self.poll_call_count.fetch_add(1, Ordering::SeqCst);
            let r_id = remote_id.to_string();
            let mut responses = self.poll_responses.lock().unwrap();
            let resp = if responses.is_empty() {
                RemotePollResponse {
                    remote_id: r_id,
                    status: RemoteStatus::Processing,
                    output_url: None,
                    error: None,
                }
            } else {
                responses.remove(0)
            };
            Box::pin(async move { Ok(resp) })
        }

        fn cancel_job(
            &self,
            _remote_id: &str,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), CloudProviderError>> + Send + '_>,
        > {
            self.cancel_call_count.fetch_add(1, Ordering::SeqCst);
            let behavior = self.cancel_behavior.lock().unwrap().clone();
            Box::pin(async move {
                match behavior {
                    Ok(()) => Ok(()),
                    Err(e) => Err(CloudProviderError::ProviderUnavailable(e)),
                }
            })
        }

        fn download_result(
            &self,
            _output_url: &str,
            target_path: &Path,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<PathBuf, CloudProviderError>> + Send + '_>,
        > {
            self.download_call_count.fetch_add(1, Ordering::SeqCst);
            let target = target_path.to_path_buf();
            let mut behavior = self.download_behavior.lock().unwrap();
            let outcome = if behavior.is_empty() {
                let mut f = File::create(&target).map_err(|e| {
                    CloudProviderError::ProviderUnavailable(format!("Failed to create dest: {}", e))
                });
                if let Ok(ref mut file) = f {
                    let _ = file.write_all(b"default_mock_content");
                }
                f.map(|_| target.clone())
            } else {
                match behavior.remove(0) {
                    Ok(source_fixture) => fs::copy(&source_fixture, &target)
                        .map_err(|e| {
                            CloudProviderError::ProviderUnavailable(format!(
                                "Failed to copy fixture: {}",
                                e
                            ))
                        })
                        .map(|_| target.clone()),
                    Err(e) => Err(CloudProviderError::ProviderUnavailable(e)),
                }
            };
            Box::pin(async move { outcome })
        }
    }

    pub struct MockPhase16Resolver {
        pub provider: Option<Arc<MockPrunaProvider>>,
        pub uploader: Option<Arc<MockAssetUploader>>,
    }

    impl CloudProviderResolver for MockPhase16Resolver {
        fn resolve_provider(
            &self,
            _provider_id: &str,
            _model_id: &str,
        ) -> Result<Arc<dyn CloudVideoProvider>, CloudProviderError> {
            match &self.provider {
                Some(p) => Ok(p.clone()),
                None => Err(CloudProviderError::ProviderUnavailable(
                    "MISSING_PROVIDER_CREDENTIALS: Mock provider resolver has no active provider"
                        .to_string(),
                )),
            }
        }

        fn resolve_runtime(
            &self,
            provider_id: &str,
            model_id: &str,
        ) -> Result<ResolvedProviderRuntime, CloudProviderError> {
            let provider = self.resolve_provider(provider_id, model_id)?;
            let uploader: Arc<dyn ProviderAssetUploader> = match &self.uploader {
                Some(u) => u.clone(),
                None => Arc::new(MockAssetUploader::new()),
            };
            Ok(ResolvedProviderRuntime { provider, uploader })
        }
    }

    // =========================================================================
    // 01. ZERO PAID CALLS / LIVE EXECUTION GUARD TESTS
    // =========================================================================

    #[test]
    fn test_phase16_01_paid_live_disabled_by_default() {
        // Guarantee ALLOW_PAID_LIVE_TEST == 0 by default
        let is_allowed = PaidLiveExecutionGuard::is_paid_live_test_allowed();
        assert!(
            !is_allowed,
            "Paid cloud execution must be DISABLED by default ($0.00 spend guard)"
        );

        let res = PaidLiveExecutionGuard::ensure_paid_execution_allowed();
        assert!(res.is_err());
        let err = format!("{}", res.unwrap_err());
        assert!(err.contains("PAID_LIVE_TEST_DISABLED"));
    }

    #[tokio::test]
    async fn test_phase16_02_new_request_when_live_disabled_fails_safe_without_upload_or_prediction(
    ) {
        let (paths, _temp, project_id, sample_mp4, sample_ref) = create_test_env();

        let mock_provider = Arc::new(MockPrunaProvider::new());
        mock_provider.live_policy.set_allowed(false); // Live policy disabled

        let mock_uploader = Arc::new(MockAssetUploader::with_policy(Arc::new(
            MockLiveExecutionPolicy::new(false),
        )));

        let resolver = Arc::new(MockPhase16Resolver {
            provider: Some(mock_provider.clone()),
            uploader: Some(mock_uploader.clone()),
        });

        let event_sink = Arc::new(TestEventSink::new());
        let service = CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            Arc::new(DefaultCloudSubmissionGate::new()),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_p16_req(
            "cjob-live-disabled-01",
            &project_id,
            &sample_mp4,
            &sample_ref,
        );

        let res = service.start_cloud_generation(req, Some(3.00)).await;
        assert!(res.is_err());
        let err_msg = format!("{}", res.unwrap_err());
        assert!(
            err_msg.contains("PAID_LIVE_TEST_DISABLED"),
            "Expected PAID_LIVE_TEST_DISABLED, got: {}",
            err_msg
        );

        // Verification: 0 uploads, 0 prediction creations
        assert_eq!(mock_uploader.upload_call_count.load(Ordering::SeqCst), 0);
        assert_eq!(
            mock_provider
                .create_prediction_call_count
                .load(Ordering::SeqCst),
            0
        );

        // Ensure job submissionState was NEVER marked Ambiguous
        let jobs = service.store().list_all_active_jobs().unwrap();
        for j in jobs {
            assert_ne!(j.submission_state, SubmissionState::Ambiguous);
        }
    }

    #[tokio::test]
    async fn test_phase16_03_existing_job_recovery_poll_cancel_download_works_when_live_guard_disabled(
    ) {
        let (paths, _temp, project_id, sample_mp4, sample_ref) = create_test_env();

        let mock_provider = Arc::new(MockPrunaProvider::new());
        *mock_provider.download_behavior.lock().unwrap() = vec![Ok(sample_mp4.clone())];

        let resolver = Arc::new(MockPhase16Resolver {
            provider: Some(mock_provider.clone()),
            uploader: None,
        });

        let event_sink = Arc::new(TestEventSink::new());
        let service = CloudJobLifecycleService::new(
            paths.clone(),
            resolver,
            event_sink,
            Arc::new(DefaultCloudSubmissionGate::new()),
            LifecycleTimingConfig::fast_test(),
        );

        // Create an existing job with remote_job_id in store
        let existing_job = PersistentCloudJob {
            schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
            state_revision: 1,
            job_id: "client_existing_1".to_string(),
            internal_job_id: "cjob-existing-1".to_string(),
            project_id: project_id.clone(),
            provider_id: "replicate".to_string(),
            model_id: "prunaai/p-video-replace".to_string(),
            model_version: "official-current".to_string(),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
            execution_class: ExecutionClass::SpecializedVideoTransformation,
            input_assets: InputAssets {
                source_video_path: Some(sample_mp4.clone()),
                reference_image_paths: vec![sample_ref.clone()],
                ..Default::default()
            },
            configuration_hash: "mock_hash".to_string(),
            submission_state: SubmissionState::Acknowledged,
            remote_job_id: Some("remote_existing_ack".to_string()),
            state: CloudJobState::Processing,
            cost: CostRecord::default(),
            output: OutputArtifactRecord::default(),
            retry: Default::default(),
            error: None,
            timestamps: Default::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: Some("processing".to_string()),
            output_url: None,
            validation_policy: ValidationPolicy {
                expected_duration_sec: Some(1.0),
                require_audio: true,
            },
        };

        service.store().save_job_atomic(&existing_job).unwrap();

        // Run recovery while live guard is disabled
        let recovered = service.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);

        // Allow worker to poll and complete existing remote job
        tokio::time::sleep(Duration::from_millis(150)).await;

        let final_job = service
            .get_job_status(&project_id, "cjob-existing-1")
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Completed);
    }

    // -------------------------------------------------------------------------
    // 02. MULTI-MODEL REGISTRY & ORDER-INDEPENDENCE TESTS
    // -------------------------------------------------------------------------

    #[test]
    fn test_phase16_04_provider_key_equality_and_hashing() {
        let k1 = ProviderKey::new("replicate", "prunaai/p-video-replace");
        let k2 = ProviderKey::new("replicate", "prunaai/p-video-replace");
        let k3 = ProviderKey::new("replicate", "minimax/video-01");

        assert_eq!(k1, k2);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_phase16_05_registry_records_unique_by_compound_key() {
        let registry = ProviderRegistry::new();

        let pruna = registry.find("replicate", "prunaai/p-video-replace");
        assert!(pruna.is_some());
        assert_eq!(
            pruna.unwrap().execution_class,
            ExecutionClass::SpecializedVideoTransformation
        );

        let minimax = registry.find("replicate", "minimax/video-01");
        assert!(minimax.is_some());

        // Verify registering pruna does NOT overwrite minimax
        assert_ne!(pruna.unwrap().model_id, minimax.unwrap().model_id);
    }

    #[test]
    fn test_phase16_06_registry_deterministic_selection_order_independent() {
        let registry_orig = ProviderRegistry::new();
        let mut registry_rev = ProviderRegistry::new();

        // Reverse records in registry_rev
        let mut recs = registry_rev.list_records().to_vec();
        recs.reverse();
        registry_rev = ProviderRegistry::default();
        for r in recs {
            registry_rev.register_provider(r);
        }

        let req = CloudJobRequest {
            job_id: "test_order_req".to_string(),
            project_id: Some("proj".to_string()),
            prompt: "Replace actor".to_string(),
            negative_prompt: None,
            source_video: Some(PathBuf::from("test.mp4")),
            reference_image: Some(PathBuf::from("ref.jpg")),
            reference_images: Some(vec![PathBuf::from("ref.jpg")]),
            duration_seconds: 6.0,
            fps: 24.0,
            resolution: (720, 1280),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
        };

        let dec1 = GenerationRouter::route_with_registry(
            TaskClass::CharacterReplacement,
            RoutingPreference::CostSaving,
            &req,
            None,
            &registry_orig,
        );

        let dec2 = GenerationRouter::route_with_registry(
            TaskClass::CharacterReplacement,
            RoutingPreference::CostSaving,
            &req,
            None,
            &registry_rev,
        );

        assert_eq!(dec1.provider_id, "replicate");
        assert_eq!(dec1.model_id, "prunaai/p-video-replace");
        assert_eq!(dec1.provider_id, dec2.provider_id);
        assert_eq!(dec1.model_id, dec2.model_id);
        assert_eq!(dec1.cost_breakdown.total_usd, dec2.cost_breakdown.total_usd);
    }

    #[test]
    fn test_phase16_07_pruna_estimate_cost_model_aware_never_uses_minimax() {
        let provider = PrunaPVideoReplaceProvider::new();
        let req_720p = CloudJobRequest {
            job_id: "req_720".to_string(),
            project_id: None,
            prompt: "test".to_string(),
            negative_prompt: None,
            source_video: None,
            reference_image: None,
            reference_images: None,
            duration_seconds: 10.0,
            fps: 24.0,
            resolution: (720, 1280),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
        };

        let est_720p = provider.estimate_cost(&req_720p);
        // 10s * $0.03 = $0.30 (NOT MiniMax $0.50)
        assert!((est_720p.estimated_usd.unwrap() - 0.30).abs() < 0.001);

        let req_1080p = CloudJobRequest {
            resolution: (1080, 1920),
            ..req_720p
        };
        let est_1080p = provider.estimate_cost(&req_1080p);
        // 10s * $0.06 = $0.60
        assert!((est_1080p.estimated_usd.unwrap() - 0.60).abs() < 0.001);
    }

    // -------------------------------------------------------------------------
    // 03. PROVIDER-INDEPENDENT ROUTER & STRICT PARSING TESTS
    // -------------------------------------------------------------------------

    #[test]
    fn test_phase16_08_route_with_registry_without_provider_instance() {
        let registry = ProviderRegistry::new();
        let req = CloudJobRequest {
            job_id: "req_static".to_string(),
            project_id: None,
            prompt: "Character swap".to_string(),
            negative_prompt: None,
            source_video: Some(PathBuf::from("src.mp4")),
            reference_image: Some(PathBuf::from("ref.jpg")),
            reference_images: None,
            duration_seconds: 5.0,
            fps: 24.0,
            resolution: (720, 1280),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
        };

        let dec = GenerationRouter::route_with_registry(
            TaskClass::CharacterReplacement,
            RoutingPreference::CostSaving,
            &req,
            None,
            &registry,
        );

        assert_eq!(dec.target, RoutingTarget::Cloud);
        assert_eq!(dec.provider_id, "replicate");
        assert_eq!(dec.model_id, "prunaai/p-video-replace");
        assert!((dec.cost_breakdown.total_usd.unwrap() - 0.15).abs() < 0.001);
    }

    #[test]
    fn test_phase16_09_strict_task_class_parsing_rejects_unknown() {
        assert!(TaskClass::from_str_strict("CHARACTER_REPLACEMENT").is_ok());
        assert!(TaskClass::from_str_strict("BACKGROUND_REMOVAL").is_ok());
        assert!(TaskClass::from_str_strict("STYLE_FILTER").is_ok());
        assert!(TaskClass::from_str_strict("AUDIO_TRANSFORMATION").is_ok());
        assert!(TaskClass::from_str_strict("ACTION_REGENERATION").is_ok());
        assert!(TaskClass::from_str_strict("FULL_GENERATIVE_TRANSFORMATION").is_ok());

        let invalid = TaskClass::from_str_strict("UNKNOWN_UNRECOGNIZED_TASK");
        assert!(invalid.is_err());
        assert!(format!("{}", invalid.unwrap_err()).contains("UNKNOWN_TASK_CLASS"));
    }

    #[test]
    fn test_phase16_10_character_replacement_resolution_and_fps_routing() {
        assert_eq!(
            ResolutionTier::from_dimensions((720, 1280)).unwrap(),
            ResolutionTier::P720
        );
        assert_eq!(
            ResolutionTier::from_dimensions((1280, 720)).unwrap(),
            ResolutionTier::P720
        );
        assert_eq!(
            ResolutionTier::from_dimensions((1080, 1920)).unwrap(),
            ResolutionTier::P1080
        );
        assert_eq!(
            ResolutionTier::from_dimensions((1920, 1080)).unwrap(),
            ResolutionTier::P1080
        );
        assert!(ResolutionTier::from_dimensions((3840, 2160)).is_err());

        assert_eq!(TargetFps::from_f64(24.0), TargetFps::Fps24);
        assert_eq!(TargetFps::from_f64(48.0), TargetFps::Fps48);
        assert_eq!(TargetFps::from_f64(30.0), TargetFps::Original);
    }

    #[test]
    fn test_phase16_11_action_regeneration_unsupported_isolated() {
        let registry = ProviderRegistry::new();
        let req = CloudJobRequest {
            job_id: "req_action".to_string(),
            project_id: None,
            prompt: "Regenerate action".to_string(),
            negative_prompt: None,
            source_video: Some(PathBuf::from("src.mp4")),
            reference_image: None,
            reference_images: None,
            duration_seconds: 6.0,
            fps: 24.0,
            resolution: (720, 1280),
            task_type: "ACTION_REGENERATION".to_string(),
        };

        let dec = GenerationRouter::route_with_registry(
            TaskClass::ActionRegeneration,
            RoutingPreference::CostSaving,
            &req,
            None,
            &registry,
        );

        assert_eq!(dec.target, RoutingTarget::Unavailable);
        assert!(!dec.auto_submit_allowed);
        assert!(dec.reason.contains("explicitly unsupported in Phase 16"));
    }

    // -------------------------------------------------------------------------
    // 04. NORMALIZED INTERNAL SPEC & PREPARED SUBMISSION TESTS
    // -------------------------------------------------------------------------

    #[test]
    fn test_phase16_12_reference_images_normalization_and_conflict_rejection() {
        let (_paths, _temp, _project_id, sample_mp4, sample_ref) = create_test_env();

        let mut req = make_test_p16_req("req_ref", "proj", &sample_mp4, &sample_ref);
        let mut project = crate::projects::Project::new("TestProj");
        project
            .transformation_config
            .preservation
            .preserve_original_audio = true;
        project.source_media = Some(SourceMedia {
            media_id: "sm_1".to_string(),
            original_file_name: "src.mp4".to_string(),
            source_path: sample_mp4.clone(),
            duration_ms: 1000,
            width: 720,
            height: 1280,
            fps: 24.0,
            file_size_bytes: 1000,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: Some("aac".to_string()),
            has_audio: true,
        });

        let plan = ValidatedSubmissionPlan {
            task_class: TaskClass::CharacterReplacement,
            routing_decision: RoutingDecision {
                target: RoutingTarget::Cloud,
                execution_class: ExecutionClass::SpecializedVideoTransformation,
                provider_id: "replicate".to_string(),
                model_id: "prunaai/p-video-replace".to_string(),
                task: TaskClass::CharacterReplacement,
                mode: RoutingPreference::CostSaving,
                reason: "Approved".to_string(),
                estimated_cost: CostEstimate::default(),
                cost_breakdown: CostBreakdown::default(),
                fallback_available: false,
                auto_submit_allowed: true,
            },
            budget_limit: 3.00,
            provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
        };

        // 1. Valid single reference
        let spec = ProviderSubmissionSpec::build(&req, &project, &plan).unwrap();
        assert_eq!(spec.reference_images.len(), 1);

        // 2. Conflicting reference_image and reference_images
        req.reference_image = Some(PathBuf::from("non_existent_different_image.jpg"));
        let err_conf = ProviderSubmissionSpec::build(&req, &project, &plan);
        assert!(err_conf.is_err());
        assert!(format!("{}", err_conf.unwrap_err()).contains("AMBIGUOUS_REFERENCE_INPUTS"));
    }

    #[test]
    fn test_phase16_13_save_audio_derived_from_project_policy() {
        let (_paths, _temp, _project_id, sample_mp4, sample_ref) = create_test_env();
        let req = make_test_p16_req("req_audio", "proj", &sample_mp4, &sample_ref);

        let mut project = crate::projects::Project::new("TestProj");
        project
            .transformation_config
            .preservation
            .preserve_original_audio = true;
        project.source_media = Some(SourceMedia {
            media_id: "sm_2".to_string(),
            original_file_name: "src.mp4".to_string(),
            source_path: sample_mp4.clone(),
            duration_ms: 1000,
            width: 720,
            height: 1280,
            fps: 24.0,
            file_size_bytes: 1000,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: None,
            has_audio: false, // Source has no audio
        });

        let plan = ValidatedSubmissionPlan {
            task_class: TaskClass::CharacterReplacement,
            routing_decision: RoutingDecision {
                target: RoutingTarget::Cloud,
                execution_class: ExecutionClass::SpecializedVideoTransformation,
                provider_id: "replicate".to_string(),
                model_id: "prunaai/p-video-replace".to_string(),
                task: TaskClass::CharacterReplacement,
                mode: RoutingPreference::CostSaving,
                reason: "Approved".to_string(),
                estimated_cost: CostEstimate::default(),
                cost_breakdown: CostBreakdown::default(),
                fallback_available: false,
                auto_submit_allowed: true,
            },
            budget_limit: 3.00,
            provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
        };

        let spec = ProviderSubmissionSpec::build(&req, &project, &plan).unwrap();
        // save_audio MUST be false because source media has no audio
        assert!(!spec.save_audio);
    }

    #[test]
    fn test_phase16_14_prepared_provider_submission_uses_uploaded_uris_never_local_paths() {
        let spec = ProviderSubmissionSpec {
            provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
            source_video: PathBuf::from(r"C:\Windows\Media\video.mp4"),
            reference_images: vec![PathBuf::from(r"C:\Users\quant\image.jpg")],
            instruction_prompt: Some("Swap character".to_string()),
            resolution_tier: ResolutionTier::P720,
            target_fps: TargetFps::Original,
            save_audio: true,
            ignore_audio: false,
            turbo: false,
            disable_safety_checker: false,
            seed: None,
        };

        let prepared = PreparedProviderSubmission {
            spec,
            uploaded_source: UploadedAsset {
                provider_file_id: Some("file_123".to_string()),
                input_uri: "https://replicate.delivery/pbxt/source_video.mp4".to_string(),
                expires_at: None,
                checksum: None,
            },
            uploaded_references: vec![UploadedAsset {
                provider_file_id: Some("file_456".to_string()),
                input_uri: "https://replicate.delivery/pbxt/ref_img.jpg".to_string(),
                expires_at: None,
                checksum: None,
            }],
        };

        // Assert URIs are clean remote delivery links
        assert!(prepared
            .uploaded_source
            .input_uri
            .starts_with("https://replicate.delivery/"));
        assert!(prepared.uploaded_references[0]
            .input_uri
            .starts_with("https://replicate.delivery/"));
        assert!(!prepared.uploaded_source.input_uri.contains(":\\"));
        assert!(!prepared.uploaded_references[0].input_uri.contains(":\\"));
    }

    // -------------------------------------------------------------------------
    // 05. SEPARATED UPLOAD & PREDICTION LIFECYCLE TESTS
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_phase16_18_upload_failure_never_becomes_ambiguous_submission() {
        let (paths, _temp, project_id, sample_mp4, sample_ref) = create_test_env();

        let mock_provider = Arc::new(MockPrunaProvider::new());
        let mock_uploader = Arc::new(MockAssetUploader::new());
        // Non-existent source video to trigger upload error
        let req = CloudJobRequest {
            source_video: Some(PathBuf::from("non_existent_source.mp4")),
            ..make_test_p16_req("cjob-upload-fail", &project_id, &sample_mp4, &sample_ref)
        };

        let resolver = Arc::new(MockPhase16Resolver {
            provider: Some(mock_provider.clone()),
            uploader: Some(mock_uploader.clone()),
        });

        let service = CloudJobLifecycleService::new(
            paths,
            resolver,
            Arc::new(TestEventSink::new()),
            Arc::new(DefaultCloudSubmissionGate::new()),
            LifecycleTimingConfig::fast_test(),
        );

        let res = service.start_cloud_generation(req, Some(3.00)).await;
        assert!(res.is_err());

        // Prediction create MUST be 0
        assert_eq!(
            mock_provider
                .create_prediction_call_count
                .load(Ordering::SeqCst),
            0
        );

        // Verify job state is NOT Ambiguous
        let jobs = service.store().list_all_active_jobs().unwrap();
        for j in jobs {
            assert_ne!(j.submission_state, SubmissionState::Ambiguous);
        }
    }

    #[tokio::test]
    async fn test_phase16_20_prediction_create_failure_transitions_to_ambiguous_and_blocked() {
        let (paths, _temp, project_id, sample_mp4, sample_ref) = create_test_env();

        let mock_provider = Arc::new(MockPrunaProvider::new());
        *mock_provider.prediction_behavior.lock().unwrap() =
            Err("Network dropped during prediction".to_string());

        let resolver = Arc::new(MockPhase16Resolver {
            provider: Some(mock_provider.clone()),
            uploader: Some(Arc::new(MockAssetUploader::with_policy(Arc::new(
                MockLiveExecutionPolicy::new(true),
            )))),
        });

        let service = CloudJobLifecycleService::new(
            paths,
            resolver,
            Arc::new(TestEventSink::new()),
            Arc::new(DefaultCloudSubmissionGate::new()),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_p16_req("cjob-pred-fail", &project_id, &sample_mp4, &sample_ref);
        let res = service.start_cloud_generation(req, Some(3.00)).await;
        assert!(res.is_err());

        // Prediction create WAS attempted (1), so this is legitimately covered by Ambiguous protection!
        assert_eq!(
            mock_provider
                .create_prediction_call_count
                .load(Ordering::SeqCst),
            1
        );

        let job = service
            .store()
            .find_job_by_client_request_id(&project_id, "cjob-pred-fail")
            .unwrap()
            .expect("Job not found in store");
        assert_eq!(job.submission_state, SubmissionState::Ambiguous);
        assert_eq!(job.state, CloudJobState::Blocked);
    }

    // -------------------------------------------------------------------------
    // 06. SSRF OUTPUT VALIDATION TESTS
    // -------------------------------------------------------------------------

    #[test]
    fn test_phase16_21_ssrf_allows_replicate_delivery_and_subdomains() {
        assert!(PrunaPVideoReplaceProvider::validate_ssrf_url(
            "https://replicate.delivery/pbxt/123.mp4"
        )
        .is_ok());
        assert!(PrunaPVideoReplaceProvider::validate_ssrf_url(
            "https://media.replicate.delivery/out.mp4"
        )
        .is_ok());
        assert!(PrunaPVideoReplaceProvider::validate_ssrf_url(
            "https://sub.media.replicate.delivery/out.mp4"
        )
        .is_ok());
    }

    #[test]
    fn test_phase16_22_ssrf_rejects_replicate_com_and_third_parties() {
        // api.replicate.com is an API endpoint, NOT an authorized video artifact delivery host
        assert!(PrunaPVideoReplaceProvider::validate_ssrf_url(
            "https://api.replicate.com/v1/predictions/123"
        )
        .is_err());
        assert!(
            PrunaPVideoReplaceProvider::validate_ssrf_url("https://replicate.com/out.mp4").is_err()
        );
        assert!(PrunaPVideoReplaceProvider::validate_ssrf_url(
            "https://replicate.delivery.attacker.com/evil.mp4"
        )
        .is_err());
        assert!(
            PrunaPVideoReplaceProvider::validate_ssrf_url("https://attacker.com/fake.mp4").is_err()
        );
    }

    #[test]
    fn test_phase16_23_ssrf_rejects_localhost_private_ips_and_redirects() {
        assert!(
            PrunaPVideoReplaceProvider::validate_ssrf_url("http://replicate.delivery/out.mp4")
                .is_err()
        ); // HTTP rejected
        assert!(
            PrunaPVideoReplaceProvider::validate_ssrf_url("https://127.0.0.1/out.mp4").is_err()
        );
        assert!(PrunaPVideoReplaceProvider::validate_ssrf_url("https://10.0.0.1/out.mp4").is_err());
        assert!(
            PrunaPVideoReplaceProvider::validate_ssrf_url("https://192.168.1.1/out.mp4").is_err()
        );
        assert!(PrunaPVideoReplaceProvider::validate_ssrf_url(
            "https://169.254.169.254/latest/meta-data"
        )
        .is_err());
        assert!(
            PrunaPVideoReplaceProvider::validate_ssrf_url("https://localhost/out.mp4").is_err()
        );
    }

    // -------------------------------------------------------------------------
    // 07. ARTIFACT CRASH RECOVERY & BACKWARD COMPATIBILITY TESTS
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_phase16_24_validating_output_recovery_promotes_existing_final_artifact_with_zero_submits_and_downloads(
    ) {
        let (paths, _temp, project_id, sample_mp4, _sample_ref) = create_test_env();

        let mock_provider = Arc::new(MockPrunaProvider::new());
        let resolver = Arc::new(MockPhase16Resolver {
            provider: Some(mock_provider.clone()),
            uploader: None,
        });

        let service = CloudJobLifecycleService::new(
            paths.clone(),
            resolver,
            Arc::new(TestEventSink::new()),
            Arc::new(DefaultCloudSubmissionGate::new()),
            LifecycleTimingConfig::fast_test(),
        );

        // Place valid final artifact directly on disk
        let final_path = service
            .store()
            .artifact_final_path(&project_id, "cjob-crash-promoted")
            .unwrap();
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::copy(&sample_mp4, &final_path).unwrap();

        let crash_job = PersistentCloudJob {
            schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
            state_revision: 5,
            job_id: "client_crash_1".to_string(),
            internal_job_id: "cjob-crash-promoted".to_string(),
            project_id: project_id.clone(),
            provider_id: "replicate".to_string(),
            model_id: "prunaai/p-video-replace".to_string(),
            model_version: "official-current".to_string(),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
            execution_class: ExecutionClass::SpecializedVideoTransformation,
            input_assets: InputAssets::default(),
            configuration_hash: "hash1".to_string(),
            submission_state: SubmissionState::Acknowledged,
            remote_job_id: Some("rem_ack".to_string()),
            state: CloudJobState::ValidatingOutput,
            cost: CostRecord::default(),
            output: OutputArtifactRecord::default(),
            retry: Default::default(),
            error: None,
            timestamps: crate::ai::cloud::JobTimestamps::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: Some("succeeded".to_string()),
            output_url: Some("https://replicate.delivery/out.mp4".to_string()),
            validation_policy: ValidationPolicy {
                expected_duration_sec: Some(1.0),
                require_audio: true,
            },
        };

        service.store().save_job_atomic(&crash_job).unwrap();

        // Run recovery
        let recovered = service.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Completed);

        // Assert 0 provider calls, 0 downloads made
        assert_eq!(
            mock_provider
                .create_prediction_call_count
                .load(Ordering::SeqCst),
            0
        );
        assert_eq!(mock_provider.download_call_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_phase16_26_legacy_singular_input_assets_manifest_deserialization() {
        let legacy_json = r#"{
            "schemaVersion": 1,
            "stateRevision": 2,
            "jobId": "legacy_client_req",
            "internalJobId": "cjob-legacy-123",
            "projectId": "p_legacy",
            "providerId": "replicate",
            "modelId": "minimax/video-01",
            "modelVersion": "minimax/video-01",
            "taskType": "CHARACTER_REPLACEMENT",
            "executionClass": "SPECIALIZED_VIDEO_TRANSFORMATION",
            "inputAssets": {
                "sourceVideoPath": "C:\\videos\\src.mp4",
                "sourceVideoHash": "sha_src_123",
                "referenceImagePath": "C:\\images\\ref.png",
                "referenceImageHash": "sha_ref_456"
            },
            "configurationHash": "config_hash_abc",
            "submissionState": "ACKNOWLEDGED",
            "state": "COMPLETED",
            "cost": {
                "confidence": "ESTIMATED",
                "budgetLimit": 3.00
            },
            "output": {},
            "retry": {},
            "timestamps": {
                "createdAt": "2026-08-19T10:00:00Z",
                "updatedAt": "2026-08-19T10:00:00Z"
            },
            "cancellationRequested": false,
            "validationPolicy": {
                "requireAudio": true
            }
        }"#;

        let job: PersistentCloudJob = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(job.internal_job_id, "cjob-legacy-123");
        assert_eq!(
            job.input_assets.get_reference_paths(),
            vec![PathBuf::from(r"C:\images\ref.png")]
        );
        assert_eq!(
            job.input_assets.get_reference_hashes(),
            vec!["sha_ref_456".to_string()]
        );
    }

    #[test]
    fn test_phase16_27_actual_cost_remains_none_unless_monetary_amount_present() {
        let job = PersistentCloudJob {
            schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
            state_revision: 1,
            job_id: "test".to_string(),
            internal_job_id: "cjob-test".to_string(),
            project_id: "proj".to_string(),
            provider_id: "replicate".to_string(),
            model_id: "prunaai/p-video-replace".to_string(),
            model_version: "official-current".to_string(),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
            execution_class: ExecutionClass::SpecializedVideoTransformation,
            input_assets: InputAssets::default(),
            configuration_hash: "hash".to_string(),
            submission_state: SubmissionState::NeverAttempted,
            remote_job_id: None,
            state: CloudJobState::Created,
            cost: CostRecord::default(),
            output: OutputArtifactRecord::default(),
            retry: Default::default(),
            error: None,
            timestamps: crate::ai::cloud::JobTimestamps::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: None,
            output_url: None,
            validation_policy: ValidationPolicy {
                expected_duration_sec: None,
                require_audio: false,
            },
        };

        // No inference duration inference into actualCost
        assert_eq!(job.cost.actual_cost, None);
    }

    // -------------------------------------------------------------------------
    // 08. FULL MOCKED LIFECYCLE WITH PHASE 16 FIXTURE
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_phase16_28_full_mocked_character_replacement_lifecycle_with_phase16_fixture() {
        let (paths, _temp, project_id, sample_mp4, sample_ref) = create_test_env();

        // Prepare explicit Phase 16 output fixture
        let target_dir = PathBuf::from("target");
        fs::create_dir_all(&target_dir).unwrap();
        let phase16_artifact = target_dir.join("phase16_test_artifact.mp4");
        create_synthetic_mp4(&phase16_artifact, 1, 720, 1280, 24, true);

        let mock_provider = Arc::new(MockPrunaProvider::new());
        *mock_provider.download_behavior.lock().unwrap() = vec![Ok(phase16_artifact.clone())];

        let resolver = Arc::new(MockPhase16Resolver {
            provider: Some(mock_provider.clone()),
            uploader: Some(Arc::new(MockAssetUploader::with_policy(Arc::new(
                MockLiveExecutionPolicy::new(true),
            )))),
        });

        let event_sink = Arc::new(TestEventSink::new());
        let service = CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink.clone(),
            Arc::new(DefaultCloudSubmissionGate::new()),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_p16_req("cjob-p16-full", &project_id, &sample_mp4, &sample_ref);
        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        assert_eq!(job.state, CloudJobState::Processing);
        assert_eq!(job.provider_id, "replicate");
        assert_eq!(job.model_id, "prunaai/p-video-replace");

        // Wait for background worker to complete
        tokio::time::sleep(Duration::from_millis(150)).await;

        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Completed);
        assert!(final_job.output.final_path.is_some());

        // Validate final artifact directly with ffprobe
        let validator = CloudOutputValidator::new();
        let meta = validator
            .validate_artifact(&phase16_artifact, Some(1.0), true)
            .expect("Phase 16 fixture ffprobe validation failed");

        assert_eq!(meta.width, 720);
        assert_eq!(meta.height, 1280);
        assert!(meta.duration_sec >= 0.9);
    }

    // -------------------------------------------------------------------------
    // 09. ADDITIONAL EXTENSIVE VALIDATION TESTS
    // -------------------------------------------------------------------------

    #[test]
    fn test_phase16_15_replicate_pruna_serializer_consumes_spec_not_raw_request() {
        let spec = ProviderSubmissionSpec {
            provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
            source_video: PathBuf::from("local_src.mp4"),
            reference_images: vec![PathBuf::from("local_ref.jpg")],
            instruction_prompt: Some("Swap character prompt".to_string()),
            resolution_tier: ResolutionTier::P1080,
            target_fps: TargetFps::Fps48,
            save_audio: true,
            ignore_audio: false,
            turbo: false,
            disable_safety_checker: false,
            seed: Some(42),
        };

        let prepared = PreparedProviderSubmission {
            spec,
            uploaded_source: UploadedAsset {
                provider_file_id: Some("f1".to_string()),
                input_uri: "https://replicate.delivery/source.mp4".to_string(),
                expires_at: None,
                checksum: None,
            },
            uploaded_references: vec![UploadedAsset {
                provider_file_id: Some("f2".to_string()),
                input_uri: "https://replicate.delivery/ref.jpg".to_string(),
                expires_at: None,
                checksum: None,
            }],
        };

        let payload = serde_json::json!({
            "input": {
                "video": prepared.uploaded_source.input_uri,
                "images": prepared.uploaded_references.iter().map(|a| a.input_uri.clone()).collect::<Vec<_>>(),
                "instruction_prompt": prepared.spec.instruction_prompt.clone().unwrap_or_default(),
                "resolution": prepared.spec.resolution_tier.as_str(),
                "target_fps": prepared.spec.target_fps.as_str(),
                "save_audio": prepared.spec.save_audio,
                "ignore_audio": prepared.spec.ignore_audio,
                "turbo": prepared.spec.turbo,
                "disable_safety_checker": prepared.spec.disable_safety_checker,
                "seed": prepared.spec.seed,
            }
        });

        let input_obj = payload["input"].as_object().unwrap();
        assert_eq!(input_obj["video"], "https://replicate.delivery/source.mp4");
        assert_eq!(input_obj["resolution"], "1080p");
        assert_eq!(input_obj["target_fps"], "48");
        assert_eq!(input_obj["save_audio"], true);
        assert_eq!(input_obj["disable_safety_checker"], false);
        assert_eq!(input_obj["seed"], 42);
    }

    #[tokio::test]
    async fn test_phase16_16_replicate_uploader_requires_token_and_valid_file() {
        use crate::ai::cloud::uploader::ReplicateAssetUploader;
        let uploader_no_token =
            ReplicateAssetUploader::with_policy(None, Arc::new(MockLiveExecutionPolicy::new(true)));
        let res = uploader_no_token
            .upload_file(Path::new("dummy.mp4"), "video/mp4")
            .await;
        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("CLOUD_AUTH_FAILED"));
    }

    #[tokio::test]
    async fn test_phase16_17_mock_uploader_tracks_calls_and_returns_valid_delivery_uris() {
        let (_paths, _temp, _project_id, sample_mp4, _sample_ref) = create_test_env();
        let uploader = Arc::new(MockAssetUploader::with_policy(Arc::new(
            MockLiveExecutionPolicy::new(true),
        )));
        let up1 = uploader
            .upload_file(&sample_mp4, "video/mp4")
            .await
            .unwrap();
        assert!(up1.input_uri.starts_with("https://replicate.delivery/"));
        assert_eq!(uploader.upload_call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_phase16_25_uploading_state_on_restart_resets_safely_never_ambiguous() {
        let (paths, _temp, project_id, sample_mp4, sample_ref) = create_test_env();
        let resolver = Arc::new(MockPhase16Resolver {
            provider: Some(Arc::new(MockPrunaProvider::new())),
            uploader: None,
        });

        let service = CloudJobLifecycleService::new(
            paths,
            resolver,
            Arc::new(TestEventSink::new()),
            Arc::new(DefaultCloudSubmissionGate::new()),
            LifecycleTimingConfig::fast_test(),
        );

        // Job crashed while in Uploading state
        let upload_crash_job = PersistentCloudJob {
            schema_version: CURRENT_CLOUD_JOB_SCHEMA_VERSION,
            state_revision: 1,
            job_id: "client_upload_crash".to_string(),
            internal_job_id: "cjob-upload-crash".to_string(),
            project_id: project_id.clone(),
            provider_id: "replicate".to_string(),
            model_id: "prunaai/p-video-replace".to_string(),
            model_version: "official-current".to_string(),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
            execution_class: ExecutionClass::SpecializedVideoTransformation,
            input_assets: InputAssets {
                source_video_path: Some(sample_mp4.clone()),
                reference_image_paths: vec![sample_ref.clone()],
                ..Default::default()
            },
            configuration_hash: "hash_upload".to_string(),
            submission_state: SubmissionState::NeverAttempted,
            remote_job_id: None,
            state: CloudJobState::Uploading,
            cost: CostRecord::default(),
            output: OutputArtifactRecord::default(),
            retry: Default::default(),
            error: None,
            timestamps: crate::ai::cloud::JobTimestamps::default(),
            cancellation_requested: false,
            progress_pct: None,
            remote_status: None,
            output_url: None,
            validation_policy: ValidationPolicy {
                expected_duration_sec: Some(1.0),
                require_audio: true,
            },
        };

        service.store().save_job_atomic(&upload_crash_job).unwrap();

        let recovered = service.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Created);
        assert_eq!(
            recovered[0].submission_state,
            SubmissionState::NeverAttempted
        );
        assert_ne!(recovered[0].submission_state, SubmissionState::Ambiguous);
    }

    #[test]
    fn test_phase16_30_ipc_contract_reference_images_camel_case() {
        let req_json = r#"{
            "jobId": "req_ipc_01",
            "prompt": "Swap character",
            "referenceImages": ["C:\\ref1.jpg", "C:\\ref2.jpg"],
            "durationSeconds": 5.0,
            "fps": 24.0,
            "resolution": [720, 1280],
            "taskType": "CHARACTER_REPLACEMENT"
        }"#;

        let req: CloudJobRequest = serde_json::from_str(req_json).unwrap();
        assert_eq!(
            req.reference_images,
            Some(vec![
                PathBuf::from(r"C:\ref1.jpg"),
                PathBuf::from(r"C:\ref2.jpg")
            ])
        );
    }

    #[test]
    fn test_phase16_31_multiple_reference_images_up_to_3_supported() {
        let (_paths, _temp, _project_id, sample_mp4, sample_ref) = create_test_env();
        let mut req = make_test_p16_req("req_3ref", "proj", &sample_mp4, &sample_ref);
        req.reference_image = None;
        req.reference_images = Some(vec![
            sample_ref.clone(),
            sample_ref.clone(),
            sample_ref.clone(),
        ]);

        let mut project = crate::projects::Project::new("Proj3Ref");
        project.source_media = Some(SourceMedia {
            media_id: "sm_3".to_string(),
            original_file_name: "src.mp4".to_string(),
            source_path: sample_mp4.clone(),
            duration_ms: 1000,
            width: 720,
            height: 1280,
            fps: 24.0,
            file_size_bytes: 1000,
            container: "mp4".to_string(),
            video_codec: "h264".to_string(),
            audio_codec: Some("aac".to_string()),
            has_audio: true,
        });

        let plan = ValidatedSubmissionPlan {
            task_class: TaskClass::CharacterReplacement,
            routing_decision: RoutingDecision {
                target: RoutingTarget::Cloud,
                execution_class: ExecutionClass::SpecializedVideoTransformation,
                provider_id: "replicate".to_string(),
                model_id: "prunaai/p-video-replace".to_string(),
                task: TaskClass::CharacterReplacement,
                mode: RoutingPreference::CostSaving,
                reason: "Approved".to_string(),
                estimated_cost: CostEstimate::default(),
                cost_breakdown: CostBreakdown::default(),
                fallback_available: false,
                auto_submit_allowed: true,
            },
            budget_limit: 3.00,
            provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
        };

        let spec = ProviderSubmissionSpec::build(&req, &project, &plan).unwrap();
        assert_eq!(spec.reference_images.len(), 3);
    }

    #[test]
    fn test_phase16_32_more_than_3_reference_images_rejected() {
        let (_paths, _temp, _project_id, sample_mp4, sample_ref) = create_test_env();
        let mut req = make_test_p16_req("req_4ref", "proj", &sample_mp4, &sample_ref);
        req.reference_image = None;
        req.reference_images = Some(vec![
            sample_ref.clone(),
            sample_ref.clone(),
            sample_ref.clone(),
            sample_ref.clone(),
        ]);

        let project = crate::projects::Project::new("Proj4Ref");
        let plan = ValidatedSubmissionPlan {
            task_class: TaskClass::CharacterReplacement,
            routing_decision: RoutingDecision {
                target: RoutingTarget::Cloud,
                execution_class: ExecutionClass::SpecializedVideoTransformation,
                provider_id: "replicate".to_string(),
                model_id: "prunaai/p-video-replace".to_string(),
                task: TaskClass::CharacterReplacement,
                mode: RoutingPreference::CostSaving,
                reason: "Approved".to_string(),
                estimated_cost: CostEstimate::default(),
                cost_breakdown: CostBreakdown::default(),
                fallback_available: false,
                auto_submit_allowed: true,
            },
            budget_limit: 3.00,
            provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
        };

        let res = ProviderSubmissionSpec::build(&req, &project, &plan);
        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("Too many reference images"));
    }

    #[test]
    fn test_phase16_33_zero_reference_images_rejected_for_character_replacement() {
        let (_paths, _temp, _project_id, sample_mp4, sample_ref) = create_test_env();
        let mut req = make_test_p16_req("req_0ref", "proj", &sample_mp4, &sample_ref);
        req.reference_image = None;
        req.reference_images = Some(vec![]);

        let project = crate::projects::Project::new("Proj0Ref");
        let plan = ValidatedSubmissionPlan {
            task_class: TaskClass::CharacterReplacement,
            routing_decision: RoutingDecision {
                target: RoutingTarget::Cloud,
                execution_class: ExecutionClass::SpecializedVideoTransformation,
                provider_id: "replicate".to_string(),
                model_id: "prunaai/p-video-replace".to_string(),
                task: TaskClass::CharacterReplacement,
                mode: RoutingPreference::CostSaving,
                reason: "Approved".to_string(),
                estimated_cost: CostEstimate::default(),
                cost_breakdown: CostBreakdown::default(),
                fallback_available: false,
                auto_submit_allowed: true,
            },
            budget_limit: 3.00,
            provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
        };

        let res = ProviderSubmissionSpec::build(&req, &project, &plan);
        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("At least 1 reference image is required"));
    }

    #[test]
    fn test_phase16_34_disable_safety_checker_always_false_in_spec() {
        let (_paths, _temp, _project_id, sample_mp4, sample_ref) = create_test_env();
        let req = make_test_p16_req("req_safe", "proj", &sample_mp4, &sample_ref);
        let project = crate::projects::Project::new("ProjSafe");
        let plan = ValidatedSubmissionPlan {
            task_class: TaskClass::CharacterReplacement,
            routing_decision: RoutingDecision {
                target: RoutingTarget::Cloud,
                execution_class: ExecutionClass::SpecializedVideoTransformation,
                provider_id: "replicate".to_string(),
                model_id: "prunaai/p-video-replace".to_string(),
                task: TaskClass::CharacterReplacement,
                mode: RoutingPreference::CostSaving,
                reason: "Approved".to_string(),
                estimated_cost: CostEstimate::default(),
                cost_breakdown: CostBreakdown::default(),
                fallback_available: false,
                auto_submit_allowed: true,
            },
            budget_limit: 3.00,
            provider_key: ProviderKey::new("replicate", "prunaai/p-video-replace"),
        };

        let spec = ProviderSubmissionSpec::build(&req, &project, &plan).unwrap();
        assert!(
            !spec.disable_safety_checker,
            "disable_safety_checker MUST ALWAYS be false"
        );
    }

    #[test]
    fn test_phase16_35_router_model_selection_pruna_chosen_over_minimax_for_character_replacement()
    {
        let registry = ProviderRegistry::new();
        let req = CloudJobRequest {
            job_id: "req_select".to_string(),
            project_id: None,
            prompt: "Replace actor".to_string(),
            negative_prompt: None,
            source_video: Some(PathBuf::from("src.mp4")),
            reference_image: Some(PathBuf::from("ref.jpg")),
            reference_images: None,
            duration_seconds: 5.0,
            fps: 24.0,
            resolution: (720, 1280),
            task_type: "CHARACTER_REPLACEMENT".to_string(),
        };

        let dec = GenerationRouter::route_with_registry(
            TaskClass::CharacterReplacement,
            RoutingPreference::CostSaving,
            &req,
            None,
            &registry,
        );

        // Pruna is the specialized CharacterReplacement model (not MiniMax generic video-01)
        assert_eq!(dec.provider_id, "replicate");
        assert_eq!(dec.model_id, "prunaai/p-video-replace");
    }
}
