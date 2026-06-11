use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;

pub const GROQ_WHISPER_PROVIDER_NAME: &str = "groq_whisper";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttProvider {
    GroqWhisper,
}

impl SttProvider {
    pub fn from_name(_name: &str) -> Self {
        Self::GroqWhisper
    }
}

/// Common interface for STT transcription providers.
#[async_trait]
pub trait SttTranscriptionClient: Send + Sync {
    /// Unique provider identifier (e.g. `GROQ_WHISPER_PROVIDER_NAME`).
    fn provider_name(&self) -> &'static str;

    /// Transcribe WAV audio bytes and return a provider-agnostic result.
    async fn transcribe(
        &self,
        api_key: &str,
        wav_bytes: Vec<u8>,
        audio_duration_ms: u64,
    ) -> Result<SttProviderTranscription, SttProviderFailure>;
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SttProviderDiagnostics {
    pub provider_name: String,
    pub audio_duration_ms: u64,
    pub transcription_ms: u64,
    pub realtime_factor: f64,
    pub fallback_used: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SttProviderTranscription {
    pub text: String,
    pub model: String,
    pub diagnostics: SttProviderDiagnostics,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SttProviderFailure {
    pub diagnostics: SttProviderDiagnostics,
}

pub fn elapsed_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

pub fn realtime_factor(transcription_ms: u64, audio_duration_ms: u64) -> f64 {
    if audio_duration_ms == 0 {
        return 0.0;
    }

    let value = transcription_ms as f64 / audio_duration_ms as f64;
    (value * 1000.0).round() / 1000.0
}

pub fn sanitize_error_code(code: &str) -> String {
    let lower = code.trim().to_ascii_lowercase();
    if lower.contains("bearer")
        || lower.contains("authorization")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("gsk_")
    {
        return "internal".to_string();
    }

    let sanitized = code
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() || sanitized.len() > 64 {
        "internal".to_string()
    } else {
        sanitized
    }
}

impl SttProviderDiagnostics {
    pub fn success(
        provider_name: &'static str,
        audio_duration_ms: u64,
        transcription_ms: u64,
    ) -> Self {
        Self {
            provider_name: provider_name.to_string(),
            audio_duration_ms,
            transcription_ms,
            realtime_factor: realtime_factor(transcription_ms, audio_duration_ms),
            fallback_used: false,
            error_code: None,
        }
    }

    pub fn failure(
        provider_name: &'static str,
        audio_duration_ms: u64,
        transcription_ms: u64,
        error_code: impl Into<String>,
    ) -> Self {
        Self {
            provider_name: provider_name.to_string(),
            audio_duration_ms,
            transcription_ms,
            realtime_factor: realtime_factor(transcription_ms, audio_duration_ms),
            fallback_used: false,
            error_code: Some(sanitize_error_code(&error_code.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        realtime_factor, sanitize_error_code, SttProvider, SttProviderDiagnostics,
        GROQ_WHISPER_PROVIDER_NAME,
    };

    #[test]
    fn provider_selection_defaults_to_groq() {
        assert_eq!(
            SttProvider::from_name(GROQ_WHISPER_PROVIDER_NAME),
            SttProvider::GroqWhisper
        );
        assert_eq!(
            SttProvider::from_name("unknown_provider"),
            SttProvider::GroqWhisper
        );
    }

    #[test]
    fn realtime_factor_is_rounded_and_zero_safe() {
        assert_eq!(realtime_factor(250, 1000), 0.25);
        assert_eq!(realtime_factor(333, 1000), 0.333);
        assert_eq!(realtime_factor(10, 0), 0.0);
    }

    #[test]
    fn safe_error_codes_do_not_preserve_details() {
        assert_eq!(sanitize_error_code("Model Missing"), "model_missing");
        assert_eq!(
            sanitize_error_code("Authorization: Bearer secret"),
            "internal"
        );
        assert_eq!(sanitize_error_code(""), "internal");
    }

    #[test]
    fn failure_diagnostics_identify_groq_and_safe_error() {
        let diagnostics = SttProviderDiagnostics::failure(
            GROQ_WHISPER_PROVIDER_NAME,
            1000,
            400,
            "model missing",
        );

        assert_eq!(diagnostics.provider_name, GROQ_WHISPER_PROVIDER_NAME);
        assert!(!diagnostics.fallback_used);
        assert_eq!(diagnostics.error_code.as_deref(), Some("model_missing"));
    }
}