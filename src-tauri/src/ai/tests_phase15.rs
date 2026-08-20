#[cfg(test)]
mod tests {
    use crate::ai::cloud::cost::{CostBreakdown, CostConfidence};
    use crate::ai::cloud::error::CloudProviderError;
    use crate::ai::cloud::job::{
        CloudJobRequest, CloudJobState, CostRecord, InputAssets, PersistentCloudJob,
        SubmissionState, ValidationPolicy,
    };
    use crate::ai::cloud::lifecycle::{
        CloudJobLifecycleService, EventSink, LifecycleTimingConfig, TestEventSink,
    };
    use crate::ai::cloud::provider::{
        CloudJobHandle, CloudVideoProvider, ProviderCapabilities, RemotePollResponse, RemoteStatus,
    };
    use crate::ai::cloud::registry::ProviderRegistry;
    use crate::ai::cloud::resolver::{CloudProviderResolver, DefaultCloudProviderResolver};
    use crate::ai::cloud::router::{RoutingDecision, RoutingPreference, RoutingTarget, TaskClass};
    use crate::ai::cloud::store::PersistentCloudJobStore;
    use crate::ai::cloud::submission::{
        CloudSubmissionGate, DefaultCloudSubmissionGate, ValidatedSubmissionPlan,
    };
    use crate::ai::cloud::validator::CloudOutputValidator;
    use crate::ai::cloud::ExecutionClass;
    use crate::projects::ProjectManager;
    use crate::system::StoragePaths;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
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

        // Create test project
        let pm = ProjectManager::new(paths.clone());
        let project = pm.create_project("Phase 15 Test Project").unwrap();

