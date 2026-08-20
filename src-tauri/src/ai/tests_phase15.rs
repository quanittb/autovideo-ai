#[cfg(test)]
mod tests {
    use crate::ai::cloud::cost::{CostBreakdown, CostConfidence};
    use crate::ai::cloud::error::CloudProviderError;
    use crate::ai::cloud::job::{
        CloudJobRequest, CloudJobState, CostRecord, InputAssets, PersistentCloudJob,
        SubmissionState,
    };
    use crate::ai::cloud::lifecycle::{
        CloudJobLifecycleService, EventSink, LifecycleTimingConfig, TestEventSink,
    };
    use crate::ai::cloud::provider::{
        CloudJobHandle, CloudVideoProvider, ProviderCapabilities, RemotePollResponse, RemoteStatus,
    };
    use crate::ai::cloud::registry::ProviderRegistry;
    use crate::ai::cloud::resolver::CloudProviderResolver;
    use crate::ai::cloud::router::{RoutingDecision, RoutingPreference, RoutingTarget, TaskClass};
    use crate::ai::cloud::store::PersistentCloudJobStore;
    use crate::ai::cloud::submission::{
        CloudSubmissionGate, DefaultCloudSubmissionGate, ValidatedSubmissionPlan,
    };
    use crate::ai::cloud::validator::CloudOutputValidator;
    use crate::ai::cloud::ExecutionClass;
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
    // Helper: Create Test Storage Paths & Initial Project
    // -------------------------------------------------------------------------

