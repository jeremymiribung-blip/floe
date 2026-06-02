use tauri::State;

use crate::recording::{RecordingError, RecordingInfo, RecordingManager, RecordingStatus};

#[tauri::command]
pub fn start_recording(
    manager: State<'_, RecordingManager>,
) -> Result<RecordingStatus, RecordingError> {
    manager.start_recording()
}

#[tauri::command]
pub fn stop_recording(
    manager: State<'_, RecordingManager>,
) -> Result<RecordingInfo, RecordingError> {
    manager.stop_recording()
}

#[tauri::command]
pub fn get_recording_status(
    manager: State<'_, RecordingManager>,
) -> Result<RecordingStatus, RecordingError> {
    manager.get_recording_status()
}

#[tauri::command]
pub fn get_latest_recording_info(
    manager: State<'_, RecordingManager>,
) -> Result<Option<RecordingInfo>, RecordingError> {
    manager.get_latest_recording_info()
}
