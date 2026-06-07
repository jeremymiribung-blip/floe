use tauri::State;

use crate::{
    asr::{LocalAsrSidecarManager, PcmAudioChunk},
    recording::{RecordingError, RecordingInfo, RecordingManager, RecordingStatus},
};

#[tauri::command]
pub async fn start_recording(
    manager: State<'_, RecordingManager>,
    local_asr: State<'_, LocalAsrSidecarManager>,
) -> Result<RecordingStatus, RecordingError> {
    let sender = local_asr.start_recording_session().await;
    if let Some(sender) = sender {
        manager.set_audio_chunk_emitter(Box::new(move |chunk: PcmAudioChunk| {
            let _ = sender.send(crate::asr::SessionCommand::Chunk(chunk));
        }));
    } else {
        manager.clear_audio_chunk_emitter();
    }

    match manager.start_recording() {
        Ok(status) => Ok(status),
        Err(error) => {
            local_asr.cancel_recording_session();
            manager.clear_audio_chunk_emitter();
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn stop_recording(
    manager: State<'_, RecordingManager>,
    local_asr: State<'_, LocalAsrSidecarManager>,
) -> Result<RecordingInfo, RecordingError> {
    let result = manager.stop_recording();
    manager.clear_audio_chunk_emitter();

    if result.is_ok() {
        local_asr.finish_recording_session().await;
    } else {
        local_asr.cancel_recording_session();
    }

    result
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
