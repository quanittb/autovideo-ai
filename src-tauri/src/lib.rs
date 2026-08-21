pub mod ai;
pub mod commands;
pub mod error;
pub mod events;
pub mod export;
pub mod jobs;
pub mod media;
pub mod models;
pub mod projects;
pub mod render;
pub mod runtime;
pub mod system;

use commands::*;
use jobs::JobEngine;
use std::sync::Arc;
use system::StoragePaths;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Perform startup recovery on interrupted pipeline jobs
    let storage_paths = StoragePaths::default_paths();
    let engine = JobEngine::new(storage_paths.clone());
    let _ = engine.recover_interrupted_jobs();

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let storage_paths = StoragePaths::default_paths();
            let event_sink = Arc::new(crate::ai::cloud::TauriEventSink::new(handle));
            let resolver = Arc::new(crate::ai::cloud::DefaultCloudProviderResolver::new());
            let submission_gate = Arc::new(crate::ai::cloud::DefaultCloudSubmissionGate::new());
            let lifecycle = Arc::new(crate::ai::cloud::CloudJobLifecycleService::new(
                storage_paths.clone(),
                resolver,
                event_sink,
                submission_gate,
                crate::ai::cloud::LifecycleTimingConfig::production(),
            ));
            let seg_store = Arc::new(crate::ai::cloud::SegmentedCloudJobStore::new(
                storage_paths.clone(),
            ));
            let registry = crate::ai::cloud::ProviderRegistry::new();
            let orchestrator = Arc::new(crate::ai::cloud::SegmentedCloudJobOrchestrator::new(
                lifecycle.clone(),
                (*seg_store).clone(),
                storage_paths,
                registry,
                Some(app.handle().clone()),
            ));

            app.manage(lifecycle.clone());
            app.manage(seg_store.clone());
            app.manage(orchestrator.clone());

            let orchestrator_clone = orchestrator.clone();
            tauri::async_runtime::spawn(async move {
                let _ = lifecycle.recover_startup_jobs().await;
                let _ = orchestrator_clone.recover_startup_segmented_jobs().await;
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            get_hardware_profile,
            get_storage_paths,
            get_ai_status,
            list_models,
            list_projects,
            get_project,
            create_project,
            update_project,
            delete_project,
            probe_media,
            import_media,
            get_media_runtime_status,
            prepare_media,
            extract_media_frames,
            extract_media_audio,
            validate_media_cache,
            open_directory,
            open_file_path,
            resolve_project_media,
            persist_editor_state,
            render_test_video,
            create_pipeline_job,
            start_pipeline_job,
            cancel_pipeline_job,
            retry_pipeline_job,
            delete_pipeline_job,
            get_pipeline_job,
            list_pipeline_jobs,
            get_job_logs,
            get_job_artifacts,
            validate_pipeline_job,
            list_ai_models,
            get_ai_model,
            register_ai_model,
            unregister_ai_model,
            get_ai_runtime_status,
            get_ai_devices,
            get_ai_providers,
            load_ai_model,
            unload_ai_model,
            inspect_ai_model,
            run_ai_inference,
            generate_test_model,
            generate_image_test_model,
            preview_ai_preprocess,
            validate_ai_preprocess,
            run_ai_pipeline,
            decode_ai_mask,
            create_ai_pipeline_job,
            get_ai_job_metrics,
            validate_ai_frame_artifacts,
            list_ai_model_families,
            list_ai_model_packages,
            get_ai_model_package,
            validate_ai_model_package,
            import_ai_model,
            activate_ai_model_version,
            rollback_ai_model,
            remove_ai_model_version,
            resolve_production_model,
            validate_ai_job_preflight,
            create_production_ai_job,
            get_ai_resource_limits,
            get_ai_runtime_resources,
            get_ai_execution_report,
            validate_ai_artifacts,
            get_storage_usage,
            clear_storage_cache,
            cleanup_temp_storage,
            get_all_job_history,
            get_generative_capabilities,
            check_generative_preflight,
            generate_keyframe,
            generate_video_pipeline,
            import_control_model,
            get_cloud_cost_estimate,
            get_generation_route,
            start_cloud_generation,
            get_cloud_job_status,
            cancel_cloud_generation,
            preflight_cloud_transformation,
            start_cloud_transformation,
            list_cloud_jobs,
            authorize_preview_asset,
            revoke_preview_asset,
            open_cloud_artifact,
            open_cloud_artifact_folder,
            preflight_segmented_cloud_transformation,
            start_segmented_cloud_transformation,
            list_segmented_cloud_jobs,
            cancel_segmented_cloud_job,
            approve_segmented_cloud_budget,
            authorize_segmented_preview_asset,
            revoke_segmented_preview_asset,
            optimize_prompt,
            get_gemini_status,
            set_gemini_api_key,
            clear_gemini_api_key,
            list_flow_profiles,
            create_flow_profile,
            delete_flow_profile,
            start_flow_generation,
            get_flow_job_status,
            list_flow_jobs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