        // Create synthetic 1s valid MP4 fixture (576x1024 @ 25fps)
        let sample_mp4 = base.join("valid_sample.mp4");
        create_synthetic_mp4(&sample_mp4, 1, 576, 1024, 25, true);

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
            Box::pin(async move {
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
    // TEST 3 — DEDUPE: SEQUENTIAL SAME CLIENT REQUEST ID (Section 2A)
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
    // TEST 4 — DEDUPE: CONCURRENT SAME CLIENT REQUEST ID (Section 2B)
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
    // TEST 5 — DEDUPE: RESTART THEN RETRY SAME CLIENT REQUEST ID (Section 2C)
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
    // TEST 6 — CANCELLATION NORMAL RECONCILIATION FLOW (Section 3)
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_06_cancellation_normal_reconciliation_flow() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.poll_responses.lock().unwrap() = vec![RemotePollResponse {
            remote_id: "mock_remote_cancel_1".to_string(),
            status: RemoteStatus::Processing,
            output_url: None,
            error: None,
        }];

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

        let req = make_test_req("cjob-test-cancel-1", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        let cancelled = service
            .cancel_cloud_generation(&project_id, &job.internal_job_id)
            .await
            .unwrap();

        assert_eq!(cancelled.state, CloudJobState::Cancelled);
        assert!(cancelled.cancellation_requested);
        assert_eq!(mock_provider.cancel_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 7 — CANCELLATION RESTART RECONCILIATION (Section 3)
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_07_cancellation_restart_reconciliation() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();

        // Create job with cancellationRequested=true, Processing, and remote ID
        let mut job = PersistentCloudJob::new(
            "client_req_cancel_rec".to_string(),
            "cjob-cancel-rec".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_cancel".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::Processing;
        job.submission_state = SubmissionState::Acknowledged;
        job.remote_job_id = Some("mock_remote_rec".to_string());
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

        let recovered = service.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Cancelled);
        assert_eq!(mock_provider.cancel_call_count.load(Ordering::SeqCst), 1);
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 0);
    }

    // =========================================================================
    // TEST 8 — CANCELLATION REMOTE FAILURE BLOCKS (Section 3)
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_08_cancellation_remote_failure_blocks() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.cancel_behavior.lock().unwrap() =
            Err("Provider API 502 Bad Gateway during cancel".to_string());

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

        let req = make_test_req("cjob-test-cancel-fail", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        let cancel_res = service
            .cancel_cloud_generation(&project_id, &job.internal_job_id)
            .await;
        assert!(cancel_res.is_err());

        let reloaded = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(reloaded.state, CloudJobState::Blocked);
        assert!(reloaded.cancellation_requested);
        assert_eq!(
            reloaded.error.as_ref().unwrap().code,
            "CANCELLATION_FAILED_REMOTE"
        );
    }

    // =========================================================================
    // TEST 9 — PERSIST BEFORE EVENT ENFORCEMENT (Section 6)
    // =========================================================================

    #[derive(Clone)]
    pub struct FailingStoreEventSink {
        pub emit_count: Arc<AtomicU32>,
    }

    impl EventSink for FailingStoreEventSink {
        fn emit_job_updated(
            &self,
            _payload: &crate::ai::cloud::CloudJobEventPayload,
        ) -> Result<(), String> {
            self.emit_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_phase15_09_persist_before_event_enforcement() {
        let (paths, _temp, _project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider),
        });
        let event_sink = Arc::new(FailingStoreEventSink {
            emit_count: Arc::new(AtomicU32::new(0)),
        });

        let service = create_test_service(
            paths,
            resolver,
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        // Provide invalid project ID with path traversal
        let req = make_test_req("cjob-fail-persist", "../invalid_project", &sample_mp4);

        let res = service.start_cloud_generation(req, Some(3.00)).await;
        assert!(res.is_err());

        // Zero events must be emitted when persistence fails
        assert_eq!(event_sink.emit_count.load(Ordering::SeqCst), 0);
    }

    // =========================================================================
    // TEST 10 — VALIDATION: WRONG DURATION FAILS (Section 7)
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_10_validation_wrong_duration_fails() {
        let (paths, temp, project_id, sample_mp4) = create_test_env();

        // Create a 5-second valid video fixture
        let five_sec_mp4 = temp.path().join("five_sec.mp4");
        create_synthetic_mp4(&five_sec_mp4, 5, 576, 1024, 25, true);

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.download_behavior.lock().unwrap() = vec![Ok(five_sec_mp4)];

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

        // Requested duration is 1.0 second, but provider delivered 5.0 seconds
        let req = make_test_req("cjob-dur-mismatch", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Failed);
        assert_eq!(final_job.error.as_ref().unwrap().code, "VALIDATION_FAILED");
        assert!(final_job
            .error
            .as_ref()
            .unwrap()
            .sanitized_message
            .contains("exceeds tolerance bounds"));
    }

    // =========================================================================
    // TEST 11 — VALIDATION: AUDIO PRESERVATION (Section 7)
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_11_validation_audio_preservation() {
        let (paths, temp, project_id, sample_mp4) = create_test_env();

        // Create a 1-second video WITHOUT audio
        let no_audio_mp4 = temp.path().join("no_audio.mp4");
        create_synthetic_mp4(&no_audio_mp4, 1, 576, 1024, 25, false);

        // Case A: Transformation with source video requires audio -> provider returns no audio -> FAILS
        {
            let mock_provider = Arc::new(MockCloudProvider::new());
            *mock_provider.download_behavior.lock().unwrap() = vec![Ok(no_audio_mp4.clone())];

            let resolver = Arc::new(MockProviderResolver {
                provider: Some(mock_provider),
            });
            let event_sink = Arc::new(TestEventSink::new());

            let service = create_test_service(
                paths.clone(),
                resolver,
                event_sink,
                LifecycleTimingConfig::fast_test(),
            );

            let req = make_test_req("cjob-audio-req", &project_id, &sample_mp4);

            let job = service
                .start_cloud_generation(req, Some(3.00))
                .await
                .unwrap();

            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            let final_job = service
                .get_job_status(&project_id, &job.internal_job_id)
                .unwrap();
            assert_eq!(final_job.state, CloudJobState::Failed);
            assert_eq!(
                final_job.error.as_ref().unwrap().code,
                "VALIDATION_FAILED"
            );
            assert!(final_job
                .error
                .as_ref()
                .unwrap()
                .sanitized_message
                .contains("Audio preservation"));
        }

        // Case B: Generation without source video does not require audio -> provider returns no audio -> SUCCEEDS
        {
            let mock_provider = Arc::new(MockCloudProvider::new());
            *mock_provider.download_behavior.lock().unwrap() = vec![Ok(no_audio_mp4)];

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

            let mut req = make_test_req("cjob-no-audio-req", &project_id, &sample_mp4);
            req.source_video = None;

            let job = service
                .start_cloud_generation(req, Some(3.00))
                .await
                .unwrap();

            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            let final_job = service
                .get_job_status(&project_id, &job.internal_job_id)
                .unwrap();
            assert_eq!(final_job.state, CloudJobState::Completed);
            assert!(final_job.output.final_path.is_some());
        }
    }

    // =========================================================================
    // TEST 12 — ATOMIC ARTIFACT PROMOTION (Section 8)
    // =========================================================================

    #[test]
    fn test_phase15_12_atomic_artifact_promotion() {
        let temp = tempdir().unwrap();
        let partial_path = temp.path().join("test.partial");
        let final_path = temp.path().join("final.mp4");

        // Create valid 1s mp4 as partial
        create_synthetic_mp4(&partial_path, 1, 576, 1024, 25, true);

        // Pre-create an old final file
        {
            let mut f = File::create(&final_path).unwrap();
            f.write_all(b"old_final_content").unwrap();
        }

        let validator = CloudOutputValidator::new();
        let record = validator
            .validate_and_promote_artifact(&partial_path, &final_path, Some(1.0), false)
            .unwrap();

        assert!(!partial_path.exists());
        assert!(final_path.exists());
        assert!(record.artifact_hash.is_some());
    }

    // =========================================================================
    // TEST 13 — MISSING CREDENTIALS WITH REAL RESOLVER (Section 9)
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_13_missing_credentials_real_resolver_blocks_and_resumes() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();

        // Seed a processing job with a remote ID
        let mut job = PersistentCloudJob::new(
            "client_req_real_resolver".to_string(),
            "cjob-real-res".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_13".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::Processing;
        job.submission_state = SubmissionState::Acknowledged;
        job.remote_job_id = Some("rem_live_123".to_string());

        let store = PersistentCloudJobStore::new(paths.clone());
        store.save_job_atomic(&job).unwrap();

        // Real DefaultCloudProviderResolver without REPLICATE_API_TOKEN set in test environment
        let real_resolver = Arc::new(DefaultCloudProviderResolver::new());
        let event_sink = Arc::new(TestEventSink::new());

        let service = CloudJobLifecycleService::new(
            paths.clone(),
            real_resolver,
            event_sink.clone(),
            Arc::new(MockSubmissionGate::default()),
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Blocked);
        assert_eq!(
            recovered[0].error.as_ref().unwrap().code,
            "MISSING_PROVIDER_CREDENTIALS"
        );
        assert_eq!(recovered[0].remote_job_id, Some("rem_live_123".to_string()));

        // Resume once provider becomes available
        let mock_provider = Arc::new(MockCloudProvider::new());
        let mock_resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });

        let service_with_creds = CloudJobLifecycleService::new(
            paths,
            mock_resolver,
            event_sink,
            Arc::new(MockSubmissionGate::default()),
            LifecycleTimingConfig::fast_test(),
        );

        let resumed = service_with_creds
            .resume_unblock_job(&project_id, "cjob-real-res")
            .await
            .unwrap();
        assert_eq!(resumed.state, CloudJobState::Processing);
        assert_eq!(resumed.remote_job_id, Some("rem_live_123".to_string()));
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 0);
    }

