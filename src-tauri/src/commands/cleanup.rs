use tauri::State;

use crate::{
    cleanup::{cleanup_transcript as cleanup_transcript_impl, TranscriptCleanupResult},
    providers::groq::GroqCleanupClient,
    settings::SettingsManager,
};

#[tauri::command]
pub async fn cleanup_transcript(
    manager: State<'_, SettingsManager>,
    groq_client: State<'_, GroqCleanupClient>,
    transcript: String,
) -> Result<TranscriptCleanupResult, String> {
    Ok(cleanup_transcript_impl(&manager, &*groq_client, transcript).await)
}
