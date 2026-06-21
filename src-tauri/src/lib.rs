mod audio;
mod cleanup;
mod commands;
mod contract;
mod diag;
mod lifecycle;
mod prompts;
mod providers;
mod recording;
mod settings;
mod system;
mod update;

// ── Public re-exports for serialization contract tests (integration tests in tests/contract_tests.rs) ──
pub use cleanup::TranscriptCleanupResult;
pub use commands::clipboard::{ClipboardError, ClipboardErrorCode};
pub use providers::groq::types::GroqTranscription;
pub use recording::{
    RecordingEndReason, RecordingError, RecordingErrorCode, RecordingInfo, RecordingState,
    RecordingStatePayload, RecordingStatus,
};
pub use settings::{ApiKeyStatus, SettingsError, SettingsErrorCode};
pub use update::{UpdateError, UpdateErrorCode, UpdateInfo, UpdateStatusLabel};
pub use system::autostart::{StartAtLoginError, StartAtLoginErrorCode, StartAtLoginStatus};
pub use system::hotkey::{HotkeyError, HotkeyErrorCode, HotkeyStatus};

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod test_helpers;

pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        system::single_instance::handle_secondary_launch(app);
    }));

    let builder = builder
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![system::startup::BACKGROUND_ARG]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(system::hotkey::HotkeyManager::default())
        .manage(commands::diag::DiagLog::new())
        .manage(diag::PipelineContext::new())
        .manage(diag::PipelineTracer::new(20))
        .manage(diag::LastSessionStore::new())
        .setup(|app| {
            use tauri::{Emitter, Manager};

            let is_background_launch = system::startup::is_background_launch_from_env();
            let config_dir = app.path().app_config_dir()?;
            let diag_path = diag::default_diag_path(&config_dir);
            let _ = diag::init(log::LevelFilter::Debug, &diag_path, 2_000_000, 3);
            log::info!("diagnostic_logger_initialized path={}", diag_path.display());

            // ── File-backed session persistence & crash detection ──
            let session_path = diag::default_session_path(&config_dir);
            if let Some(store) = app.try_state::<diag::LastSessionStore>() {
                store.set_persist_path(session_path.clone());
            }

            // Detect and finalize any crashed session from the previous run.
            match diag::finalize_crashed_session(&session_path) {
                Ok(Some(crash_info)) => {
                    log::warn!(
                        "crash_detected_and_finalized previous_trace_id={:?}",
                        crash_info.trace_id
                    );
                }
                Ok(None) => {
                    // No crash detected — either no session file existed or it was clean.
                }
                Err(e) => {
                    log::warn!("crash_detection_failed error=\"{e}\"");
                }
            }

            let groq_http_client = providers::http::build_shared_http_client()?;

            let settings_manager = settings::SettingsManager::new(config_dir.clone());
            let mut app_settings = settings_manager.get_app_settings().unwrap_or_else(|e| {
                log::warn!("app_settings_load_failed error=\"{:?}\"", e);
                settings::AppSettings::default()
            });
            settings::migrate_legacy_keyring_entries(&mut app_settings);
            if let Err(e) = settings_manager.save_app_settings(app_settings) {
                log::warn!("app_settings_save_failed error=\"{:?}\"", e);
            }

            // Get app_settings before moving settings_manager
            let app_settings = settings_manager
                .get_app_settings()
                .unwrap_or_else(|e| {
                    log::warn!("app_settings_load_failed error=\"{:?}\"", e);
                    settings::AppSettings::default()
                });

            app.manage(settings_manager);

            // Create RecordingManager with configured audio device
            let recording_manager = recording::RecordingManager::new(Box::new(
                recording::CpalInputBackend::new(app_settings.device_id),
            ));
            app.manage(recording_manager);

            app.manage(providers::groq::GroqCleanupClient::new(
                groq_http_client.clone(),
            ));
            app.manage(providers::groq::stt::GroqTranscriptionClient::new(
                groq_http_client.clone(),
            ));

            system::tray::setup_tray(app)?;
            system::hotkey::register_startup_hotkey(app.handle());

            // Set up recording manager emitters
            if let Some(manager) = app.try_state::<recording::RecordingManager>() {
                let emit_app = app.handle().clone();
                manager.set_level_emitter(Box::new(move |level: f32| {
                    let _ = emit_app.emit(
                        audio::RECORDING_LEVEL_EVENT,
                        audio::RecordingLevelPayload { level },
                    );
                }));

                let state_app = app.handle().clone();
                manager.set_state_emitter(Box::new(move |state: recording::RecordingState| {
                    let _ = state_app.emit(
                        audio::RECORDING_STATE_EVENT,
                        recording::RecordingStatePayload {
                            state,
                            is_recording: state.is_active(),
                        },
                    );
                }));
            }

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
                if system::overlay::is_overlay_window(window) {
                    api.prevent_close();
                    if window.hide().is_err() {
                        lifecycle::log_lifecycle(
                            lifecycle::LifecycleLevel::Warn,
                            "overlay_hide_failed",
                        );
                    }
                } else if system::window::is_main_window(window) {
                    system::window::handle_main_window_close_request(window, api);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::save_api_key,
            commands::settings::validate_api_key,
            commands::settings::clear_api_key,
            commands::settings::get_api_key_status,
            commands::settings::get_app_settings,
            commands::settings::get_audio_devices,
            commands::settings::save_app_settings,
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
            commands::recording::force_stop_recording,
            commands::transcription::transcribe_latest_recording,
            commands::cleanup::cleanup_transcript,
            commands::clipboard::copy_text_to_clipboard,
            commands::clipboard::paste_text,
            commands::clipboard::paste_clipboard,
            commands::bubble::bubble_cancel_recording,
            commands::bubble::bubble_show,
            commands::bubble::bubble_hide,
            commands::bubble::bubble_set_state,
            commands::diag::diag_log,
            commands::diag::diag_log_str,
            commands::diag::get_recent_traces,
            commands::diag::get_current_trace,
            commands::diag::get_diagnostics_report,
            commands::diag::update_session_hotkey_latency,
            commands::diag::log_frontend_event,
            commands::update::check_and_install_update,
        ]);

    match builder.build(tauri::generate_context!()) {
        Ok(app) => {
            lifecycle::log_lifecycle(lifecycle::LifecycleLevel::Info, "app_started");
            system::single_instance::log_primary_started();
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
