use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager, Runtime, WebviewWindow,
};

use crate::{
    recording::{RecordingErrorCode, RecordingManager, ShutdownRecordingResult},
    system::hotkey,
};

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_SHOW_ID: &str = "tray-show-floe";
const TRAY_SETTINGS_ID: &str = "tray-settings";
const TRAY_QUIT_ID: &str = "tray-quit";
const SETTINGS_EVENT: &str = "floe-show-settings";

static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy)]
pub enum LifecycleLevel {
    Info,
    Warn,
    Error,
}

pub fn log_lifecycle(level: LifecycleLevel, event: &'static str) {
    let level = match level {
        LifecycleLevel::Info => "info",
        LifecycleLevel::Warn => "warn",
        LifecycleLevel::Error => "error",
    };

    eprintln!("[floe:lifecycle] level={level} event={event}");
}

pub fn setup_tray(app: &mut App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, TRAY_SHOW_ID, "Show Floe", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, TRAY_SETTINGS_ID, "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT_ID, "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &settings, &separator, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("Floe")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_SHOW_ID => show_main_window(app),
            TRAY_SETTINGS_ID => show_settings(app),
            TRAY_QUIT_ID => {
                log_lifecycle(LifecycleLevel::Info, "tray_quit_requested");
                app.exit(0);
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    log_lifecycle(LifecycleLevel::Info, "tray_ready");

    Ok(())
}

pub fn cleanup_before_exit<R: Runtime>(app: &AppHandle<R>) {
    if SHUTDOWN_STARTED.swap(true, Ordering::SeqCst) {
        log_lifecycle(LifecycleLevel::Info, "shutdown_cleanup_already_started");
        return;
    }

    log_lifecycle(LifecycleLevel::Info, "shutdown_cleanup_started");

    hotkey::unregister_shutdown_hotkey(app);

    match app.try_state::<RecordingManager>() {
        Some(manager) => match manager.stop_for_shutdown() {
            Ok(ShutdownRecordingResult::Idle) => {
                log_lifecycle(LifecycleLevel::Info, "shutdown_recording_idle");
            }
            Ok(ShutdownRecordingResult::Finalized) => {
                log_lifecycle(LifecycleLevel::Info, "shutdown_recording_finalized");
            }
            Ok(ShutdownRecordingResult::DiscardedEmpty) => {
                log_lifecycle(LifecycleLevel::Info, "shutdown_recording_discarded_empty");
            }
            Err(error) => {
                log_lifecycle(
                    LifecycleLevel::Error,
                    recording_shutdown_error_event(error.code),
                );
            }
        },
        None => {
            log_lifecycle(LifecycleLevel::Warn, "shutdown_recording_manager_missing");
        }
    }

    log_lifecycle(LifecycleLevel::Info, "shutdown_cleanup_finished");
}

fn show_settings<R: Runtime>(app: &AppHandle<R>) {
    show_main_window(app);

    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        if window.emit(SETTINGS_EVENT, ()).is_err() {
            log_lifecycle(LifecycleLevel::Warn, "settings_event_emit_failed");
        }
    }
}

fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        log_lifecycle(LifecycleLevel::Warn, "main_window_missing");
        return;
    };

    show_and_focus_window(&window);
}

fn show_and_focus_window<R: Runtime>(window: &WebviewWindow<R>) {
    if window.show().is_err() {
        log_lifecycle(LifecycleLevel::Warn, "main_window_show_failed");
    }

    if window.unminimize().is_err() {
        log_lifecycle(LifecycleLevel::Warn, "main_window_unminimize_failed");
    }

    if window.set_focus().is_err() {
        log_lifecycle(LifecycleLevel::Warn, "main_window_focus_failed");
    }
}

fn recording_shutdown_error_event(code: RecordingErrorCode) -> &'static str {
    match code {
        RecordingErrorCode::NoInputDevice => "shutdown_recording_error_no_input_device",
        RecordingErrorCode::PermissionDenied => "shutdown_recording_error_permission_denied",
        RecordingErrorCode::AlreadyRecording => "shutdown_recording_error_already_recording",
        RecordingErrorCode::NotRecording => "shutdown_recording_error_not_recording",
        RecordingErrorCode::EmptyRecording => "shutdown_recording_error_empty_recording",
        RecordingErrorCode::UnsupportedSampleFormat => {
            "shutdown_recording_error_unsupported_sample_format"
        }
        RecordingErrorCode::DeviceDisconnected => "shutdown_recording_error_device_disconnected",
        RecordingErrorCode::StreamBuildFailed => "shutdown_recording_error_stream_build_failed",
        RecordingErrorCode::StreamPlayFailed => "shutdown_recording_error_stream_play_failed",
        RecordingErrorCode::WavEncodingFailed => "shutdown_recording_error_wav_encoding_failed",
        RecordingErrorCode::Internal => "shutdown_recording_error_internal",
    }
}
