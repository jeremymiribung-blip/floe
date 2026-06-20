use std::time::Instant;

use tauri::State;

use crate::{
    contract::MAX_RECORDING_DURATION_SECS,
    diag::{DiagEvent, LastSessionStore, PipelineContext},
    providers::groq::stt::GroqTranscriptionClient,
    providers::groq::{GroqTranscription, GroqTranscriptionError, GroqTranscriptionErrorCode},
    recording::{RecordingError, RecordingErrorCode, RecordingManager, WAV_HEADER_LEN},
    settings::SettingsManager,
};

const MAX_AUDIO_BYTES: u64 = 25_000_000;

#[tauri::command]
pub async fn transcribe_latest_recording(
    recording_manager: State<'_, RecordingManager>,
    settings_manager: State<'_, SettingsManager>,
    groq_client: State<'_, GroqTranscriptionClient>,
    diag_ctx: State<'_, PipelineContext>,
    last_session: State<'_, LastSessionStore>,
) -> Result<GroqTranscription, GroqTranscriptionError> {
    let trace_id = diag_ctx.current_trace_id().unwrap_or_default();

    let wav_bytes = latest_wav_bytes(recording_manager.get_latest_recording_wav_bytes()?)?;
    let audio_duration_ms = recording_manager
        .get_latest_recording_info()?
        .map(|info| info.duration_ms)
        .unwrap_or(0);

    validate_audio(audio_duration_ms, wav_bytes.len() as u64)?;

    log::info!(
        "{}",
        DiagEvent::SttAttempt {
            trace_id: trace_id.clone(),
            attempt: 1,
            audio_duration_ms,
        }
    );

    let api_key = settings_manager
        .get_api_key_secret()
        .ok()
        .flatten()
        .unwrap_or_default();

    let start = Instant::now();

    match groq_client.transcribe_wav(&api_key, wav_bytes).await {
        Ok(transcription) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let realtime = if audio_duration_ms > 0 {
                duration_ms as f64 / audio_duration_ms as f64
            } else {
                0.0
            };

            log::info!(
                "{}",
                DiagEvent::SttCompleted {
                    trace_id: trace_id.clone(),
                    duration_ms,
                    model: transcription.model.clone(),
                    retry_count: transcription.retry_count,
                    audio_duration_ms,
                    realtime_factor: realtime,
                }
            );

            let chars = transcription.text.chars().count() as u32;
            let words = transcription.text.split_whitespace().count() as u32;

            let provider_name = "groq".to_string();
            let provider = crate::diag::SttProviderSnapshot {
                provider_name: provider_name.clone(),
                model: transcription.model.clone(),
                audio_duration_ms,
                transcription_ms: duration_ms,
                realtime_factor: realtime,
                fallback_used: false,
                transcript_chars: Some(chars),
                transcript_words: Some(words),
            };

            let rate_limit_map = transcription
                .rate_limit
                .as_ref()
                .map(|rl| crate::diag::rate_limit_to_map(rl));

            let has_retries = transcription.retry_count > 0;

            last_session.update(|snapshot| {
                snapshot.transcription_ms = duration_ms;
                snapshot.transcription_attempts = transcription.retry_count + 1;
                snapshot.stt_model = Some(transcription.model.clone());
                snapshot.stt_provider = Some(provider);
                snapshot.retries.stt = transcription.retry_count;
                snapshot.rate_limit =
                    rate_limit_map.map(|stt_map| crate::diag::RateLimitSnapshot {
                        stt: Some(stt_map),
                        cleanup: None,
                    });
                if has_retries {
                    snapshot.recovery_actions.push(crate::diag::RecoveryAction {
                        stage: "transcription".to_string(),
                        action: "retry_succeeded".to_string(),
                        reason: format!("stt_retried_{}_times", transcription.retry_count),
                    });
                }
            });

            Ok(transcription)
        }
        Err(err) => {
            let duration_ms = start.elapsed().as_millis() as u64;

            log::error!(
                "{}",
                DiagEvent::SttFailed {
                    trace_id: trace_id.clone(),
                    duration_ms,
                    attempt: err.retry_count + 1,
                    retry_count: err.retry_count,
                    error_code: format!("{:?}", err.code),
                }
            );

            let sanitized = format!("{:?}", err.code).to_lowercase();
            let had_retries = err.retry_count > 0;
            last_session.update(|snapshot| {
                snapshot.transcription_ms = duration_ms;
                snapshot.transcription_attempts = err.retry_count + 1;
                snapshot.transcription_error_code = Some(sanitized.clone());
                snapshot.error_stage = Some("stt".to_string());
                snapshot.sanitized_error_code = Some(sanitized.clone());
                snapshot.last_error = Some(crate::diag::LastError {
                    stage: "stt".to_string(),
                    code: sanitized,
                    message: String::new(),
                });
                if had_retries {
                    snapshot.recovery_actions.push(crate::diag::RecoveryAction {
                        stage: "transcription".to_string(),
                        action: "retry_exhausted".to_string(),
                        reason: format!("stt_failed_after_{}_retries", err.retry_count),
                    });
                }
                snapshot.completed = false;
            });

            Err(err)
        }
    }
}

