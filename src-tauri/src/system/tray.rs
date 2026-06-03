use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager, Runtime,
};

use crate::{
    lifecycle::{log_lifecycle, mark_quitting, LifecycleLevel},
    system::window::{hide_main_window, show_main_window},
};

const TRAY_SHOW_ID: &str = "tray-show-floe";
const TRAY_HIDE_ID: &str = "tray-hide-floe";
const TRAY_SETTINGS_ID: &str = "tray-settings";
const TRAY_QUIT_ID: &str = "tray-quit";
const SETTINGS_EVENT: &str = "floe-show-settings";
const TRAY_TOOLTIP: &str = "Floe";

pub fn setup_tray(app: &mut App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, TRAY_SHOW_ID, "Show Floe", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, TRAY_HIDE_ID, "Hide Floe", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, TRAY_SETTINGS_ID, "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_QUIT_ID, "Quit", true, None::<&str>)?;
    let separator_top = PredefinedMenuItem::separator(app)?;
    let separator_bottom = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &hide,
            &separator_top,
            &settings,
            &separator_bottom,
            &quit,
        ],
    )?;
    let mut tray = TrayIconBuilder::new()
        .tooltip(TRAY_TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_SHOW_ID => handle_show(app),
            TRAY_HIDE_ID => handle_hide(app),
            TRAY_SETTINGS_ID => handle_settings(app),
            TRAY_QUIT_ID => handle_quit(app),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    log_lifecycle(LifecycleLevel::Info, "tray_ready");

    Ok(())
}

fn handle_show<R: Runtime>(app: &AppHandle<R>) {
    log_lifecycle(LifecycleLevel::Info, "tray_show_requested");
    show_main_window(app);
}

fn handle_hide<R: Runtime>(app: &AppHandle<R>) {
    log_lifecycle(LifecycleLevel::Info, "tray_hide_requested");
    hide_main_window(app);
}

fn handle_settings<R: Runtime>(app: &AppHandle<R>) {
    show_main_window(app);

    if let Some(window) = app.get_webview_window(crate::system::window::MAIN_WINDOW_LABEL) {
        if window.emit(SETTINGS_EVENT, ()).is_err() {
            log_lifecycle(LifecycleLevel::Warn, "settings_event_emit_failed");
        }
    }
}

fn handle_quit<R: Runtime>(app: &AppHandle<R>) {
    log_lifecycle(LifecycleLevel::Info, "tray_quit_requested");
    mark_quitting();
    app.exit(0);
}

#[cfg(test)]
mod tests {
    #[test]
    fn tray_menu_ids_are_stable() {
        assert_eq!(super::TRAY_SHOW_ID, "tray-show-floe");
        assert_eq!(super::TRAY_HIDE_ID, "tray-hide-floe");
        assert_eq!(super::TRAY_SETTINGS_ID, "tray-settings");
        assert_eq!(super::TRAY_QUIT_ID, "tray-quit");
        assert_eq!(super::SETTINGS_EVENT, "floe-show-settings");
        assert_eq!(super::TRAY_TOOLTIP, "Floe");
    }
}
