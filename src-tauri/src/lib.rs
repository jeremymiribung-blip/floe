mod cleanup;
mod commands;
mod lifecycle;
mod providers;
mod recording;
mod settings;
mod system;

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![system::startup::BACKGROUND_ARG]),
        ))
        .manage(recording::RecordingManager::with_cpal())
        .manage(system::hotkey::HotkeyManager::default())
        .setup(|app| {
            use tauri::Manager;

            let is_background_launch = system::startup::is_background_launch_from_env();
            let config_dir = app.path().app_config_dir()?;
            app.manage(settings::SettingsManager::new(config_dir));
            system::tray::setup_tray(app)?;
            system::hotkey::register_startup_hotkey(app.handle());

            if is_background_launch {
                lifecycle::log_lifecycle(
                    lifecycle::LifecycleLevel::Info,
                    "background_startup_hidden",
                );
            } else {
                system::window::show_main_window(app.handle());
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if system::window::is_main_window(window) {
                    system::window::handle_main_window_close_request(window, api);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_status,
            commands::app::run_manual_test_stub,
            commands::settings::save_groq_api_key,
            commands::settings::clear_groq_api_key,
            commands::settings::get_groq_api_key_status,
            commands::settings::save_cerebras_api_key,
            commands::settings::clear_cerebras_api_key,
            commands::settings::get_cerebras_api_key_status,
            commands::settings::get_app_settings,
            commands::settings::save_app_settings,
            commands::settings::get_cleanup_mode,
            commands::settings::set_cleanup_mode,
            commands::settings::get_start_at_login_status,
            commands::settings::set_start_at_login_enabled,
            commands::hotkey::get_hotkey_settings,
            commands::hotkey::set_hotkey,
            commands::hotkey::reset_hotkey_to_default,
            commands::hotkey::register_global_hotkey,
            commands::hotkey::unregister_global_hotkey,
            commands::recording::start_recording,
            commands::recording::stop_recording,
            commands::recording::get_recording_status,
            commands::recording::get_latest_recording_info,
            commands::recording::get_latest_recording_wav_bytes,
            commands::transcription::transcribe_latest_recording,
            commands::cleanup::cleanup_transcript,
            commands::clipboard::copy_text_to_clipboard,
            commands::clipboard::paste_text,
            commands::clipboard::paste_clipboard,
        ]);

    match builder.build(tauri::generate_context!()) {
        Ok(app) => {
            lifecycle::log_lifecycle(lifecycle::LifecycleLevel::Info, "app_started");
            app.run(|app, event| {
                if let tauri::RunEvent::ExitRequested { .. } = event {
                    lifecycle::cleanup_before_exit(app);
                }
            });
        }
        Err(_error) => {
            lifecycle::log_lifecycle(lifecycle::LifecycleLevel::Error, "app_build_failed");
        }
    }
}