    // =========================================================================
    // TEST 14 — VALIDATING_OUTPUT RECOVERY WITHOUT PROVIDER (Section 10)
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_14_validating_output_recovery_no_provider_needed() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();
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
            "hash_val".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::ValidatingOutput;
        job.validation_policy = ValidationPolicy {
            expected_duration_sec: Some(1.0),
            require_audio: false,
        };
        store.save_job_atomic(&job).unwrap();

        // Create the downloaded .partial file on disk
        let partial_path = store
            .artifact_partial_path(&project_id, "cjob-val-rec")
            .unwrap();
        create_synthetic_mp4(&partial_path, 1, 576, 1024, 25, true);

        // Resolver with NO provider credentials
        let resolver_empty = Arc::new(MockProviderResolver { provider: None });
        let event_sink = Arc::new(TestEventSink::new());

        let service = CloudJobLifecycleService::new(
            paths,
            resolver_empty,
            event_sink,
            Arc::new(MockSubmissionGate::default()),
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service.recover_startup_jobs().await.unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Completed);
        assert!(recovered[0].output.final_path.is_some());
    }

    // =========================================================================
    // TEST 15 — MONOTONIC REVISION CRASH RECOVERY (Section 4A)
    // =========================================================================

    #[test]
    fn test_phase15_15_monotonic_revision_crash_recovery() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths);

        // Primary on disk has revision N with SUBMITTED / IN_FLIGHT / no remoteJobId
        let mut primary_job = PersistentCloudJob::new(
            "job_test_15".to_string(),
            "cjob-15".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_15".to_string(),
            CostRecord::default(),
        );
        primary_job.state_revision = 1;
        primary_job.state = CloudJobState::Submitted;
        primary_job.submission_state = SubmissionState::InFlight;
        primary_job.remote_job_id = None;
        store.save_job_atomic(&primary_job).unwrap();

        // Temp file on disk has revision N+1 with PROCESSING / ACKNOWLEDGED / remoteJobId present (crash before rename)
        let mut tmp_job = primary_job.clone();
        tmp_job.state_revision = 2;
        tmp_job.state = CloudJobState::Processing;
        tmp_job.submission_state = SubmissionState::Acknowledged;
        tmp_job.remote_job_id = Some("rem_ack_15".to_string());

        let tmp_path = store
            .job_tmp_file_path(&project_id, &primary_job.internal_job_id)
            .unwrap();
        let tmp_json = serde_json::to_string_pretty(&tmp_job).unwrap();
        fs::write(&tmp_path, tmp_json).unwrap();

        // Loading must select the higher revision (N+1 = 2) and promote it
        let recovered = store
            .load_job(&project_id, &primary_job.internal_job_id)
            .unwrap();
        assert_eq!(recovered.state_revision, 2);
        assert_eq!(recovered.state, CloudJobState::Processing);
        assert_eq!(recovered.submission_state, SubmissionState::Acknowledged);
        assert_eq!(recovered.remote_job_id, Some("rem_ack_15".to_string()));
    }

    // =========================================================================
    // TEST 16 — PATH TRAVERSAL REJECTION
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_16_path_traversal_rejection() {
        let (paths, _temp, _project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
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

        let malicious_requests = [
            ("../escape_proj", "cjob-1"),
            ("proj_1", "../escape_job"),
            ("C:\\Windows", "cjob-2"),
            ("/etc/passwd", "cjob-3"),
            ("proj\0null", "cjob-4"),
        ];

        for (pid, jid) in malicious_requests {
            let req = CloudJobRequest {
                job_id: jid.to_string(),
                project_id: Some(pid.to_string()),
                prompt: "Path traversal probe".to_string(),
                negative_prompt: None,
                source_video: Some(sample_mp4.clone()),
                reference_image: None,
                duration_seconds: 1.0,
                fps: 25.0,
                resolution: (576, 1024),
                task_type: "FullTransformation".to_string(),
            };

            let res = service.start_cloud_generation(req, Some(3.00)).await;
            assert!(
                res.is_err(),
                "Path traversal must be rejected for pid: {}",
                pid
            );
            let err = format!("{}", res.unwrap_err());
            assert!(
                err.contains("INVALID_IDENTIFIER") || err.contains("Project not found"),
                "Expected rejection, got: {}",
                err
            );
        }
    }

    // =========================================================================
    // TEST 17 — PRODUCTION COST-SAVING REGRESSION TEST (Section 1)
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_17_production_cost_saving_regression_blocks_full_transformation() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        // Production service uses DefaultCloudSubmissionGate with COST_SAVING policy
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
}
