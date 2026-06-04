use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Monitor, Runtime, Window};

use crate::lifecycle::{log_lifecycle, LifecycleLevel};

pub const OVERLAY_WINDOW_LABEL: &str = "recording-bubble";
const OVERLAY_STATE_EVENT: &str = "recording-bubble-state";
const OVERLAY_WIDTH: f64 = 170.0;
const OVERLAY_HEIGHT: f64 = 48.0;
const OVERLAY_BOTTOM_MARGIN: f64 = 64.0;

pub fn show_overlay<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        log_lifecycle(LifecycleLevel::Warn, "overlay_window_missing");
        return;
    };

    emit_overlay_state(app, true);

    if let Err(error) = window.show() {
        log_lifecycle(LifecycleLevel::Warn, "overlay_show_failed");
        let _ = error;
    }

    emit_overlay_state(app, true);
}

pub fn hide_overlay<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };

    emit_overlay_state(app, false);

    if window.hide().is_err() {
        log_lifecycle(LifecycleLevel::Warn, "overlay_hide_failed");
    }
}

pub fn position_overlay_bottom_center<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(OVERLAY_WINDOW_LABEL) else {
        return;
    };

    let monitor = match preferred_overlay_monitor(app, &window) {
        Some(monitor) => monitor,
        None => {
            log_lifecycle(LifecycleLevel::Warn, "overlay_monitor_missing");
            return;
        }
    };

    let work_area = logical_work_area(&monitor);
    let position = calculate_bottom_center_position(
        work_area,
        LogicalSize {
            width: OVERLAY_WIDTH,
            height: OVERLAY_HEIGHT,
        },
        OVERLAY_BOTTOM_MARGIN,
    );

    if window.set_position(position).is_err() {
        log_lifecycle(LifecycleLevel::Warn, "overlay_position_failed");
    }
}

pub fn is_overlay_window<R: Runtime>(window: &Window<R>) -> bool {
    window.label() == OVERLAY_WINDOW_LABEL
}

fn preferred_overlay_monitor<R: Runtime>(
    app: &AppHandle<R>,
    window: &tauri::WebviewWindow<R>,
) -> Option<Monitor> {
    if let Ok(cursor_position) = app.cursor_position() {
        if let Ok(Some(monitor)) = app.monitor_from_point(cursor_position.x, cursor_position.y) {
            return Some(monitor);
        }
    }

    if let Ok(Some(monitor)) = window.current_monitor() {
        return Some(monitor);
    }

    match app.primary_monitor() {
        Ok(Some(monitor)) => Some(monitor),
        _ => None,
    }
}

fn emit_overlay_state<R: Runtime>(app: &AppHandle<R>, recording: bool) {
    if app
        .emit_to(
            OVERLAY_WINDOW_LABEL,
            OVERLAY_STATE_EVENT,
            OverlayStatePayload { recording },
        )
        .is_err()
    {
        log_lifecycle(LifecycleLevel::Warn, "overlay_state_emit_failed");
    }
}

fn logical_work_area(monitor: &Monitor) -> LogicalRect {
    let work_area = monitor.work_area();
    let scale_factor = monitor.scale_factor();

    LogicalRect {
        x: work_area.position.x as f64 / scale_factor,
        y: work_area.position.y as f64 / scale_factor,
        width: work_area.size.width as f64 / scale_factor,
        height: work_area.size.height as f64 / scale_factor,
    }
}

fn calculate_bottom_center_position(
    work_area: LogicalRect,
    overlay_size: LogicalSize,
    bottom_margin: f64,
) -> LogicalPosition<f64> {
    LogicalPosition::new(
        work_area.x + (work_area.width - overlay_size.width) / 2.0,
        work_area.y + work_area.height - bottom_margin - overlay_size.height,
    )
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayStatePayload {
    recording: bool,
}

#[derive(Debug, Clone, Copy)]
struct LogicalRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy)]
struct LogicalSize {
    width: f64,
    height: f64,
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_bottom_center_position, LogicalRect, LogicalSize, OVERLAY_BOTTOM_MARGIN,
        OVERLAY_HEIGHT, OVERLAY_STATE_EVENT, OVERLAY_WIDTH, OVERLAY_WINDOW_LABEL,
    };

    #[test]
    fn overlay_label_is_stable() {
        assert_eq!(OVERLAY_WINDOW_LABEL, "recording-bubble");
    }

    #[test]
    fn overlay_state_event_is_stable() {
        assert_eq!(OVERLAY_STATE_EVENT, "recording-bubble-state");
    }

    #[test]
    fn calculates_bottom_center_on_primary_monitor() {
        let position = calculate_bottom_center_position(
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            LogicalSize {
                width: OVERLAY_WIDTH,
                height: OVERLAY_HEIGHT,
            },
            OVERLAY_BOTTOM_MARGIN,
        );

        assert_eq!(position.x, 875.0);
        assert_eq!(position.y, 968.0);
    }

    #[test]
    fn respects_work_area_offsets() {
        let position = calculate_bottom_center_position(
            LogicalRect {
                x: 100.0,
                y: 40.0,
                width: 1280.0,
                height: 900.0,
            },
            LogicalSize {
                width: OVERLAY_WIDTH,
                height: OVERLAY_HEIGHT,
            },
            OVERLAY_BOTTOM_MARGIN,
        );

        assert_eq!(position.x, 655.0);
        assert_eq!(position.y, 828.0);
    }

    #[test]
    fn preserves_negative_monitor_coordinates() {
        let position = calculate_bottom_center_position(
            LogicalRect {
                x: -1920.0,
                y: -120.0,
                width: 1920.0,
                height: 1080.0,
            },
            LogicalSize {
                width: OVERLAY_WIDTH,
                height: OVERLAY_HEIGHT,
            },
            OVERLAY_BOTTOM_MARGIN,
        );

        assert_eq!(position.x, -1045.0);
        assert_eq!(position.y, 848.0);
    }

    #[test]
    fn uses_expected_bottom_margin() {
        const { assert!(OVERLAY_BOTTOM_MARGIN == 64.0) };
    }

    #[test]
    fn bubble_sits_below_prior_baseline() {
        const {
            assert!(
                OVERLAY_BOTTOM_MARGIN < 80.0,
                "bubble must sit clearly lower than the prior 96px baseline",
            );
            assert!(
                OVERLAY_BOTTOM_MARGIN >= 32.0,
                "bubble must not clip into the screen edge",
            );
        };
    }
}
