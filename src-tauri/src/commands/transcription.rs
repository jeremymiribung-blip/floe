use tauri::State;

use crate::{
    providers::groq::{
        GroqTranscription, GroqTranscriptionClient, GroqTranscriptionError,
        GroqTranscriptionErrorCode,
    },
    recording::{RecordingError, RecordingErrorCode, RecordingManager},
    settings::{SettingsError, SettingsErrorCode, SettingsManager},
};

#[tauri::command]
pub async fn transcribe_latest_recording(
    recording_manager: State<'_, RecordingManager>,
    settings_manager: State<'_, SettingsManager>,
    groq_client: State<'_, GroqTranscriptionClient>,
) -> Result<GroqTranscription, GroqTranscriptionError> {
    let wav_bytes = latest_wav_bytes(recording_manager.get_latest_recording_wav_bytes()?)?;
    let api_key = settings_manager
        .get_groq_api_key_secret()
        .map_err(map_settings_error)?
        .ok_or_else(missing_api_key_error)?;

    groq_client.transcribe_wav(&api_key, wav_bytes).await
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

fn map_settings_error(error: SettingsError) -> GroqTranscriptionError {
    match error.code {
        SettingsErrorCode::SecretStoreUnavailable => GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::MissingApiKey,
            "The Groq API key could not be read from secure storage.",
        ),
        _ => GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::ServerError,
            "Transcription settings could not be loaded.",
        ),
    }
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

fn missing_api_key_error() -> GroqTranscriptionError {
    GroqTranscriptionError::new(
        GroqTranscriptionErrorCode::MissingApiKey,
        "Configure a Groq API key before transcribing.",
    )
}

fn empty_audio_error() -> GroqTranscriptionError {
    GroqTranscriptionError::new(
        GroqTranscriptionErrorCode::EmptyAudio,
        "Record audio before requesting a transcription.",
    )
}

#[cfg(test)]
mod tests {
    use super::{latest_wav_bytes, map_settings_error};
    use crate::{
        providers::groq::{GroqTranscriptionError, GroqTranscriptionErrorCode},
        recording::{RecordingError, RecordingErrorCode},
        settings::{SettingsError, SettingsErrorCode},
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
    fn unavailable_secret_store_maps_to_missing_key() {
        let error = map_settings_error(SettingsError {
            code: SettingsErrorCode::SecretStoreUnavailable,
            message: "unavailable".to_string(),
        });

        assert_eq!(error.code, GroqTranscriptionErrorCode::MissingApiKey);
    }

    #[test]
    fn non_secret_settings_errors_map_to_server_error() {
        let error = map_settings_error(SettingsError {
            code: SettingsErrorCode::AppSettingsUnavailable,
            message: "settings failed".to_string(),
        });

        assert_eq!(error.code, GroqTranscriptionErrorCode::ServerError);
        assert!(!error.message.contains("settings failed"));
    }

    #[test]
    fn recording_errors_map_to_transcription_errors_without_details() {
        let empty: GroqTranscriptionError = RecordingError {
            code: RecordingErrorCode::EmptyRecording,
            message: "raw recording detail".to_string(),
        }
        .into();
        let internal: GroqTranscriptionError = RecordingError {
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
