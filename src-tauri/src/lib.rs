pub mod ai;
pub mod commands;
pub mod error;
pub mod events;
pub mod export;
pub mod jobs;
pub mod media;
pub mod models;
pub mod projects;
pub mod runtime;
pub mod system;

use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
