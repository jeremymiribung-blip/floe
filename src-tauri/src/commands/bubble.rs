use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::recording::{RecordingError, RecordingErrorCode, RecordingManager};
use crate::system::overlay;

#[tauri::command]
pub fn bubble_show(app: AppHandle) -> Result<(), String> {
    log::info!("bubble_show_command called");
    overlay::position_overlay_bottom_center(&app);
    overlay::show_overlay(&app);
    Ok(())
}

#[tauri::command]
pub fn bubble_hide(app: AppHandle) -> Result<(), String> {
    log::info!("bubble_hide_command called");
    overlay::hide_overlay(&app);
    Ok(())
}

#[tauri::command]
pub fn bubble_set_state(app: AppHandle, state: String) {
    overlay::position_overlay_bottom_center(&app);
    overlay::set_overlay_state(&app, &state);
}

/// Cancel the active recording from the bubble overlay.
/// Stops the recording and emits a `recording-bubble-cancelled` event
/// to the main window so the pipeline knows to skip transcription/paste.
#[tauri::command]
pub async fn bubble_cancel_recording(
    app: AppHandle,
    manager: State<'_, RecordingManager>,
) -> Result<(), RecordingError> {
    // Stop the recording if active; ignore if already stopped (race with hotkey release).
    match manager.stop_recording() {
        Ok(_) => {}
        Err(e) if e.code == RecordingErrorCode::NotRecording => {}
        Err(e) => return Err(e),
    }

    // Notify the main window that the recording was cancelled by the user.
    let _ = app.emit_to(
        "main",
        crate::contract::EVENT_BUBBLE_CANCEL,
        BubbleCancelPayload {
            cancelled: true,
            process: false,
        },
    );

    overlay::hide_overlay(&app);

    Ok(())
}

/// Stop the active recording from the bubble overlay and process it.
/// Stops the recording and emits a `recording-bubble-cancelled` event
/// with `process: true` so the pipeline continues with transcription/paste.

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BubbleCancelPayload {
    cancelled: bool,
    process: bool,
}
