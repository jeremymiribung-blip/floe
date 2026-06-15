use std::time::Instant;

use tauri::State;

use crate::{
    cleanup::{cleanup_transcript as cleanup_transcript_impl, TranscriptCleanupResult},
    diag::{DiagEvent, PipelineContext},
    providers::groq::{GroqCleanupClient, GroqCleanupError},
    settings::SettingsManager,
};

#[tauri::command]
pub async fn cleanup_transcript(
    manager: State<'_, SettingsManager>,
    groq_client: State<'_, GroqCleanupClient>,
    diag_ctx: State<'_, PipelineContext>,
    transcript: String,
) -> Result<TranscriptCleanupResult, GroqCleanupError> {
    let trace_id = diag_ctx.current_trace_id().unwrap_or_default();
    let transcript_len = transcript.len() as u32;

    log::info!(
        "{}",
        DiagEvent::CleanupAttempt {
            trace_id: trace_id.clone(),
            attempt: 1,
            transcript_len,
        }
    );

    let start = Instant::now();
    let result = cleanup_transcript_impl(&manager, &*groq_client, transcript).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    if result.fallback_used {
        let error_code = result
            .error_code
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        log::warn!(
            "{}",
            DiagEvent::CleanupFallback {
                trace_id,
                error_code,
            }
        );
    } else {
        log::info!(
            "{}",
            DiagEvent::CleanupCompleted {
                trace_id,
                duration_ms,
                model: result.model.clone(),
                retry_count: result.retry_count,
                validation_ms: result.validation_ms,
            }
        );
    }

    Ok(result)
}
