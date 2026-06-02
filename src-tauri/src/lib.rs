mod commands;
mod providers;
mod recording;
mod settings;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(recording::RecordingManager::with_cpal())
        .setup(|app| {
            use tauri::Manager;

            let config_dir = app.path().app_config_dir()?;
            app.manage(settings::SettingsManager::new(config_dir));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_status,
            commands::app::run_manual_test_stub,
            commands::settings::save_groq_api_key,
            commands::settings::clear_groq_api_key,
            commands::settings::get_groq_api_key_status,
            commands::settings::get_app_settings,
            commands::settings::save_app_settings,
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::get_recording_status,
            commands::recording::get_latest_recording_info,
            commands::recording::get_latest_recording_wav_bytes,
            commands::transcription::transcribe_latest_recording,
            commands::clipboard::copy_text_to_clipboard,
            commands::clipboard::paste_text,
            commands::clipboard::paste_clipboard,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Floe");
}
