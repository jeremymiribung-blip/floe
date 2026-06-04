use tauri::State;

use crate::{
    cleanup::{cleanup_transcript as cleanup_transcript_impl, TranscriptCleanupResult},
    settings::SettingsManager,
};

#[tauri::command]
pub async fn cleanup_transcript(
    manager: State<'_, SettingsManager>,
    transcript: String,
) -> Result<TranscriptCleanupResult, String> {
    Ok(cleanup_transcript_impl(&manager, transcript).await)
}