    fn create_test_env() -> (StoragePaths, tempfile::TempDir, String, PathBuf) {
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

        // Create synthetic 1s valid MP4 fixture (576x1024 @ 25fps with audio)
        let sample_mp4 = base.join("valid_sample.mp4");
        create_synthetic_mp4(&sample_mp4, 1, 576, 1024, 25, true);

        // Create test project with source media metadata
        let pm = ProjectManager::new(paths.clone());
        let mut project = pm.create_project("Phase 15 Test Project").unwrap();
        project.source_media = Some(SourceMedia {
            media_id: "sm_123".to_string(),
            original_file_name: "valid_sample.mp4".to_string(),
            source_path: sample_mp4.clone(),
            duration_ms: 1000,
            width: 576,
            height: 1024,
            fps: 25.0,
            file_size_bytes: 23000,
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

        (paths, temp, project.id, sample_mp4)
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

    fn make_test_req(job_id: &str, project_id: &str, sample_mp4: &Path) -> CloudJobRequest {
        CloudJobRequest {
            job_id: job_id.to_string(),
            project_id: Some(project_id.to_string()),
            prompt: "A cinematic transformation in dramatic lighting".to_string(),
            negative_prompt: Some("blurry, low quality".to_string()),
            source_video: Some(sample_mp4.to_path_buf()),
            reference_image: None,
            duration_seconds: 1.0,
            fps: 25.0,
            resolution: (576, 1024),
            task_type: "FullTransformation".to_string(),
        }
    }

    // -------------------------------------------------------------------------
    // Mock Cloud Submission Gate
    // -------------------------------------------------------------------------

    #[derive(Default, Clone)]
    pub struct MockSubmissionGate {
        pub fail_reason: Option<String>,
    }

    impl CloudSubmissionGate for MockSubmissionGate {
        fn validate_and_prepare(
            &self,
            _request: &CloudJobRequest,
            _max_cost: Option<f64>,
            _cloud_provider: &dyn CloudVideoProvider,
            _registry: &ProviderRegistry,
        ) -> Result<ValidatedSubmissionPlan, CloudProviderError> {
            if let Some(ref r) = self.fail_reason {
                return Err(CloudProviderError::RequestInvalid(r.clone()));
            }
            Ok(ValidatedSubmissionPlan {
                task_class: TaskClass::FullGenerativeTransformation,
                routing_decision: RoutingDecision {
                    target: RoutingTarget::Cloud,
                    execution_class: ExecutionClass::SpecializedVideoTransformation,
                    provider_id: "replicate".to_string(),
                    model_id: "minimax/video-01".to_string(),
                    task: TaskClass::FullGenerativeTransformation,
                    mode: RoutingPreference::CostSaving,
                    reason: "Mock approved plan for lifecycle testing".to_string(),
                    estimated_cost: crate::ai::cloud::CostEstimate {
                        provider: "replicate".to_string(),
                        model: "minimax/video-01".to_string(),
                        estimated_usd: Some(0.50),
                        min_usd: Some(0.50),
                        max_usd: Some(0.50),
                        confidence: 1.0,
                        currency: "USD".to_string(),
                        status: CostConfidence::Exact,
                        breakdown: "1 prediction".to_string(),
                    },
                    cost_breakdown: CostBreakdown::default(),
                    fallback_available: false,
                    auto_submit_allowed: true,
                },
                budget_limit: 3.00,
                provider_id: "replicate".to_string(),
            })
        }
    }

    fn create_test_service(
        paths: StoragePaths,
        resolver: Arc<dyn CloudProviderResolver>,
        event_sink: Arc<dyn EventSink>,
        timing: LifecycleTimingConfig,
    ) -> CloudJobLifecycleService {
        CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            Arc::new(MockSubmissionGate::default()),
            timing,
        )
    }

    // -------------------------------------------------------------------------
    // Mock Cloud Video Provider
    // -------------------------------------------------------------------------

    pub struct MockCloudProvider {
        pub submit_call_count: Arc<AtomicU32>,
        pub poll_call_count: Arc<AtomicU32>,
        pub cancel_call_count: Arc<AtomicU32>,
        pub download_call_count: Arc<AtomicU32>,
        pub submit_behavior: Mutex<Result<String, String>>,
        pub poll_responses: Mutex<Vec<RemotePollResponse>>,
        pub download_behavior: Mutex<Vec<Result<PathBuf, String>>>,
        pub cancel_behavior: Mutex<Result<(), String>>,
        pub submit_delay_ms: Option<u64>,
        pub download_delay_ms: Option<u64>,
    }

    impl MockCloudProvider {
        pub fn new() -> Self {
            Self {
                submit_call_count: Arc::new(AtomicU32::new(0)),
                poll_call_count: Arc::new(AtomicU32::new(0)),
                cancel_call_count: Arc::new(AtomicU32::new(0)),
                download_call_count: Arc::new(AtomicU32::new(0)),
                submit_behavior: Mutex::new(Ok("mock_remote_123".to_string())),
                poll_responses: Mutex::new(vec![RemotePollResponse {
                    remote_id: "mock_remote_123".to_string(),
                    status: RemoteStatus::Succeeded,
                    output_url: Some("https://mock.storage/video.mp4".to_string()),
                    error: None,
                }]),
                download_behavior: Mutex::new(Vec::new()),
                cancel_behavior: Mutex::new(Ok(())),
                submit_delay_ms: None,
                download_delay_ms: None,
            }
        }
    }

    impl CloudVideoProvider for MockCloudProvider {
        fn provider_id(&self) -> &str {
            "replicate"
        }

        fn provider_name(&self) -> &str {
            "Mock Provider"
        }

        fn is_configured(&self) -> bool {
            true
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                supports_text_to_video: true,
                supports_image_to_video: true,
                supports_video_to_video: true,
                supports_reference_image: true,
                supports_character_reference: true,
                supports_audio: true,
                max_duration_sec: 10.0,
                supported_resolutions: vec![(320, 240), (576, 1024), (1080, 1920)],
                estimated_cost_per_second: None,
            }
        }

        fn estimate_cost(&self, _req: &CloudJobRequest) -> crate::ai::cloud::CostEstimate {
            crate::ai::cloud::CostEstimate {
                provider: "replicate".to_string(),
                model: "minimax/video-01".to_string(),
                estimated_usd: Some(0.50),
                min_usd: Some(0.45),
                max_usd: Some(0.60),
                confidence: 0.85,
                currency: "USD".to_string(),
                status: CostConfidence::Estimated,
                breakdown: "1 prediction @ $0.50".to_string(),
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
            self.submit_call_count.fetch_add(1, Ordering::SeqCst);
            let behavior = self.submit_behavior.lock().unwrap().clone();
            let delay = self.submit_delay_ms;
            Box::pin(async move {
                if let Some(ms) = delay {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
                match behavior {
                    Ok(remote_id) => Ok(CloudJobHandle {
                        job_id: "test_job".to_string(),
                        remote_id,
                        provider_id: "replicate".to_string(),
                        model: "minimax/video-01".to_string(),
                    }),
                    Err(err) => Err(CloudProviderError::ProviderUnavailable(err)),
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
            let delay = self.download_delay_ms;
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
            Box::pin(async move {
                if let Some(ms) = delay {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
                outcome
            })
        }
    }

    pub struct MockProviderResolver {
        pub provider: Option<Arc<MockCloudProvider>>,
    }

    impl CloudProviderResolver for MockProviderResolver {
        fn resolve_provider(
            &self,
            _provider_id: &str,
        ) -> Result<Arc<dyn CloudVideoProvider>, CloudProviderError> {
            match &self.provider {
                Some(p) => Ok(p.clone()),
                None => Err(CloudProviderError::ProviderUnavailable(
                    "MISSING_PROVIDER_CREDENTIALS: Mock provider resolver has no active provider"
                        .to_string(),
                )),
            }
        }
    }

    // =========================================================================
    // TEST 1 — FULL LIFECYCLE SUCCESS
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_01_full_lifecycle_success() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.download_behavior.lock().unwrap() = vec![Ok(sample_mp4.clone())];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-01", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        assert_eq!(job.submission_state, SubmissionState::Acknowledged);
        assert_eq!(job.remote_job_id, Some("mock_remote_123".to_string()));

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Completed);
        assert!(final_job.output.final_path.is_some());
        assert!(final_job.output.final_path.as_ref().unwrap().exists());
        assert!(final_job.output.artifact_hash.is_some());

        let events = event_sink.events.read().unwrap();
        assert!(!events.is_empty());
        assert_eq!(events.last().unwrap().state, CloudJobState::Completed);
    }

    // =========================================================================
    // TEST 2 — RESTART RESTORES PROCESSING JOB
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_02_restart_restores_processing_job() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.poll_responses.lock().unwrap() = vec![RemotePollResponse {
            remote_id: "mock_remote_123".to_string(),
            status: RemoteStatus::Processing,
            output_url: None,
            error: None,
        }];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths.clone(),
            resolver.clone(),
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-02", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        assert_eq!(job.state, CloudJobState::Processing);
        let internal_id = job.internal_job_id.clone();

        drop(service);

        let service_v2 = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service_v2.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].internal_job_id, internal_id);
        assert_eq!(recovered[0].state, CloudJobState::Processing);
        assert_eq!(
            recovered[0].remote_job_id,
            Some("mock_remote_123".to_string())
        );

        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 3 — DEDUPE: SEQUENTIAL SAME CLIENT REQUEST ID
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_03_dedupe_sequential_same_client_request_id() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("frontend-request-123", &project_id, &sample_mp4);

        let _job1 = service
            .start_cloud_generation(req.clone(), Some(3.00))
            .await
            .unwrap();
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);

        // Second sequential call with same frontend request ID
        let res2 = service.start_cloud_generation(req, Some(3.00)).await;
        assert!(res2.is_err());
        assert!(format!("{}", res2.unwrap_err()).contains("DUPLICATE_SUBMISSION_PREVENTED"));

        // Only 1 provider submit occurred
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 4 — DEDUPE: CONCURRENT SAME CLIENT REQUEST ID
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_04_dedupe_concurrent_same_client_request_id() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = Arc::new(create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        ));

        let req1 = make_test_req("frontend-concurrent-999", &project_id, &sample_mp4);
        let req2 = req1.clone();

        let s1 = service.clone();
        let s2 = service.clone();

        let (res1, res2) = tokio::join!(
            tokio::spawn(async move { s1.start_cloud_generation(req1, Some(3.00)).await }),
            tokio::spawn(async move { s2.start_cloud_generation(req2, Some(3.00)).await })
        );

        let out1 = res1.unwrap();
        let out2 = res2.unwrap();

        let successes = [out1.is_ok(), out2.is_ok()].iter().filter(|&&b| b).count();
        assert_eq!(
            successes, 1,
            "Exactly one concurrent submission must succeed"
        );
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);

