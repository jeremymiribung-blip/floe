use tauri::AppHandle;

use crate::system::overlay;

#[tauri::command]
pub fn bubble_show(app: AppHandle) {
    overlay::position_overlay_bottom_center(&app);
    overlay::show_overlay(&app);
}

#[tauri::command]
pub fn bubble_hide(app: AppHandle) {
    overlay::hide_overlay(&app);
}

#[cfg(test)]
mod tests {
    #[test]
    fn bubble_commands_are_compileable() {
        // Smoke test: the commands exist and accept AppHandle. The runtime
        // path is exercised via Tauri integration tests, not here.
    }
}
