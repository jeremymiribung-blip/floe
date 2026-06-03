use tauri::State;

use crate::{
    cleanup::{cleanup_transcript as cleanup_transcript_impl, TranscriptCleanupResult},
    settings::SettingsManager,
};

#[tauri::command]
pub fn cleanup_transcript(
    manager: State<'_, SettingsManager>,
    transcript: String,
) -> TranscriptCleanupResult {
    tauri::async_runtime::block_on(cleanup_transcript_impl(&manager, transcript))
}
