use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;

use crate::asr::error::SessionError;
use crate::asr::traits::{AsrProvider, AsrSession, TranscriptionError, TranscriptionErrorCode};
use crate::asr::types::{
    AsrDiagnostics, AudioChunk, BackendType, Deployment, HealthStatus, ModelSpec,
    ProviderCapabilities, SessionConfig, StreamingSupport, TranscriptResult,
};
use crate::providers::groq::stt::GroqTranscriptionClient;
use crate::providers::groq::{GroqTranscriptionError, GroqTranscriptionErrorCode, GROQ_STT_MODEL};
use crate::recording::encode_pcm16_wav;

#[derive(Debug)]
pub struct GroqAdapter {
    http_client: reqwest::Client,
    api_key: String,
    client: GroqTranscriptionClient,
}

impl GroqAdapter {
    pub fn new(http_client: reqwest::Client, api_key: String) -> Self {
        let client = GroqTranscriptionClient::new(http_client.clone());
        Self {
            http_client,
            api_key,
            client,
        }
    }
}

const GROQ_MODELS: &[ModelSpec] = &[ModelSpec {
    id: "whisper-large-v3-turbo",
    name: "Whisper Large v3 Turbo",
    requires_gpu: false,
    max_duration_secs: 180,
    supported_languages: None,
    parameters: Some("809M"),
}];

#[async_trait]
impl AsrProvider for GroqAdapter {
    fn id(&self) -> &'static str {
        "groq"
    }

    fn name(&self) -> &'static str {
        "Groq Cloud"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            backend_type: BackendType::Cloud,
            deployment: Deployment::Cloud,
            streaming: StreamingSupport::Full,
            partials: false,
            timestamps: false,
            gpu_required: false,
            fallback_compatible: true,
            max_audio_seconds: 180,
            supported_sample_rates: vec![16_000],
            min_audio_bytes: 1,
            max_audio_bytes: 25_000_000,
        }
    }

    fn default_model(&self) -> &'static str {
        "whisper-large-v3-turbo"
    }

    fn available_models(&self) -> &[ModelSpec] {
        GROQ_MODELS
    }

    async fn create_session(
        &self,
        _config: SessionConfig,
    ) -> Result<Box<dyn AsrSession>, SessionError> {
        if self.api_key.is_empty() {
            return Err(SessionError::new(
                super::super::error::SessionErrorCode::InvalidConfig,
                "Groq API key is not configured",
            ));
        }
        Ok(Box::new(GroqSession::new(
            self.client.clone(),
            self.api_key.clone(),
        )))
    }

    async fn health_check(&self) -> Result<HealthStatus, ()> {
        if self.api_key.is_empty() {
            return Ok(HealthStatus::Unhealthy("no api key configured".into()));
        }
        let response = self
            .http_client
            .get("https://api.groq.com/openai/v1/models")
            .bearer_auth(&self.api_key)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => Ok(HealthStatus::Healthy),
            Ok(resp) if resp.status().is_server_error() => {
                Ok(HealthStatus::Degraded("groq server error".into()))
            }
            Ok(_) => Ok(HealthStatus::Unhealthy("groq rejected key".into())),
            Err(_) => Ok(HealthStatus::Unhealthy("groq unreachable".into())),
        }
    }
}

#[derive(Debug)]
pub struct GroqSession {
    client: GroqTranscriptionClient,
    api_key: String,
    audio_data: Mutex<Vec<f32>>,
    sample_rate: AtomicU32,
}

