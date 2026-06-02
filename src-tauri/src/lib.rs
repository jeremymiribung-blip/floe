mod commands;
mod recording;

pub fn run() {
    tauri::Builder::default()
        .manage(recording::RecordingManager::with_cpal())
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_status,
            commands::app::get_settings_stub,
            commands::app::run_manual_test_stub,
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::get_recording_status,
            commands::recording::get_latest_recording_info,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Floe");
}