        let jobs = service.store().list_jobs_in_project(&project_id).unwrap();
        assert_eq!(
            jobs.len(),
            1,
            "Exactly one persistent internal job must exist"
        );
    }

    // =========================================================================
    // TEST 5 — DEDUPE: RESTART THEN RETRY SAME CLIENT REQUEST ID
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_05_dedupe_restart_then_retry_same_client_request_id() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.poll_responses.lock().unwrap() = vec![RemotePollResponse {
            remote_id: "mock_remote_restart".to_string(),
            status: RemoteStatus::Processing,
            output_url: None,
            error: None,
        }];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths.clone(),
            resolver.clone(),
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("frontend-restart-retry-456", &project_id, &sample_mp4);

        let job1 = service
            .start_cloud_generation(req.clone(), Some(3.00))
            .await
            .unwrap();
        let initial_internal_id = job1.internal_job_id.clone();
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);

        // Simulate app restart
        drop(service);
        let service_v2 = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let _ = service_v2.recover_startup_jobs().await.unwrap();

        // Retry same client request ID after restart
        let retry_res = service_v2.start_cloud_generation(req, Some(3.00)).await;
        assert!(retry_res.is_err());
        assert!(format!("{}", retry_res.unwrap_err()).contains("DUPLICATE_SUBMISSION_PREVENTED"));

        // Verify submit count unchanged and internal job preserved
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
        let loaded = service_v2
            .get_job_status(&project_id, "frontend-restart-retry-456")
            .unwrap();
        assert_eq!(loaded.internal_job_id, initial_internal_id);
    }

    // =========================================================================
    // TEST 6 — STALE POLL RESPONSE CANNOT OVERWRITE CANCELLED STATE
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_06_stale_poll_cannot_overwrite_cancelled() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        // Polling response returns Succeeded
        *mock_provider.poll_responses.lock().unwrap() = vec![RemotePollResponse {
            remote_id: "mock_remote_stale_poll".to_string(),
            status: RemoteStatus::Succeeded,
            output_url: Some("https://mock.storage/video.mp4".to_string()),
            error: None,
        }];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let mut timing = LifecycleTimingConfig::production();
        timing.poll_interval_ms = 5_000;

        let service = create_test_service(paths, resolver, event_sink, timing);

        let req = make_test_req("cjob-stale-poll", &project_id, &sample_mp4);
        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        // Cancel job while poller is in background
        let cancel_res = service
            .cancel_cloud_generation(&project_id, &job.internal_job_id)
            .await
            .unwrap();
        assert_eq!(cancel_res.state, CloudJobState::Cancelled);

        // Wait for background worker to settle
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Authoritative state must remain CANCELLED, not transitioned to Downloading/Completed
        let on_disk = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(on_disk.state, CloudJobState::Cancelled);
    }

    // =========================================================================
    // TEST 7 — STALE LOWER REVISION SAVE REJECTED BY STORE
    // =========================================================================

    #[test]
    fn test_phase15_07_stale_lower_revision_save_rejected() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths);

        let mut job = PersistentCloudJob::new(
            "client_req_cas".to_string(),
            "cjob-cas-test".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_cas".to_string(),
            CostRecord::default(),
        );
        job.state_revision = 12;
        job.state = CloudJobState::Cancelled;
        store.save_job_atomic(&job).unwrap();

        // Attempt to save stale lower revision (11)
        let mut stale_job = job.clone();
        stale_job.state_revision = 11;
        stale_job.state = CloudJobState::Processing;

        let res = store.save_job_atomic(&stale_job);
        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("STALE_JOB_REVISION"));

        // Verify disk primary remains revision 12 CANCELLED
        let primary = store.load_job(&project_id, "cjob-cas-test").unwrap();
        assert_eq!(primary.state_revision, 12);
        assert_eq!(primary.state, CloudJobState::Cancelled);
    }

    // =========================================================================
    // TEST 8 — SINGLE OWNER FOR REMOTE CANCELLATION
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_08_single_owner_remote_cancellation() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.poll_responses.lock().unwrap() = vec![RemotePollResponse {
            remote_id: "mock_remote_single_owner".to_string(),
            status: RemoteStatus::Processing,
            output_url: None,
            error: None,
        }];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let mut timing = LifecycleTimingConfig::production();
        timing.poll_interval_ms = 500;

        let service = create_test_service(paths, resolver, event_sink, timing);

        let req = make_test_req("cjob-single-cancel", &project_id, &sample_mp4);
        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        // Cancel command
        let cancel_start = std::time::Instant::now();
        let cancel_res = service
            .cancel_cloud_generation(&project_id, &job.internal_job_id)
            .await
            .unwrap();
        let cancel_elapsed = cancel_start.elapsed();

        assert!(cancel_elapsed < Duration::from_millis(500));
        assert_eq!(cancel_res.state, CloudJobState::Cancelled);

        // Wait for background worker to settle
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Exactly ONE remote cancel call must have been made
        assert_eq!(
            mock_provider.cancel_call_count.load(Ordering::SeqCst),
            1,
            "Remote cancel must have single ownership"
        );
        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Cancelled);
    }

    // =========================================================================
    // TEST 9 — CANCELLATION DURING IN-FLIGHT DOWNLOAD NEVER COMPLETED
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_09_cancellation_during_download_never_completed() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mut mock_provider = MockCloudProvider::new();
        mock_provider.download_delay_ms = Some(200); // 200ms download delay
        *mock_provider.download_behavior.lock().unwrap() = vec![Ok(sample_mp4.clone())];

        let mock_provider = Arc::new(mock_provider);
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-cancel-dl", &project_id, &sample_mp4);
        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        // Wait for poller to enter download phase
        tokio::time::sleep(Duration::from_millis(30)).await;

        // Cancel while download is in flight
        let _ = service
            .cancel_cloud_generation(&project_id, &job.internal_job_id)
            .await
            .unwrap();

        // Let download complete
        tokio::time::sleep(Duration::from_millis(300)).await;

        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(
            final_job.state,
            CloudJobState::Cancelled,
            "Job must remain CANCELLED and never become COMPLETED"
        );
        assert!(final_job.output.final_path.is_none());
    }

    // =========================================================================
    // TEST 10 — IN-FLIGHT SUBMISSION CANCELLATION RACE RESOLVED SAFELY
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_10_in_flight_submission_cancellation_race() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mut mock_provider = MockCloudProvider::new();
        mock_provider.submit_delay_ms = Some(150); // 150ms submit delay
        *mock_provider.submit_behavior.lock().unwrap() = Ok("mock_remote_delayed".to_string());

        let mock_provider = Arc::new(mock_provider);
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = Arc::new(create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        ));

        let req = make_test_req("cjob-submit-race", &project_id, &sample_mp4);

        let s1 = service.clone();
        let s2 = service.clone();
        let pid = project_id.clone();

        let (submit_res, _) = tokio::join!(
            tokio::spawn(async move { s1.start_cloud_generation(req, Some(3.00)).await }),
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(40)).await;
                // Cancel while submit_job is awaiting remote ID
                let _ = s2.cancel_cloud_generation(&pid, "cjob-submit-race").await;
            })
        );

        let job = submit_res.unwrap().unwrap();
        // The newly learned remoteJobId must be preserved
        assert_eq!(job.remote_job_id, Some("mock_remote_delayed".to_string()));
        // State must be safely Cancelled via reconciliation
        assert_eq!(job.state, CloudJobState::Cancelled);
        // Remote cancellation must have been invoked on that same remoteJobId
        assert_eq!(mock_provider.cancel_call_count.load(Ordering::SeqCst), 1);
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 11 — CORRUPT MANIFEST FAILS CLOSED WITH ZERO SUBMITS
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_11_corrupt_manifest_fails_closed_zero_submits() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths.clone());

        // Create corrupt files for an existing job
        let jobs_dir = store.project_cloud_jobs_dir(&project_id).unwrap();
        fs::create_dir_all(&jobs_dir).unwrap();
        let corrupt_primary = jobs_dir.join("cjob-corrupt.json");
        let corrupt_tmp = jobs_dir.join("cjob-corrupt.json.tmp");
        fs::write(&corrupt_primary, b"{ bad json content").unwrap();
        fs::write(&corrupt_tmp, b"{ bad json content").unwrap();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-retry-corrupt", &project_id, &sample_mp4);

        let res = service.start_cloud_generation(req, Some(3.00)).await;
        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("RECOVERY_FAILED"));

        // Must fail closed with ZERO provider submissions
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 0);
    }

    // =========================================================================
    // TEST 12 — DOWNLOAD RETRY BUDGET SURVIVES RESTART
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_12_download_retry_budget_survives_restart() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths.clone());

        // Seed a job that failed 2 download attempts out of 3 allowed
        let mut job = PersistentCloudJob::new(
            "client_req_dl_budget".to_string(),
            "cjob-dl-budget".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_dl_budget".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::Downloading;
        job.submission_state = SubmissionState::Acknowledged;
        job.remote_job_id = Some("rem_dl_123".to_string());
        job.output_url = Some("https://mock.storage/video.mp4".to_string());
        job.retry.download_attempts = 2; // 2 attempts already consumed
        store.save_job_atomic(&job).unwrap();

        // Download fails on 3rd attempt
        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.download_behavior.lock().unwrap() =
            vec![Err("Network timeout on 3rd attempt".to_string())];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let mut timing = LifecycleTimingConfig::fast_test();
        timing.max_download_attempts = 3;

        let service = create_test_service(paths, resolver, event_sink, timing);

        let _ = service.recover_startup_jobs().await.unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let final_job = service
            .get_job_status(&project_id, "cjob-dl-budget")
            .unwrap();
        // Only 1 download attempt allowed before hitting max_download_attempts (3)
        assert_eq!(final_job.state, CloudJobState::Failed);
        assert_eq!(final_job.error.as_ref().unwrap().code, "DOWNLOAD_FAILED");
        assert_eq!(
            mock_provider.download_call_count.load(Ordering::SeqCst),
            1,
            "Only the remaining budget attempt should execute"
        );
    }

    // =========================================================================
    // TEST 13 — RESUME_UNBLOCK_JOB DEADLOCK-FREE RECONCILIATION
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_13_resume_unblock_job_no_deadlock() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();

        let mut job = PersistentCloudJob::new(
            "client_req_resume_deadlock".to_string(),
            "cjob-deadlock-test".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_dl".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::Blocked;
        job.submission_state = SubmissionState::Acknowledged;
        job.remote_job_id = Some("rem_dl_123".to_string());
        job.cancellation_requested = true;

        let store = PersistentCloudJobStore::new(paths.clone());
        store.save_job_atomic(&job).unwrap();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let result = tokio::time::timeout(
            Duration::from_secs(2),
            service.resume_unblock_job(&project_id, "cjob-deadlock-test"),
        )
        .await
        .expect("Operation must not deadlock");

        let resumed_job = result.unwrap();
        assert_eq!(resumed_job.state, CloudJobState::Cancelled);
        assert_eq!(mock_provider.cancel_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 14 — STARTUP RECOVERY REMOTE CANCEL FAILURE BLOCKS
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_14_startup_recovery_cancel_failure_blocks() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();

        let mut job = PersistentCloudJob::new(
            "client_req_cancel_fail".to_string(),
            "cjob-cancel-fail".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_fail".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::Processing;
        job.submission_state = SubmissionState::Acknowledged;
        job.remote_job_id = Some("rem_fail_123".to_string());
        job.cancellation_requested = true;

        let store = PersistentCloudJobStore::new(paths.clone());
        store.save_job_atomic(&job).unwrap();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.cancel_behavior.lock().unwrap() =
            Err("HTTP 500 Internal Server Error during cancel".to_string());

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);

        assert_eq!(recovered[0].state, CloudJobState::Blocked);
        assert_eq!(
            recovered[0].error.as_ref().unwrap().code,
            "CANCELLATION_FAILED_REMOTE"
        );
        assert_eq!(recovered[0].remote_job_id, Some("rem_fail_123".to_string()));
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 0);
    }

    // =========================================================================
    // TEST 15 — PERSIST BEFORE EVENT WITH INJECTED STORE FAILURE
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_15_persist_before_event_with_injected_store_failure() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-fail-seam", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        let initial_events_count = event_sink.events.read().unwrap().len();
        assert!(initial_events_count > 0);

        service.store().set_fail_next_save(true);

        let cancel_res = service
            .cancel_cloud_generation(&project_id, &job.internal_job_id)
            .await;
        assert!(cancel_res.is_err());
        assert!(format!("{}", cancel_res.unwrap_err()).contains("SIMULATED_PERSISTENCE_FAILURE"));

        let post_failure_events_count = event_sink.events.read().unwrap().len();
        assert_eq!(initial_events_count, post_failure_events_count);

        let on_disk = service
            .store()
            .load_job(&project_id, &job.internal_job_id)
            .unwrap();
        assert_ne!(on_disk.state, CloudJobState::Cancelled);
        assert!(!on_disk.cancellation_requested);
    }

    // =========================================================================
    // TEST 16 — REAL PROJECT AUDIO POLICY DERIVATION
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_16_real_project_audio_policy_derivation() {
        let (paths, temp, _project_id, sample_mp4) = create_test_env();
        let pm = ProjectManager::new(paths.clone());

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider),
        });

        // Scenario A: source has audio + preserve_original_audio = true -> requireAudio = true
        {
            let mut p = pm.create_project("Project A").unwrap();
            p.source_media = Some(SourceMedia {
                media_id: "sm_a".to_string(),
                original_file_name: "a.mp4".to_string(),
                source_path: sample_mp4.clone(),
                duration_ms: 1000,
                width: 576,
                height: 1024,
                fps: 25.0,
                file_size_bytes: 20000,
                container: "mp4".to_string(),
                video_codec: "h264".to_string(),
                audio_codec: Some("aac".to_string()),
                has_audio: true,
            });
            p.transformation_config.preservation.preserve_original_audio = true;
            pm.update_project(&p).unwrap();

            let service = create_test_service(
                paths.clone(),
                resolver.clone(),
                Arc::new(TestEventSink::new()),
                LifecycleTimingConfig::fast_test(),
            );

            let req = make_test_req("cjob-policy-a", &p.id, &sample_mp4);
            let job = service
                .start_cloud_generation(req, Some(3.00))
                .await
                .unwrap();
            assert!(job.validation_policy.require_audio);
        }

        // Scenario B: source has audio + preserve_original_audio = false -> requireAudio = false
        {
            let mut p = pm.create_project("Project B").unwrap();
            p.source_media = Some(SourceMedia {
                media_id: "sm_b".to_string(),
                original_file_name: "b.mp4".to_string(),
                source_path: sample_mp4.clone(),
                duration_ms: 1000,
                width: 576,
                height: 1024,
                fps: 25.0,
                file_size_bytes: 20000,
                container: "mp4".to_string(),
                video_codec: "h264".to_string(),
                audio_codec: Some("aac".to_string()),
                has_audio: true,
            });
            p.transformation_config.preservation.preserve_original_audio = false;
            pm.update_project(&p).unwrap();

            let service = create_test_service(
                paths.clone(),
                resolver.clone(),
                Arc::new(TestEventSink::new()),
                LifecycleTimingConfig::fast_test(),
            );

            let req = make_test_req("cjob-policy-b", &p.id, &sample_mp4);
            let job = service
                .start_cloud_generation(req, Some(3.00))
                .await
                .unwrap();
            assert!(!job.validation_policy.require_audio);
        }

        // Scenario C: source has NO audio + preserve_original_audio = true -> requireAudio = false
        {
            let no_audio_mp4 = temp.path().join("no_audio_src.mp4");
            create_synthetic_mp4(&no_audio_mp4, 1, 576, 1024, 25, false);

            let mut p = pm.create_project("Project C").unwrap();
            p.source_media = Some(SourceMedia {
                media_id: "sm_c".to_string(),
                original_file_name: "c.mp4".to_string(),
                source_path: no_audio_mp4.clone(),
                duration_ms: 1000,
                width: 576,
                height: 1024,
                fps: 25.0,
                file_size_bytes: 20000,
                container: "mp4".to_string(),
                video_codec: "h264".to_string(),
                audio_codec: None,
                has_audio: false,
            });
            p.transformation_config.preservation.preserve_original_audio = true;
            pm.update_project(&p).unwrap();

            let service = create_test_service(
                paths,
                resolver,
                Arc::new(TestEventSink::new()),
                LifecycleTimingConfig::fast_test(),
            );

            let req = make_test_req("cjob-policy-c", &p.id, &no_audio_mp4);
            let job = service
                .start_cloud_generation(req, Some(3.00))
                .await
                .unwrap();
            assert!(!job.validation_policy.require_audio);
        }
    }

    // =========================================================================
    // TEST 17 — PRODUCTION COST-SAVING REGRESSION TEST
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_17_production_cost_saving_regression_blocks_full_transformation() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            Arc::new(DefaultCloudSubmissionGate::new()),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-cost-saving-check", &project_id, &sample_mp4);

        let result = service.start_cloud_generation(req, Some(3.00)).await;
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("ROUTING_UNAVAILABLE")
                || err_msg.contains("TASK_ROUTES_TO_LOCAL_EXECUTION"),
            "Expected cost saving block, got: {}",
            err_msg
        );

        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 0);
    }

    // =========================================================================
    // TEST 18 — OLD STATE BACKWARD COMPATIBILITY
    // =========================================================================

    #[test]
    fn test_phase15_18_old_state_backward_compatibility() {
        let old_json = r#"{
            "schemaVersion": 1,
            "jobId": "legacy_job_1",
            "internalJobId": "cjob-legacy-1",
            "projectId": "proj_legacy",
            "providerId": "replicate",
            "modelId": "minimax/video-01",
            "modelVersion": "minimax/video-01",
            "taskType": "CharacterReplacement",
            "executionClass": "SPECIALIZED_VIDEO_TRANSFORMATION",
            "inputAssets": {},
            "configurationHash": "legacy_hash",
            "state": "QUEUED",
            "cost": {
                "confidence": "EXACT",
                "budgetLimit": 3.0
            },
            "output": {},
            "timestamps": {
                "createdAt": "2026-08-16T10:00:00Z",
                "updatedAt": "2026-08-16T10:00:00Z"
            }
        }"#;

        let job: PersistentCloudJob =
            serde_json::from_str(old_json).expect("Legacy job JSON must deserialize cleanly");

        assert_eq!(job.internal_job_id, "cjob-legacy-1");
        assert_eq!(job.state, CloudJobState::Created);
        assert_eq!(job.submission_state, SubmissionState::NeverAttempted);
        assert_eq!(job.state_revision, 0);
        assert_eq!(job.cost.budget_limit, 3.0);
    }

    // =========================================================================
    // TEST 19 — EVENT CONTRACT SERIALIZATION
    // =========================================================================

    #[test]
    fn test_phase15_19_event_contract_serialization() {
        let mut job = PersistentCloudJob::new(
            "job_fe_1".to_string(),
            "cjob-fe-1".to_string(),
            "proj_123".to_string(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "StyleFilter".to_string(),
            ExecutionClass::LocalDeterministic,
            InputAssets::default(),
            "config_hash_abc".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::Processing;
        job.remote_job_id = Some("rem_xyz".to_string());
        job.progress_pct = Some(45.5);

        let payload = job.to_event_payload();
        let serialized = serde_json::to_value(&payload).unwrap();

        assert_eq!(
            serialized.get("jobId").unwrap().as_str().unwrap(),
            "job_fe_1"
        );
        assert_eq!(
            serialized.get("internalJobId").unwrap().as_str().unwrap(),
            "cjob-fe-1"
        );
        assert_eq!(
            serialized.get("projectId").unwrap().as_str().unwrap(),
            "proj_123"
        );
        assert_eq!(
            serialized.get("state").unwrap().as_str().unwrap(),
            "PROCESSING"
        );
        assert_eq!(
            serialized.get("remoteJobId").unwrap().as_str().unwrap(),
            "rem_xyz"
        );
        assert_eq!(
            serialized.get("progressPct").unwrap().as_f64().unwrap(),
            45.5
        );
    }

    // =========================================================================
    // TEST 20 — ILLEGAL TRANSITIONS REJECTED
    // =========================================================================

    #[test]
    fn test_phase15_20_illegal_transitions_rejected() {
        assert!(!CloudJobState::Completed.can_transition_to(CloudJobState::Processing));
        assert!(!CloudJobState::Cancelled.can_transition_to(CloudJobState::Submitted));
        assert!(!CloudJobState::Failed.can_transition_to(CloudJobState::Processing));
        assert!(!CloudJobState::Processing.can_transition_to(CloudJobState::Completed));
        assert!(!CloudJobState::Created.can_transition_to(CloudJobState::Completed));

        assert!(CloudJobState::Created.can_transition_to(CloudJobState::Validating));
        assert!(CloudJobState::Submitted.can_transition_to(CloudJobState::Processing));
        assert!(CloudJobState::Processing.can_transition_to(CloudJobState::Downloading));
        assert!(CloudJobState::Downloading.can_transition_to(CloudJobState::ValidatingOutput));
        assert!(CloudJobState::ValidatingOutput.can_transition_to(CloudJobState::Completed));
    }

    // =========================================================================
    // TEST 21 — SOURCE MEDIA IMMUTABILITY
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_21_source_media_immutability() {
        let (paths, temp, project_id, sample_mp4) = create_test_env();

        let initial_source_hash = CloudOutputValidator::compute_file_sha256(&sample_mp4).unwrap();

        let corrupt_mp4 = temp.path().join("corrupt_temp.mp4");
        {
            let mut f = File::create(&corrupt_mp4).unwrap();
            f.write_all(b"corrupt").unwrap();
        }

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.download_behavior.lock().unwrap() = vec![Ok(corrupt_mp4)];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-immutability", &project_id, &sample_mp4);

        let _ = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        assert!(sample_mp4.exists());
        let post_test_source_hash = CloudOutputValidator::compute_file_sha256(&sample_mp4).unwrap();
        assert_eq!(initial_source_hash, post_test_source_hash);
    }

    // =========================================================================
    // TEST 22 — POLLING TIMEOUT FAILS BOUNDED
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_22_polling_timeout_fails_bounded() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.poll_responses.lock().unwrap() = Vec::new();

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let mut timing = LifecycleTimingConfig::fast_test();
        timing.max_poll_duration_sec = 0;

        let service = create_test_service(paths, resolver, event_sink, timing);

        let req = make_test_req("cjob-test-timeout", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Failed);
        assert_eq!(final_job.error.as_ref().unwrap().code, "PROVIDER_TIMEOUT");
    }

    // =========================================================================
    // TEST 23 — DOWNLOAD RETRY BOUNDED
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_23_download_retry_bounded() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.download_behavior.lock().unwrap() = vec![
            Err("Transient network connection drop".to_string()),
            Ok(sample_mp4.clone()),
        ];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-retry-dl", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(250)).await;

        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Completed);
        assert_eq!(final_job.retry.download_attempts, 2);
    }

    // =========================================================================
    // TEST 24 — AMBIGUOUS SUBMISSION BLOCKS AUTO RESUBMISSION
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_24_ambiguous_submission_blocks_auto_resubmit() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.submit_behavior.lock().unwrap() =
            Err("Network connection reset during POST".to_string());

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths.clone(),
            resolver.clone(),
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-ambiguous", &project_id, &sample_mp4);

        let result = service
            .start_cloud_generation(req.clone(), Some(3.00))
            .await;
        assert!(result.is_err());

        let active = service.store().list_all_active_jobs().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].state, CloudJobState::Blocked);
        assert_eq!(active[0].submission_state, SubmissionState::Ambiguous);
        assert_eq!(
            active[0].error.as_ref().unwrap().code,
            "AMBIGUOUS_SUBMISSION"
        );

        drop(service);
        let service_v2 = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service_v2.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Blocked);
        assert_eq!(recovered[0].submission_state, SubmissionState::Ambiguous);

        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 25 — IN-FLIGHT SUBMISSION CANCELLATION PRESERVES SUBMITTED BEFORE ACK
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_25_in_flight_cancellation_preserves_submitted_state_before_ack() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mut mock_provider = MockCloudProvider::new();
        mock_provider.submit_delay_ms = Some(200); // 200ms submit delay
        *mock_provider.submit_behavior.lock().unwrap() = Ok("mock_remote_inflight_ack".to_string());

        let mock_provider = Arc::new(mock_provider);
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = Arc::new(create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        ));

        let req = make_test_req("cjob-inflight-ack", &project_id, &sample_mp4);

        let s1 = service.clone();
        let s2 = service.clone();
        let pid = project_id.clone();

        let (submit_res, _) = tokio::join!(
            tokio::spawn(async move { s1.start_cloud_generation(req, Some(3.00)).await }),
            tokio::spawn(async move {
                // Wait for submit_job to be in-flight
                tokio::time::sleep(Duration::from_millis(50)).await;

                // Cancel while submit_job is awaiting response
                let cancel_res = s2
                    .cancel_cloud_generation(&pid, "cjob-inflight-ack")
                    .await
                    .unwrap();

                // While submit_job is still in-flight without remoteJobId, state MUST NOT be CANCELLED!
                assert_eq!(cancel_res.state, CloudJobState::Submitted);
                assert!(cancel_res.cancellation_requested);
                assert_eq!(
                    cancel_res.remote_status,
                    Some("cancellation_pending_submission_ack".to_string())
                );
            })
        );

        let final_job = submit_res.unwrap().unwrap();
        // Once submit_job returns, remote ID is preserved and cancellation reconciled
        assert_eq!(
            final_job.remote_job_id,
            Some("mock_remote_inflight_ack".to_string())
        );
        assert_eq!(final_job.state, CloudJobState::Cancelled);
        assert_eq!(mock_provider.cancel_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 26 — CANCELLATION DURING VALIDATION NEVER PROMOTES ARTIFACT
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_26_cancellation_during_validation_never_promotes() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.download_behavior.lock().unwrap() = vec![Ok(sample_mp4.clone())];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-val-cancel", &project_id, &sample_mp4);
        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        // Cancel job while background task is processing
        let _ = service
            .cancel_cloud_generation(&project_id, &job.internal_job_id)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(250)).await;

        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Cancelled);

        // Final promoted artifact must NOT exist on disk
        let final_path = service
            .store()
            .artifact_final_path(&project_id, &job.internal_job_id)
            .unwrap();
        assert!(!final_path.exists());
    }

    // =========================================================================
    // TEST 27 — RETRY PERSISTENCE FAILURE PREVENTS DOWNLOAD ATTEMPT
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_27_retry_persistence_failure_prevents_download() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.download_behavior.lock().unwrap() = vec![Ok(sample_mp4.clone())];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-retry-persist-fail", &project_id, &sample_mp4);
        let _job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        // Arm fail_next_save right as poller transitions to download
        service.store().set_fail_next_save(true);

        tokio::time::sleep(Duration::from_millis(150)).await;

        // Worker must stop and NOT consume unrecorded download attempt
        assert_eq!(
            mock_provider.download_call_count.load(Ordering::SeqCst),
            0,
            "Download must not execute if retry counter persistence fails"
        );
    }

    // =========================================================================
    // TEST 28 — LIST_ALL_ACTIVE_JOBS FAILS CLOSED ON CORRUPT MANIFEST
    // =========================================================================

    #[test]
    fn test_phase15_28_list_all_active_jobs_fail_closed_on_corrupt_manifest() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths);

        let jobs_dir = store.project_cloud_jobs_dir(&project_id).unwrap();
        fs::create_dir_all(&jobs_dir).unwrap();

        // Write corrupt manifest in project
        let corrupt_primary = jobs_dir.join("cjob-broken.json");
        fs::write(&corrupt_primary, b"{ unparseable corrupt json").unwrap();

        let result = store.list_all_active_jobs();
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("RECOVERY_FAILED"));
    }

    // =========================================================================
    // TEST 29 — CONCURRENT CANCEL COMMANDS EXECUTE SINGLE REMOTE CANCEL
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_29_concurrent_cancel_commands_single_remote_cancel() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.poll_responses.lock().unwrap() = vec![RemotePollResponse {
            remote_id: "mock_remote_conc_cancel".to_string(),
            status: RemoteStatus::Processing,
            output_url: None,
            error: None,
        }];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = Arc::new(create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::production(),
        ));

        let req = make_test_req("cjob-conc-cancel", &project_id, &sample_mp4);
        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        let s1 = service.clone();
        let s2 = service.clone();
        let pid1 = project_id.clone();
        let pid2 = project_id.clone();
        let jid1 = job.internal_job_id.clone();
        let jid2 = job.internal_job_id.clone();

        let (c1, c2) = tokio::join!(
            tokio::spawn(async move { s1.cancel_cloud_generation(&pid1, &jid1).await }),
            tokio::spawn(async move { s2.cancel_cloud_generation(&pid2, &jid2).await })
        );

        let res1 = c1.unwrap().unwrap();
        let res2 = c2.unwrap().unwrap();

        assert_eq!(res1.state, CloudJobState::Cancelled);
        assert_eq!(res2.state, CloudJobState::Cancelled);

        tokio::time::sleep(Duration::from_millis(150)).await;

        // Exactly ONE remote cancel call must have been made
        assert_eq!(
            mock_provider.cancel_call_count.load(Ordering::SeqCst),
            1,
            "Concurrent cancel commands must not trigger duplicate remote cancel calls"
        );
    }

    // =========================================================================
    // TEST 30 — CAS HARDENING AGAINST NEWER VALID TMP
    // =========================================================================

    #[test]
    fn test_phase15_30_cas_hardening_against_newer_valid_tmp() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths);

        let mut job = PersistentCloudJob::new(
            "client_req_cas_tmp".to_string(),
            "cjob-cas-tmp".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_cas_tmp".to_string(),
            CostRecord::default(),
        );

        // Write primary at revision 10
        job.state_revision = 10;
        job.state = CloudJobState::Processing;
        store.save_job_atomic(&job).unwrap();

        // Write .tmp at revision 12 (crash recovery evidence)
        let tmp_path = store
            .job_tmp_file_path(&project_id, "cjob-cas-tmp")
            .unwrap();
        let mut tmp_job = job.clone();
        tmp_job.state_revision = 12;
        tmp_job.state = CloudJobState::Cancelled;
        let tmp_str = serde_json::to_string_pretty(&tmp_job).unwrap();
        fs::write(&tmp_path, tmp_str).unwrap();

        // Attempt to save incoming revision 11 (which is newer than primary 10, but older than tmp 12)
        let mut incoming_job = job.clone();
        incoming_job.state_revision = 11;
        incoming_job.state = CloudJobState::Downloading;

        let save_res = store.save_job_atomic(&incoming_job);
        assert!(save_res.is_err());
        assert!(format!("{}", save_res.unwrap_err()).contains("STALE_JOB_REVISION"));

        // Verify revision 12 remains authoritative on load
        let loaded = store.load_job(&project_id, "cjob-cas-tmp").unwrap();
        assert_eq!(loaded.state_revision, 12);
        assert_eq!(loaded.state, CloudJobState::Cancelled);
    }

    // =========================================================================
    // TEST 31 — ATOMIC TMP CRASH RECOVERY
    // =========================================================================

    #[test]
    fn test_phase15_31_atomic_tmp_crash_recovery() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths);

        let mut job = PersistentCloudJob::new(
            "client_req_crash".to_string(),
            "cjob-crash-rec".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_crash".to_string(),
            CostRecord::default(),
        );
        job.state_revision = 5;
        store.save_job_atomic(&job).unwrap();

        // Simulate crash: write newer tmp file (rev 6)
        let tmp_path = store
            .job_tmp_file_path(&project_id, "cjob-crash-rec")
            .unwrap();
        let mut newer_job = job.clone();
        newer_job.state_revision = 6;
        newer_job.state = CloudJobState::Processing;
        let tmp_str = serde_json::to_string_pretty(&newer_job).unwrap();
        fs::write(&tmp_path, tmp_str).unwrap();

        let loaded = store.load_job(&project_id, "cjob-crash-rec").unwrap();
        assert_eq!(loaded.state_revision, 6);
        assert_eq!(loaded.state, CloudJobState::Processing);
    }

    // =========================================================================
    // TEST 32 — PATH TRAVERSAL REJECTION
    // =========================================================================

    #[test]
    fn test_phase15_32_path_traversal_rejection() {
        let (paths, _temp, _project_id, _sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths);

        assert!(store.project_cloud_jobs_dir("../escape").is_err());
        assert!(store.project_cloud_jobs_dir("dir/escape").is_err());
        assert!(store.project_cloud_jobs_dir("dir\\escape").is_err());
        assert!(store.project_cloud_jobs_dir("").is_err());
        assert!(store.job_file_path("proj_1", "../escape").is_err());
        assert!(store.artifact_final_path("proj_1", "bad:id").is_err());
    }

    // =========================================================================
    // TEST 33 — MISSING PROVIDER CREDENTIALS RECOVERY
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_33_missing_provider_credentials_recovery() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();

        let mut job = PersistentCloudJob::new(
            "client_req_creds".to_string(),
            "cjob-creds".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_creds".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::Processing;
        job.remote_job_id = Some("rem_creds_1".to_string());

        let store = PersistentCloudJobStore::new(paths.clone());
        store.save_job_atomic(&job).unwrap();

        // Resolver with NO provider configured
        let resolver = Arc::new(MockProviderResolver { provider: None });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Blocked);
        assert_eq!(
            recovered[0].error.as_ref().unwrap().code,
            "MISSING_PROVIDER_CREDENTIALS"
        );
    }

    // =========================================================================
    // TEST 34 — VALIDATING_OUTPUT LOCAL RECOVERY WITHOUT CREDENTIALS
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_34_validating_output_local_recovery_without_credentials() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths.clone());

        let mut job = PersistentCloudJob::new(
            "client_req_val_rec".to_string(),
            "cjob-val-rec".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_val_rec".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::ValidatingOutput;
        store.save_job_atomic(&job).unwrap();

        // Copy valid sample MP4 into partial artifact location
        let partial_path = store
            .artifact_partial_path(&project_id, "cjob-val-rec")
            .unwrap();
        fs::copy(&sample_mp4, &partial_path).unwrap();

        // Resolver with NO provider credentials
        let resolver = Arc::new(MockProviderResolver { provider: None });
        let event_sink = Arc::new(TestEventSink::new());

        let service = create_test_service(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Completed);
        assert!(recovered[0].output.final_path.is_some());
    }

    // =========================================================================
    // TEST 35 — WRONG DURATION VALIDATION FAILURE
    // =========================================================================

    #[test]
    fn test_phase15_35_wrong_duration_validation_failure() {
        let (_paths, temp, _project_id, _sample_mp4) = create_test_env();
        let short_mp4 = temp.path().join("short.mp4");
        create_synthetic_mp4(&short_mp4, 1, 576, 1024, 25, true);

        let validator = CloudOutputValidator::new();
        // Request expected 10.0s, but artifact is only 1.0s
        let res = validator.validate_artifact(&short_mp4, Some(10.0), false);
        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("exceeds tolerance bounds"));
    }

    // =========================================================================
    // TEST 36 — REQUIRE_AUDIO VALIDATION FAILURE
    // =========================================================================

    #[test]
    fn test_phase15_36_require_audio_validation_failure() {
        let (_paths, temp, _project_id, _sample_mp4) = create_test_env();
        let no_audio_mp4 = temp.path().join("no_audio.mp4");
        create_synthetic_mp4(&no_audio_mp4, 1, 576, 1024, 25, false);

        let validator = CloudOutputValidator::new();
        let res = validator.validate_artifact(&no_audio_mp4, Some(1.0), true);
        assert!(res.is_err());
        assert!(format!("{}", res.unwrap_err()).contains("has no audio stream"));
    }

    // =========================================================================
    // TEST 37 — NO AUDIO REQUIRED VALIDATION SUCCESS
    // =========================================================================

    #[test]
    fn test_phase15_37_no_audio_required_validation_success() {
        let (_paths, temp, _project_id, _sample_mp4) = create_test_env();
        let no_audio_mp4 = temp.path().join("no_audio_ok.mp4");
        create_synthetic_mp4(&no_audio_mp4, 1, 576, 1024, 25, false);

        let validator = CloudOutputValidator::new();
        let res = validator.validate_artifact(&no_audio_mp4, Some(1.0), false);
        assert!(res.is_ok());
    }

    // =========================================================================
    // TEST 38 — CORRUPT OUTPUT FAILS VALIDATION
    // =========================================================================

    #[test]
    fn test_phase15_38_corrupt_output_fails_validation() {
        let (_paths, temp, _project_id, _sample_mp4) = create_test_env();
        let corrupt_mp4 = temp.path().join("corrupt.mp4");
        fs::write(&corrupt_mp4, b"not a real mp4 video file").unwrap();

        let validator = CloudOutputValidator::new();
        let res = validator.validate_artifact(&corrupt_mp4, Some(1.0), false);
        assert!(res.is_err());
    }
}
