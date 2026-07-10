use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::{
    lifecycle::{log_lifecycle, LifecycleLevel},
    system::window::MAIN_WINDOW_LABEL,
};

pub const PRIMARY_STARTED_EVENT: &str = "single_instance_primary_started";
pub const SECONDARY_LAUNCH_DETECTED_EVENT: &str = "single_instance_secondary_launch_detected";
pub const FOCUS_EXISTING_WINDOW_EVENT: &str = "single_instance_focus_existing_window";
pub const FOCUS_FAILED_EVENT: &str = "single_instance_focus_failed";

pub fn log_primary_started() {
    log_lifecycle(LifecycleLevel::Info, PRIMARY_STARTED_EVENT);
}

pub fn handle_secondary_launch<R: Runtime>(app: &AppHandle<R>) {
    log_lifecycle(LifecycleLevel::Info, SECONDARY_LAUNCH_DETECTED_EVENT);

    // Emit an event so the frontend can react (e.g. focus settings or show a toast)
    // before or instead of the default window focus behavior.
    let _ = app.emit("single-instance-triggered", ());

    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        log_lifecycle(LifecycleLevel::Warn, FOCUS_FAILED_EVENT);
        return;
    };

    let mut ok = true;
    if window.show().is_err() {
        ok = false;
    }
    let _ = window.unminimize();
    if window.set_focus().is_err() {
        ok = false;
    }

    if ok {
        log_lifecycle(LifecycleLevel::Info, FOCUS_EXISTING_WINDOW_EVENT);
    } else {
        log_lifecycle(LifecycleLevel::Warn, FOCUS_FAILED_EVENT);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FOCUS_EXISTING_WINDOW_EVENT, FOCUS_FAILED_EVENT, PRIMARY_STARTED_EVENT,
        SECONDARY_LAUNCH_DETECTED_EVENT,
    };

    #[test]
    fn lifecycle_event_names_are_stable() {
        assert_eq!(PRIMARY_STARTED_EVENT, "single_instance_primary_started");
        assert_eq!(
            SECONDARY_LAUNCH_DETECTED_EVENT,
            "single_instance_secondary_launch_detected"
        );
        assert_eq!(
            FOCUS_EXISTING_WINDOW_EVENT,
            "single_instance_focus_existing_window"
        );
        assert_eq!(FOCUS_FAILED_EVENT, "single_instance_focus_failed");
    }
}
