#[cfg(test)]
mod tests {
    use crate::ai::cloud::cost::CostConfidence;
    use crate::ai::cloud::error::CloudProviderError;
    use crate::ai::cloud::job::{
        CloudJobRequest, CloudJobState, CostRecord, InputAssets, PersistentCloudJob,
        SubmissionState,
    };
    use crate::ai::cloud::lifecycle::{
        CloudJobLifecycleService, LifecycleTimingConfig, TestEventSink,
    };
    use crate::ai::cloud::provider::{
        CloudJobHandle, CloudVideoProvider, ProviderCapabilities, RemotePollResponse, RemoteStatus,
    };
    use crate::ai::cloud::resolver::CloudProviderResolver;
    use crate::ai::cloud::store::PersistentCloudJobStore;
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
        create_synthetic_mp4(&sample_mp4);

        (paths, temp, project.id, sample_mp4)
    }

    fn create_synthetic_mp4(path: &Path) {
        let _ = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=576x1024:rate=25",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                path.to_str().unwrap(),
            ])
            .output();

        if !path.exists() {
            // Fallback mock bytes if ffmpeg is not in PATH during minimal test environments
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
    // Mock Cloud Video Provider
    // -------------------------------------------------------------------------

    pub struct MockCloudProvider {
        pub submit_call_count: Arc<AtomicU32>,
        pub poll_call_count: Arc<AtomicU32>,
        pub cancel_call_count: Arc<AtomicU32>,
        pub download_call_count: Arc<AtomicU32>,
        pub submit_behavior: Mutex<Result<String, String>>, // Ok(remote_id) or Err(msg)
        pub poll_responses: Mutex<Vec<RemotePollResponse>>,
        pub download_behavior: Mutex<Vec<Result<PathBuf, String>>>, // Ok(fixture_to_copy) or Err(msg)
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
            Box::pin(async move { Ok(()) })
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

    // -------------------------------------------------------------------------
    // Mock Provider Resolver
    // -------------------------------------------------------------------------

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

        let service = CloudJobLifecycleService::new(
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

        // Wait briefly for background polling and promotion to complete
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Completed);
        assert!(final_job.output.final_path.is_some());
        assert!(final_job.output.final_path.as_ref().unwrap().exists());
        assert!(final_job.output.artifact_hash.is_some());

        // Verify events were emitted in order
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
        // Configure poll to remain in processing during the first manager instance
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

        let service = CloudJobLifecycleService::new(
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

        // Destroy service and recreate a new service instance (simulating app restart)
        drop(service);

        let service_v2 = CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service_v2.recover_startup_jobs().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].internal_job_id, internal_id);
        assert_eq!(recovered[0].state, CloudJobState::Processing);
        assert_eq!(
            recovered[0].remote_job_id,
            Some("mock_remote_123".to_string())
        );

        // Submit call count must strictly remain 1 (no double submission on restart!)
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 3 — RESTART CANNOT DOUBLE SUBMIT
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_03_restart_cannot_double_submit() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = CloudJobLifecycleService::new(
            paths.clone(),
            resolver.clone(),
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-03", &project_id, &sample_mp4);

        let _ = service
            .start_cloud_generation(req.clone(), Some(3.00))
            .await
            .unwrap();
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);

        // Attempting start_cloud_generation again for the same job must fail and not submit
        let duplicate_attempt = service.start_cloud_generation(req, Some(3.00)).await;
        assert!(duplicate_attempt.is_err());
        assert!(format!("{}", duplicate_attempt.unwrap_err())
            .contains("DUPLICATE_SUBMISSION_PREVENTED"));
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 4 — CANCELLATION SURVIVES RESTART
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_04_cancellation_survives_restart() {
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

        let service = CloudJobLifecycleService::new(
            paths.clone(),
            resolver.clone(),
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-04", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        let cancelled_job = service
            .cancel_cloud_generation(&project_id, &job.internal_job_id)
            .await
            .unwrap();
        assert_eq!(cancelled_job.state, CloudJobState::Cancelled);
        assert!(cancelled_job.cancellation_requested);

        // Recreate service instance and recover
        drop(service);
        let service_v2 = CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let reloaded = service_v2
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(reloaded.state, CloudJobState::Cancelled);
        assert!(reloaded.cancellation_requested);
    }

    // =========================================================================
    // TEST 5 — CORRUPT OUTPUT FAILS VALIDATION
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_05_corrupt_output_fails_validation() {
        let (paths, temp, project_id, sample_mp4) = create_test_env();

        // Create corrupt artifact (zero bytes or random text)
        let corrupt_mp4 = temp.path().join("corrupt_video.mp4");
        {
            let mut f = File::create(&corrupt_mp4).unwrap();
            f.write_all(b"not a valid video stream").unwrap();
        }

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.download_behavior.lock().unwrap() = vec![Ok(corrupt_mp4)];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-05", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();

        // Wait for background download and validation to run
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let final_job = service
            .get_job_status(&project_id, &job.internal_job_id)
            .unwrap();
        assert_eq!(final_job.state, CloudJobState::Failed);
        assert!(final_job.error.is_some());
        assert_eq!(final_job.error.as_ref().unwrap().code, "VALIDATION_FAILED");
        assert!(final_job.output.final_path.is_none());
    }

    // =========================================================================
    // TEST 6 — POLLING TIMEOUT FAILS BOUNDED
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_06_polling_timeout_fails_bounded() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        // Provide empty poll responses -> stays Processing forever
        *mock_provider.poll_responses.lock().unwrap() = Vec::new();

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        // Fast timeout of 100ms
        let mut timing = LifecycleTimingConfig::fast_test();
        timing.max_poll_duration_sec = 0; // Immediate timeout on 1st loop

        let service = CloudJobLifecycleService::new(paths, resolver, event_sink, timing);

        let req = make_test_req("cjob-test-06", &project_id, &sample_mp4);

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
    // TEST 7 — DOWNLOAD RETRY BOUNDED
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_07_download_retry_bounded() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        // 1st download attempt fails, 2nd attempt succeeds with sample_mp4
        *mock_provider.download_behavior.lock().unwrap() = vec![
            Err("Transient network connection drop".to_string()),
            Ok(sample_mp4.clone()),
        ];

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-07", &project_id, &sample_mp4);

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
    // TEST 8 — AMBIGUOUS SUBMISSION BLOCKS AUTO RESUBMISSION
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_08_ambiguous_submission_blocks_auto_resubmit() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        // Configure provider submit to fail with network drop
        *mock_provider.submit_behavior.lock().unwrap() =
            Err("Network connection reset during POST".to_string());

        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = CloudJobLifecycleService::new(
            paths.clone(),
            resolver.clone(),
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-08", &project_id, &sample_mp4);

        let result = service
            .start_cloud_generation(req.clone(), Some(3.00))
            .await;
        assert!(result.is_err());

        // Verify the job was recorded as Blocked with AMBIGUOUS_SUBMISSION on disk
        let active = service.store().list_all_active_jobs().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].state, CloudJobState::Blocked);
        assert_eq!(active[0].submission_state, SubmissionState::Ambiguous);
        assert_eq!(
            active[0].error.as_ref().unwrap().code,
            "AMBIGUOUS_SUBMISSION"
        );

        // Simulate app restart
        drop(service);
        let service_v2 = CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service_v2.recover_startup_jobs().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Blocked);
        assert_eq!(recovered[0].submission_state, SubmissionState::Ambiguous);

        // Submit call count must be exactly 1 (never auto-resubmitted on restart!)
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 9 — ATOMIC WRITE RECOVERY FROM CORRUPT TEMP
    // =========================================================================

    #[test]
    fn test_phase15_09_atomic_write_recovery_temp_corrupt() {
        let (paths, _temp, project_id, _sample_mp4) = create_test_env();
        let store = PersistentCloudJobStore::new(paths);

        let mut job = PersistentCloudJob::new(
            "job_test_09".to_string(),
            "cjob-09".to_string(),
            project_id.clone(),
            "replicate".to_string(),
            "minimax/video-01".to_string(),
            "minimax/video-01".to_string(),
            "FullTransformation".to_string(),
            ExecutionClass::SpecializedVideoTransformation,
            InputAssets::default(),
            "hash_123".to_string(),
            CostRecord::default(),
        );
        job.state = CloudJobState::Processing;

        // Save primary
        store.save_job_atomic(&job).unwrap();

        // Write corrupt .tmp file
        let tmp_path = store
            .job_tmp_file_path(&project_id, &job.internal_job_id)
            .unwrap();
        {
            let mut f = File::create(&tmp_path).unwrap();
            f.write_all(b"{ invalid corrupted json").unwrap();
        }

        // Load must recover primary safely
        let loaded = store.load_job(&project_id, &job.internal_job_id).unwrap();
        assert_eq!(loaded.internal_job_id, "cjob-09");
        assert_eq!(loaded.state, CloudJobState::Processing);
    }

    // =========================================================================
    // TEST 10 — OLD STATE BACKWARD COMPATIBILITY
    // =========================================================================

    #[test]
    fn test_phase15_10_old_state_backward_compatibility() {
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
    // TEST 11 — EVENT CONTRACT SERIALIZATION
    // =========================================================================

    #[test]
    fn test_phase15_11_event_contract_serialization() {
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
    // TEST 12 — ILLEGAL TRANSITIONS REJECTED
    // =========================================================================

    #[test]
    fn test_phase15_12_illegal_transitions_rejected() {
        assert!(!CloudJobState::Completed.can_transition_to(CloudJobState::Processing));
        assert!(!CloudJobState::Cancelled.can_transition_to(CloudJobState::Submitted));
        assert!(!CloudJobState::Failed.can_transition_to(CloudJobState::Processing));
        assert!(!CloudJobState::Processing.can_transition_to(CloudJobState::Completed));
        assert!(!CloudJobState::Created.can_transition_to(CloudJobState::Completed));

        // Allowed transitions
        assert!(CloudJobState::Created.can_transition_to(CloudJobState::Validating));
        assert!(CloudJobState::Submitted.can_transition_to(CloudJobState::Processing));
        assert!(CloudJobState::Processing.can_transition_to(CloudJobState::Downloading));
        assert!(CloudJobState::Downloading.can_transition_to(CloudJobState::ValidatingOutput));
        assert!(CloudJobState::ValidatingOutput.can_transition_to(CloudJobState::Completed));
    }

    // =========================================================================
    // TEST 13 — SOURCE MEDIA IMMUTABILITY
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_13_source_media_immutability() {
        let (paths, temp, project_id, sample_mp4) = create_test_env();

        let initial_source_hash = CloudOutputValidator::compute_file_sha256(&sample_mp4).unwrap();

        // Run failing lifecycle test
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

        let service = CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-13", &project_id, &sample_mp4);

        let _ = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Verify source video file was NOT modified or deleted
        assert!(sample_mp4.exists());
        let post_test_source_hash = CloudOutputValidator::compute_file_sha256(&sample_mp4).unwrap();
        assert_eq!(initial_source_hash, post_test_source_hash);
    }

    // =========================================================================
    // TEST 14 — CONCURRENT SAME-JOB SUBMISSIONS
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_14_concurrent_same_job_submissions() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        let resolver = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = Arc::new(CloudJobLifecycleService::new(
            paths,
            resolver,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        ));

        let req1 = make_test_req("cjob-test-14", &project_id, &sample_mp4);
        let req2 = req1.clone();

        let s1 = service.clone();
        let s2 = service.clone();

        // Launch two simultaneous start_cloud_generation calls for the exact same job ID
        let (res1, res2) = tokio::join!(
            tokio::spawn(async move { s1.start_cloud_generation(req1, Some(3.00)).await }),
            tokio::spawn(async move { s2.start_cloud_generation(req2, Some(3.00)).await })
        );

        let out1 = res1.unwrap();
        let out2 = res2.unwrap();

        // One must succeed and one must be rejected with DUPLICATE_SUBMISSION_PREVENTED
        let successes = [out1.is_ok(), out2.is_ok()].iter().filter(|&&b| b).count();
        assert_eq!(successes, 1, "Exactly one submission must succeed");

        // Provider submit must be called exactly ONCE!
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // TEST 15 — MONOTONIC REVISION CRASH RECOVERY
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

        let service = CloudJobLifecycleService::new(
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
    // TEST 17 — MISSING CREDENTIALS SAFE RECOVERY & RESUME
    // =========================================================================

    #[tokio::test]
    async fn test_phase15_17_missing_credentials_safe_recovery_and_resume() {
        let (paths, _temp, project_id, sample_mp4) = create_test_env();

        let mock_provider = Arc::new(MockCloudProvider::new());
        *mock_provider.poll_responses.lock().unwrap() = vec![RemotePollResponse {
            remote_id: "mock_remote_17".to_string(),
            status: RemoteStatus::Processing,
            output_url: None,
            error: None,
        }];
        *mock_provider.submit_behavior.lock().unwrap() = Ok("mock_remote_17".to_string());

        let resolver_with_provider = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let event_sink = Arc::new(TestEventSink::new());

        let service = CloudJobLifecycleService::new(
            paths.clone(),
            resolver_with_provider,
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let req = make_test_req("cjob-test-17", &project_id, &sample_mp4);

        let job = service
            .start_cloud_generation(req, Some(3.00))
            .await
            .unwrap();
        let internal_id = job.internal_job_id.clone();
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);

        // Simulate app restart WITHOUT credentials
        drop(service);
        let resolver_empty = Arc::new(MockProviderResolver { provider: None });
        let service_no_creds = CloudJobLifecycleService::new(
            paths.clone(),
            resolver_empty,
            event_sink.clone(),
            LifecycleTimingConfig::fast_test(),
        );

        let recovered = service_no_creds.recover_startup_jobs().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, CloudJobState::Blocked);
        assert_eq!(
            recovered[0].error.as_ref().unwrap().code,
            "MISSING_PROVIDER_CREDENTIALS"
        );
        assert_eq!(
            recovered[0].remote_job_id,
            Some("mock_remote_17".to_string())
        );

        // Now credentials become available -> resume unblock job
        drop(service_no_creds);
        let resolver_restored = Arc::new(MockProviderResolver {
            provider: Some(mock_provider.clone()),
        });
        let service_restored = CloudJobLifecycleService::new(
            paths,
            resolver_restored,
            event_sink,
            LifecycleTimingConfig::fast_test(),
        );

        let resumed = service_restored
            .resume_unblock_job(&project_id, &internal_id)
            .await
            .unwrap();
        assert_eq!(resumed.state, CloudJobState::Processing);
        assert_eq!(resumed.remote_job_id, Some("mock_remote_17".to_string()));

        // Submit call count must remain strictly 1 (no double submission on resume!)
        assert_eq!(mock_provider.submit_call_count.load(Ordering::SeqCst), 1);
    }
}
