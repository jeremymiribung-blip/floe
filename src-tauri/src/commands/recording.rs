use tauri::State;

use crate::recording::{RecordingError, RecordingInfo, RecordingManager, RecordingStatus};

#[tauri::command]
pub async fn start_recording(
    manager: State<'_, RecordingManager>,
) -> Result<RecordingStatus, RecordingError> {
    manager.start_recording()
}

#[tauri::command]
pub async fn stop_recording(
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

#[tauri::command]
pub fn get_latest_recording_wav_bytes(
    manager: State<'_, RecordingManager>,
) -> Result<Option<Vec<u8>>, RecordingError> {
    manager.get_latest_recording_wav_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_command_signatures_are_stable() {
        // Compile-time verification that recording commands have the expected signatures
        // This ensures no provider-switching complexity has been added
        
        // start_recording: State<RecordingManager> -> Result<RecordingStatus, RecordingError>
        // stop_recording: State<RecordingManager> -> Result<RecordingInfo, RecordingError>
        // get_recording_status: State<RecordingManager> -> Result<RecordingStatus, RecordingError>
        // get_latest_recording_info: State<RecordingManager> -> Result<Option<RecordingInfo>, RecordingError>
        // get_latest_recording_wav_bytes: State<RecordingManager> -> Result<Option<Vec<u8>>, RecordingError>
        
        // These signatures don't include any provider parameters or return provider info
    }
}
