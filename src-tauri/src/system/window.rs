use tauri::{AppHandle, CloseRequestApi, Manager, Runtime, WebviewWindow, Window};

use crate::lifecycle::{is_quitting, log_lifecycle, LifecycleLevel};

pub const MAIN_WINDOW_LABEL: &str = "main";

pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        log_lifecycle(LifecycleLevel::Warn, "main_window_missing");
        return;
    };

    show_and_focus_window(&window);
}

pub fn hide_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        log_lifecycle(LifecycleLevel::Warn, "main_window_missing");
        return;
    };

    if window.hide().is_err() {
        log_lifecycle(LifecycleLevel::Warn, "main_window_hide_failed");
    }
}

pub fn show_and_focus_window<R: Runtime>(window: &WebviewWindow<R>) {
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

pub fn should_hide_on_close(quitting: bool) -> bool {
    !quitting
}

pub fn handle_main_window_close_request<R: Runtime>(window: &Window<R>, api: &CloseRequestApi) {
    if !should_hide_on_close(is_quitting()) {
        log_lifecycle(LifecycleLevel::Info, "app_quitting");
        return;
    }

    api.prevent_close();

    if window.hide().is_err() {
        log_lifecycle(LifecycleLevel::Warn, "main_window_hide_failed");
    } else {
        log_lifecycle(
            LifecycleLevel::Info,
            "window_close_requested_hidden_to_tray",
        );
    }
}

pub fn is_main_window<R: Runtime>(window: &Window<R>) -> bool {
    window.label() == MAIN_WINDOW_LABEL
}

#[cfg(test)]
mod tests {
    use super::should_hide_on_close;

    #[test]
    fn should_hide_on_close_returns_true_when_not_quitting() {
        assert!(should_hide_on_close(false));
    }

    #[test]
    fn should_hide_on_close_returns_false_when_quitting() {
        assert!(!should_hide_on_close(true));
    }

    #[test]
    fn main_window_label_is_main() {
        assert_eq!(super::MAIN_WINDOW_LABEL, "main");
    }
}