fn validate_audio(audio_duration_ms: u64, bytes: u64) -> Result<(), GroqTranscriptionError> {
    let duration_secs = (audio_duration_ms / 1000).max(1);
    if duration_secs > MAX_RECORDING_DURATION_SECS {
        return Err(GroqTranscriptionError {
            domain: "stt",
            code: GroqTranscriptionErrorCode::InvalidRequest,
            message: format!(
                "Audio too long: {}s (max {}s)",
                duration_secs, MAX_RECORDING_DURATION_SECS
            ),
            model: String::new(),
            retry_count: 0,
            rate_limit: None,
        });
    }
    if bytes > MAX_AUDIO_BYTES {
        return Err(GroqTranscriptionError {
            domain: "stt",
            code: GroqTranscriptionErrorCode::InvalidRequest,
            message: format!(
                "Audio too large: {} bytes (max {} bytes)",
                bytes, MAX_AUDIO_BYTES
            ),
            model: String::new(),
            retry_count: 0,
            rate_limit: None,
        });
    }
    Ok(())
}

fn latest_wav_bytes(wav_bytes: Option<Vec<u8>>) -> Result<Vec<u8>, GroqTranscriptionError> {
    let Some(wav_bytes) = wav_bytes else {
        return Err(empty_audio_error());
    };

    if wav_bytes.len() <= WAV_HEADER_LEN {
        return Err(empty_audio_error());
    }

    Ok(wav_bytes)
}

fn empty_audio_error() -> GroqTranscriptionError {
    GroqTranscriptionError {
        domain: "stt",
        code: GroqTranscriptionErrorCode::EmptyAudio,
        message: "Record audio before requesting a transcription.".to_string(),
        model: String::new(),
        retry_count: 0,
        rate_limit: None,
    }
}

impl From<RecordingError> for GroqTranscriptionError {
    fn from(err: RecordingError) -> Self {
        let code = match err.code {
            RecordingErrorCode::EmptyRecording | RecordingErrorCode::NotRecording => {
                GroqTranscriptionErrorCode::EmptyAudio
            }
            _ => GroqTranscriptionErrorCode::ServerError,
        };
        GroqTranscriptionError {
            domain: "stt",
            code,
            message: "Recording failed".to_string(),
            model: String::new(),
            retry_count: 0,
            rate_limit: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::latest_wav_bytes;
    use crate::{
        providers::groq::{GroqTranscriptionError, GroqTranscriptionErrorCode},
        recording::{RecordingError, RecordingErrorCode},
    };

    #[test]
    fn latest_wav_bytes_requires_completed_audio() {
        let error = latest_wav_bytes(None).expect_err("missing latest recording should fail");
        assert_eq!(error.code, GroqTranscriptionErrorCode::EmptyAudio);

        let error = latest_wav_bytes(Some(Vec::new())).expect_err("empty wav should fail");
        assert_eq!(error.code, GroqTranscriptionErrorCode::EmptyAudio);

        assert_eq!(
            latest_wav_bytes(Some(vec![1u8; 128])).unwrap(),
            vec![1u8; 128]
        );
    }

    #[test]
    fn recording_errors_map_to_transcription_errors_without_details() {
        let empty: GroqTranscriptionError = RecordingError {
            domain: "recording",
            code: RecordingErrorCode::EmptyRecording,
            message: "raw recording detail".to_string(),
        }
        .into();
        let internal: GroqTranscriptionError = RecordingError {
            domain: "recording",
            code: RecordingErrorCode::Internal,
            message: "raw recording detail".to_string(),
        }
        .into();

        assert_eq!(empty.code, GroqTranscriptionErrorCode::EmptyAudio);
        assert_eq!(internal.code, GroqTranscriptionErrorCode::ServerError);
        assert!(!empty.message.contains("raw recording detail"));
        assert!(!internal.message.contains("raw recording detail"));
    }
}