impl GroqSession {
    pub fn new(client: GroqTranscriptionClient, api_key: String) -> Self {
        Self {
            client,
            api_key,
            audio_data: Mutex::new(Vec::new()),
            sample_rate: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl AsrSession for GroqSession {
    fn model(&self) -> &str {
        GROQ_STT_MODEL
    }

    fn provider_id(&self) -> &'static str {
        "groq"
    }

    async fn submit_audio(&self, chunk: AudioChunk) -> Result<(), ()> {
        self.sample_rate.store(chunk.sample_rate, Ordering::SeqCst);
        let mut data = self.audio_data.lock().map_err(|_| ())?;
        data.extend_from_slice(&chunk.data);
        Ok(())
    }

    async fn partial_transcript(&self) -> Option<String> {
        None
    }

    async fn finalize(self: Box<Self>) -> Result<TranscriptResult, TranscriptionError> {
        let samples = {
            let mut data = self.audio_data.lock().map_err(|_| {
                TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    "failed to lock audio buffer",
                    false,
                )
            })?;
            std::mem::take(&mut *data)
        };

        if samples.is_empty() {
            return Err(TranscriptionError::new(
                TranscriptionErrorCode::InvalidRequest,
                "no audio data to transcribe",
                false,
            ));
        }

        let sample_rate = self.sample_rate.load(Ordering::SeqCst).max(16000);
        let wav_bytes = encode_pcm16_wav(&samples, sample_rate, 1).map_err(|_| {
            TranscriptionError::new(
                TranscriptionErrorCode::Internal,
                "failed to encode WAV audio",
                false,
            )
        })?;
        let started = std::time::Instant::now();

        match self.client.transcribe_wav(&self.api_key, wav_bytes).await {
            Ok(transcription) => {
                let transcription_ms = started.elapsed().as_millis() as u64;
                Ok(TranscriptResult {
                    text: transcription.text,
                    model: transcription.model,
                    diagnostics: AsrDiagnostics::new(
                        "groq",
                        GROQ_STT_MODEL,
                        BackendType::Cloud,
                        0,
                        transcription_ms,
                        "",
                    )
                    .with_retry(transcription.retry_count),
                })
            }
            Err(err) => Err(TranscriptionError::from(err)),
        }
    }

    async fn cancel(self: Box<Self>) {}
}

impl From<GroqTranscriptionError> for TranscriptionError {
    fn from(err: GroqTranscriptionError) -> Self {
        let code = match err.code {
            GroqTranscriptionErrorCode::MissingApiKey
            | GroqTranscriptionErrorCode::InvalidApiKey => TranscriptionErrorCode::InvalidAuth,
            GroqTranscriptionErrorCode::RateLimit => TranscriptionErrorCode::RateLimit,
            GroqTranscriptionErrorCode::Timeout => TranscriptionErrorCode::Timeout,
            GroqTranscriptionErrorCode::ApiUnreachable => TranscriptionErrorCode::ApiUnreachable,
            GroqTranscriptionErrorCode::MalformedResponse => {
                TranscriptionErrorCode::MalformedResponse
            }
            GroqTranscriptionErrorCode::UnsupportedAudio => {
                TranscriptionErrorCode::UnsupportedAudio
            }
            GroqTranscriptionErrorCode::InvalidRequest | GroqTranscriptionErrorCode::EmptyAudio => {
                TranscriptionErrorCode::InvalidRequest
            }
            GroqTranscriptionErrorCode::ServerError => TranscriptionErrorCode::ServerError,
        };
        let retryable = matches!(
            err.code,
            GroqTranscriptionErrorCode::RateLimit
                | GroqTranscriptionErrorCode::Timeout
                | GroqTranscriptionErrorCode::ApiUnreachable
                | GroqTranscriptionErrorCode::ServerError
        );
        TranscriptionError::new(code, err.message, retryable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groq_adapter_identity() {
        let client = reqwest::Client::new();
        let adapter = GroqAdapter::new(client, "test_key".into());
        assert_eq!(adapter.id(), "groq");
        assert_eq!(adapter.name(), "Groq Cloud");
    }

    #[test]
    fn groq_capabilities_are_cloud_fallback_compatible() {
        let client = reqwest::Client::new();
        let adapter = GroqAdapter::new(client, "test_key".into());
        let caps = adapter.capabilities();
        assert_eq!(caps.backend_type, BackendType::Cloud);
        assert!(caps.fallback_compatible);
        assert!(!caps.partials);
        assert!(!caps.gpu_required);
    }

    #[test]
    fn groq_models_include_whisper_turbo() {
        let client = reqwest::Client::new();
        let adapter = GroqAdapter::new(client, "test_key".into());
        let models = adapter.available_models();
        assert!(!models.is_empty());
        assert_eq!(adapter.default_model(), "whisper-large-v3-turbo");
    }

    #[tokio::test]
    async fn groq_adapter_rejects_empty_api_key() {
        let client = reqwest::Client::new();
        let adapter = GroqAdapter::new(client, String::new());
        let result = adapter.health_check().await;
        assert!(result.is_ok());
    }

    #[test]
    fn pcm16_wav_via_encode_pcm16_has_correct_header() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let wav = encode_pcm16_wav(&samples, 16000, 1).expect("wav encoding should succeed");

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");

        let channels = u16::from_le_bytes([wav[22], wav[23]]);
        assert_eq!(channels, 1);

        let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
        assert_eq!(sample_rate, 16000);

        let bits_per_sample = u16::from_le_bytes([wav[34], wav[35]]);
        assert_eq!(bits_per_sample, 16);

        assert_eq!(&wav[36..40], b"data");

        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size, 10);

        assert_eq!(wav.len(), 44 + 10);
    }

    #[test]
    fn pcm16_wav_via_encode_pcm16_has_correct_sample_values() {
        let samples = vec![0.0f32, 1.0, -1.0];
        let wav = encode_pcm16_wav(&samples, 16000, 1).expect("wav encoding should succeed");

        let first = i16::from_le_bytes([wav[44], wav[45]]);
        assert_eq!(first, 0);

        let second = i16::from_le_bytes([wav[46], wav[47]]);
        assert_eq!(second, i16::MAX);

        let third = i16::from_le_bytes([wav[48], wav[49]]);
        assert_eq!(third, i16::MIN);
    }

    #[test]
    fn transcription_error_from_groq_error_maps_correctly() {
        use crate::providers::groq::GroqTranscriptionErrorCode;

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::InvalidApiKey,
            "key rejected",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::InvalidAuth);
        assert!(!err.retryable);

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::RateLimit,
            "rate limited",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::RateLimit);
        assert!(err.retryable);

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::ServerError,
            "server error",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::ServerError);
        assert!(err.retryable);

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::MissingApiKey,
            "missing key",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::InvalidAuth);

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::UnsupportedAudio,
            "unsupported",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::UnsupportedAudio);
        assert!(!err.retryable);

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::Timeout,
            "timeout",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::Timeout);
        assert!(err.retryable);

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::ApiUnreachable,
            "unreachable",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::ApiUnreachable);
        assert!(err.retryable);

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::MalformedResponse,
            "malformed",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::MalformedResponse);

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::InvalidRequest,
            "invalid",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::InvalidRequest);

        let err = TranscriptionError::from(GroqTranscriptionError::new(
            GroqTranscriptionErrorCode::EmptyAudio,
            "empty",
        ));
        assert_eq!(err.code, TranscriptionErrorCode::InvalidRequest);
    }
}
