use std::time::Instant;

use tauri::State;

use crate::{
    asr::backend::AsrBackend,
    diag::{DiagEvent, PipelineContext},
    providers::groq::{GroqTranscription, GroqTranscriptionError, GroqTranscriptionErrorCode},
    recording::{RecordingError, RecordingErrorCode, RecordingManager},
};

#[tauri::command]
pub async fn transcribe_latest_recording(
    recording_manager: State<'_, RecordingManager>,
    asr_backend: State<'_, AsrBackend>,
    diag_ctx: State<'_, PipelineContext>,
) -> Result<GroqTranscription, GroqTranscriptionError> {
    let trace_id = diag_ctx.current_trace_id().unwrap_or_default();

    let wav_bytes = latest_wav_bytes(recording_manager.get_latest_recording_wav_bytes()?)?;
    let audio_duration_ms = recording_manager
        .get_latest_recording_info()?
        .map(|info| info.duration_ms)
        .unwrap_or(0);

    log::info!(
        "{}",
        DiagEvent::SttAttempt {
            trace_id: trace_id.clone(),
            attempt: 1,
            audio_duration_ms,
        }
    );

    let start = Instant::now();

    match asr_backend
        .transcribe(wav_bytes, audio_duration_ms, None)
        .await
    {
        Ok(result) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let realtime = if audio_duration_ms > 0 {
                duration_ms as f64 / audio_duration_ms as f64
            } else {
                0.0
            };

            log::info!(
                "{}",
                DiagEvent::SttCompleted {
                    trace_id,
                    duration_ms,
                    model: result.model.clone(),
                    retry_count: result.diagnostics.retry_count,
                    audio_duration_ms,
                    realtime_factor: realtime,
                }
            );

            Ok(GroqTranscription {
                text: result.text,
                model: result.model,
                retry_count: result.diagnostics.retry_count,
                rate_limit: None,
            })
        }
        Err(asr_error) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            let error_code = format!("{:?}", asr_error.code);

            log::error!(
                "{}",
                DiagEvent::SttFailed {
                    trace_id,
                    duration_ms,
                    attempt: asr_error.retry_count + 1,
                    retry_count: asr_error.retry_count,
                    error_code: error_code.clone(),
                }
            );

            Err(GroqTranscriptionError {
                domain: "stt",
                code: map_asr_error_code(asr_error.code),
                message: asr_error.message,
                model: String::new(),
                retry_count: asr_error.retry_count,
                rate_limit: None,
            })
        }
    }
}

fn map_asr_error_code(code: crate::asr::error::AsrErrorCode) -> GroqTranscriptionErrorCode {
    use crate::asr::error::AsrErrorCode;
    match code {
        AsrErrorCode::AudioEmpty => GroqTranscriptionErrorCode::EmptyAudio,
        AsrErrorCode::AudioTooLong | AsrErrorCode::AudioTooLarge => {
            GroqTranscriptionErrorCode::InvalidRequest
        }
        AsrErrorCode::NoProvider
        | AsrErrorCode::ProviderUnhealthy
        | AsrErrorCode::ProviderRejected
        | AsrErrorCode::TranscriptionFailed
        | AsrErrorCode::FallbackFailed
        | AsrErrorCode::SessionTimeout
        | AsrErrorCode::Internal => GroqTranscriptionErrorCode::ServerError,
        AsrErrorCode::ModelNotFound => GroqTranscriptionErrorCode::InvalidRequest,
    }
}

fn latest_wav_bytes(wav_bytes: Option<Vec<u8>>) -> Result<Vec<u8>, GroqTranscriptionError> {
    let Some(wav_bytes) = wav_bytes else {
        return Err(empty_audio_error());
    };

    if wav_bytes.is_empty() {
        return Err(empty_audio_error());
    }

    Ok(wav_bytes)
}

impl From<RecordingError> for GroqTranscriptionError {
    fn from(error: RecordingError) -> Self {
        match error.code {
            RecordingErrorCode::EmptyRecording => empty_audio_error(),
            _ => GroqTranscriptionError::new(
                GroqTranscriptionErrorCode::ServerError,
                "The latest recording could not be loaded.",
            ),
        }
    }
}

fn empty_audio_error() -> GroqTranscriptionError {
    GroqTranscriptionError::new(
        GroqTranscriptionErrorCode::EmptyAudio,
        "Record audio before requesting a transcription.",
    )
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
            latest_wav_bytes(Some(vec![1, 2, 3])).unwrap(),
            vec![1, 2, 3]
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
