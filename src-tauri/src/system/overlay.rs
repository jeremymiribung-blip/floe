use tauri::{AppHandle, LogicalPosition, Manager, Runtime, Window};

use crate::lifecycle::{log_lifecycle, LifecycleLevel};

pub const OVERLAY_WINDOW_LABEL: &str = "recording-bubble";
const OVERLAY_WIDTH: f64 = 160.0;
const OVERLAY_HEIGHT: f64 = 80.0;
const OVERLAY_BOTTOM_MARGIN: f64 = 48.0;

pub fn show_overlay<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        log_lifecycle(LifecycleLevel::Warn, "overlay_window_missing");
        return;
    };

    if let Err(error) = window.show() {
        log_lifecycle(LifecycleLevel::Warn, "overlay_show_failed");
        let _ = error;
    }
}

pub fn hide_overlay<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };

    if window.hide().is_err() {
        log_lifecycle(LifecycleLevel::Warn, "overlay_hide_failed");
    }
}

pub fn position_overlay_bottom_center<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };

    let monitor = match app.primary_monitor() {
        Ok(Some(monitor)) => monitor,
        _ => {
            log_lifecycle(LifecycleLevel::Warn, "overlay_primary_monitor_missing");
            return;
        }
    };

    let monitor_size = monitor.size();
    let scale_factor = monitor.scale_factor();
    let monitor_pos = monitor.position();

    let work_area_width = monitor_size.width as f64 / scale_factor;
    let work_area_height = monitor_size.height as f64 / scale_factor;
    let work_area_y = monitor_pos.y as f64 / scale_factor;

    let x = (work_area_width - OVERLAY_WIDTH) / 2.0;
    let y = work_area_height + work_area_y - OVERLAY_HEIGHT - OVERLAY_BOTTOM_MARGIN;

    let position = LogicalPosition::new(x.max(0.0), y.max(0.0));
    if window.set_position(position).is_err() {
        log_lifecycle(LifecycleLevel::Warn, "overlay_position_failed");
    }
}

pub fn is_overlay_window<R: Runtime>(window: &Window<R>) -> bool {
    window.label() == OVERLAY_WINDOW_LABEL
}

#[cfg(test)]
mod tests {
    use super::OVERLAY_WINDOW_LABEL;

    #[test]
    fn overlay_label_is_stable() {
        assert_eq!(OVERLAY_WINDOW_LABEL, "recording-bubble");
    }
}
