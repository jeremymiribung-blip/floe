use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, Runtime};

use crate::{
    recording::{RecordingErrorCode, RecordingManager, ShutdownRecordingResult},
    system::hotkey,
};

static IS_QUITTING: AtomicBool = AtomicBool::new(false);
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

pub fn is_quitting() -> bool {
    IS_QUITTING.load(Ordering::SeqCst)
}

pub fn mark_quitting() {
    IS_QUITTING.store(true, Ordering::SeqCst);
}

#[cfg(test)]
pub fn reset_quitting_for_test() {
    IS_QUITTING.store(false, Ordering::SeqCst);
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

#[cfg(test)]
mod tests {
    use super::{is_quitting, mark_quitting, reset_quitting_for_test};

    #[test]
    fn is_quitting_defaults_to_false() {
        reset_quitting_for_test();
        assert!(!is_quitting());
    }

    #[test]
    fn mark_quitting_sets_flag() {
        reset_quitting_for_test();
        assert!(!is_quitting());
        mark_quitting();
        assert!(is_quitting());
        reset_quitting_for_test();
    }
}
