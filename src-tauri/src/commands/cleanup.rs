use std::time::Instant;

use tauri::State;

use crate::{
    cleanup::{cleanup_transcript as cleanup_transcript_impl, TranscriptCleanupResult},
    diag::{DiagEvent, LastSessionStore, PipelineContext},
    providers::groq::{GroqCleanupClient, GroqCleanupError},
    settings::SettingsManager,
};

#[tauri::command]
pub async fn cleanup_transcript(
    manager: State<'_, SettingsManager>,
    groq_client: State<'_, GroqCleanupClient>,
    diag_ctx: State<'_, PipelineContext>,
    last_session: State<'_, LastSessionStore>,
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
                trace_id: trace_id.clone(),
                error_code: error_code.clone(),
            }
        );
        last_session.update(|snapshot| {
            snapshot.cleanup_ms = duration_ms;
            snapshot.cleanup_validation_ms = result.validation_ms;
            snapshot.cleanup_fallback_used = true;
            snapshot.cleanup_error_code = Some(error_code.clone());
            snapshot.error_stage = Some("cleanup".to_string());
            snapshot.sanitized_error_code = Some(error_code.clone());
            snapshot.last_error = Some(crate::diag::LastError {
                stage: "cleanup".to_string(),
                code: error_code.clone(),
                message: String::new(),
            });
            snapshot.recovery_actions.push(crate::diag::RecoveryAction {
                stage: "cleanup".to_string(),
                action: "fallback_to_raw_text".to_string(),
                reason: format!("cleanup_provider_failed: {}", error_code),
            });
            snapshot.completed = false;
        });
    } else {
        let chars = result.text.chars().count() as u32;
        let words = result.text.split_whitespace().count() as u32;

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
        let rate_limit_map = result
            .rate_limit
            .as_deref()
            .map(crate::diag::rate_limit_to_map);

        last_session.update(|snapshot| {
            snapshot.cleanup_ms = duration_ms;
            snapshot.cleanup_validation_ms = result.validation_ms;
            snapshot.cleanup_attempts = result.retry_count + 1;
            snapshot.cleanup_model = Some(result.model.clone());
            snapshot.cleanup_chars = Some(chars);
            snapshot.cleanup_words = Some(words);
            snapshot.retries.cleanup = result.retry_count;
            // Merge cleanup rate-limit data with any existing STT rate-limit data.
            if let Some(cl_map) = rate_limit_map {
                let rl =
                    snapshot
                        .rate_limit
                        .get_or_insert(crate::diag::RateLimitSnapshot {
                            stt: None,
                            cleanup: None,
                        });
                rl.cleanup = Some(cl_map);
            }
        });
    }

    Ok(result)
}
