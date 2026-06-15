use std::sync::atomic::{AtomicU8, Ordering};

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::recording::{RecordingErrorCode, RecordingManager, ShutdownRecordingResult};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLifecycle {
    Running = 0,
    Quitting = 1,
    ShuttingDown = 2,
    Exited = 3,
}

static LIFECYCLE: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Copy)]
pub enum LifecycleLevel {
    Info,
    Warn,
    Error,
}

pub fn log_lifecycle(level: LifecycleLevel, event: &'static str) {
    match level {
        LifecycleLevel::Info => log::info!("event={event}"),
        LifecycleLevel::Warn => log::warn!("event={event}"),
        LifecycleLevel::Error => log::error!("event={event}"),
    }
}

pub fn current_lifecycle() -> AppLifecycle {
    match LIFECYCLE.load(Ordering::Acquire) {
        0 => AppLifecycle::Running,
        1 => AppLifecycle::Quitting,
        2 => AppLifecycle::ShuttingDown,
        3 => AppLifecycle::Exited,
        _ => unreachable!(),
    }
}

pub fn transition_lifecycle(from: AppLifecycle, to: AppLifecycle) -> bool {
    if from as u8 >= to as u8 {
        return false;
    }
    LIFECYCLE
        .compare_exchange(from as u8, to as u8, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

pub fn can_accept_commands() -> bool {
    current_lifecycle() == AppLifecycle::Running
}

pub fn is_quitting_or_shutdown() -> bool {
    current_lifecycle() as u8 >= AppLifecycle::Quitting as u8
}

pub fn is_quitting() -> bool {
    is_quitting_or_shutdown()
}

#[cfg(test)]
pub fn reset_lifecycle_for_test() {
    LIFECYCLE.store(AppLifecycle::Running as u8, Ordering::Release);
}

pub fn cleanup_before_exit<R: Runtime>(app: &AppHandle<R>) {
    if !transition_lifecycle(AppLifecycle::Quitting, AppLifecycle::ShuttingDown)
        && !transition_lifecycle(AppLifecycle::Running, AppLifecycle::ShuttingDown)
    {
        log_lifecycle(LifecycleLevel::Info, "shutdown_cleanup_already_started");
        return;
    }

    log_lifecycle(LifecycleLevel::Info, "shutdown_cleanup_started");

    let _ = app.emit(crate::contract::EVENT_SHUTTING_DOWN, ());

    crate::system::hotkey::unregister_shutdown_hotkey(app);

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

    LIFECYCLE.store(AppLifecycle::Exited as u8, Ordering::Release);
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
        RecordingErrorCode::StopFailed => "shutdown_recording_error_stop_failed",
        RecordingErrorCode::WatchdogTimeout => "shutdown_recording_error_watchdog_timeout",
        RecordingErrorCode::AppShuttingDown => "shutdown_recording_error_app_shutting_down",
        RecordingErrorCode::Internal => "shutdown_recording_error_internal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_default() {
        reset_lifecycle_for_test();
        assert_eq!(current_lifecycle(), AppLifecycle::Running);
    }

    #[test]
    fn running_accepts_commands() {
        reset_lifecycle_for_test();
        assert!(can_accept_commands());
    }

    #[test]
    fn running_to_quitting_succeeds() {
        reset_lifecycle_for_test();
        assert!(transition_lifecycle(
            AppLifecycle::Running,
            AppLifecycle::Quitting
        ));
        assert_eq!(current_lifecycle(), AppLifecycle::Quitting);
    }

    #[test]
    fn running_to_shutting_down_succeeds() {
        reset_lifecycle_for_test();
        assert!(transition_lifecycle(
            AppLifecycle::Running,
            AppLifecycle::ShuttingDown
        ));
        assert_eq!(current_lifecycle(), AppLifecycle::ShuttingDown);
    }

    #[test]
    fn quitting_to_shutting_down_succeeds() {
        reset_lifecycle_for_test();
        assert!(transition_lifecycle(
            AppLifecycle::Running,
            AppLifecycle::Quitting
        ));
        assert!(transition_lifecycle(
            AppLifecycle::Quitting,
            AppLifecycle::ShuttingDown
        ));
        assert_eq!(current_lifecycle(), AppLifecycle::ShuttingDown);
    }

    #[test]
    fn full_cycle() {
        reset_lifecycle_for_test();
        assert!(transition_lifecycle(
            AppLifecycle::Running,
            AppLifecycle::Quitting
        ));
        assert!(transition_lifecycle(
            AppLifecycle::Quitting,
            AppLifecycle::ShuttingDown
        ));
        LIFECYCLE.store(AppLifecycle::Exited as u8, Ordering::Release);
        assert_eq!(current_lifecycle(), AppLifecycle::Exited);
    }

    #[test]
    fn double_transition_fails() {
        reset_lifecycle_for_test();
        assert!(transition_lifecycle(
            AppLifecycle::Running,
            AppLifecycle::Quitting
        ));
        assert!(!transition_lifecycle(
            AppLifecycle::Running,
            AppLifecycle::Quitting
        ));
    }

    #[test]
    fn quitting_rejects_commands() {
        reset_lifecycle_for_test();
        assert!(transition_lifecycle(
            AppLifecycle::Running,
            AppLifecycle::Quitting
        ));
        assert!(!can_accept_commands());
    }

    #[test]
    fn shutting_down_rejects_commands() {
        reset_lifecycle_for_test();
        assert!(transition_lifecycle(
            AppLifecycle::Running,
            AppLifecycle::ShuttingDown
        ));
        assert!(!can_accept_commands());
    }

    #[test]
    fn exited_rejects_commands() {
        reset_lifecycle_for_test();
        LIFECYCLE.store(AppLifecycle::Exited as u8, Ordering::Release);
        assert!(!can_accept_commands());
    }

    #[test]
    fn is_quitting_defaults_to_false() {
        reset_lifecycle_for_test();
        assert!(!is_quitting());
    }

    #[test]
    fn mark_quitting_shows_as_quitting() {
        reset_lifecycle_for_test();
        assert!(!is_quitting());
        transition_lifecycle(AppLifecycle::Running, AppLifecycle::Quitting);
        assert!(is_quitting());
    }

    #[test]
    fn reverse_transition_fails() {
        reset_lifecycle_for_test();
        assert!(transition_lifecycle(
            AppLifecycle::Running,
            AppLifecycle::Quitting
        ));
        assert!(!transition_lifecycle(
            AppLifecycle::Quitting,
            AppLifecycle::Running
        ));
    }
}
