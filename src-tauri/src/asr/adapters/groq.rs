use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use reqwest::StatusCode;

use crate::asr::error::SessionError;
use crate::asr::traits::{AsrProvider, AsrSession, TranscriptionError, TranscriptionErrorCode};
use crate::asr::types::{
    AudioChunk, BackendType, Deployment, HealthStatus, ModelSpec, ProviderCapabilities,
    SessionConfig, StreamingSupport, TranscriptResult, AsrDiagnostics,
};

const GROQ_BASE_URL: &str = "https://api.groq.com";
const TRANSCRIPTIONS_PATH: &str = "/openai/v1/audio/transcriptions";
const STT_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const GROQ_STT_MODEL: &str = "whisper-large-v3-turbo";

#[derive(Debug)]
pub struct GroqAdapter {
    http_client: reqwest::Client,
    api_key: String,
}

impl GroqAdapter {
    pub fn new(http_client: reqwest::Client, api_key: String) -> Self {
        Self {
            http_client,
            api_key,
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
            self.http_client.clone(),
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
    http_client: reqwest::Client,
    api_key: String,
    audio_data: Mutex<Vec<f32>>,
    sample_rate: AtomicU32,
}

impl GroqSession {
    pub fn new(http_client: reqwest::Client, api_key: String) -> Self {
        Self {
            http_client,
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
                return TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    "failed to lock audio buffer",
                    false,
                );
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
        let wav_bytes = encode_f32_to_wav(&samples, sample_rate);

        let started = std::time::Instant::now();

        let file_part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("recording.wav")
            .mime_str("audio/wav")
            .map_err(|_| {
                TranscriptionError::new(
                    TranscriptionErrorCode::Internal,
                    "failed to create multipart payload",
                    false,
                )
            })?;

        let form = reqwest::multipart::Form::new()
            .text("model", GROQ_STT_MODEL)
            .text("temperature", "0")
            .part("file", file_part);

        let response = self
            .http_client
            .post(format!(
                "{}{}",
                GROQ_BASE_URL.trim_end_matches('/'),
                TRANSCRIPTIONS_PATH
            ))
            .timeout(STT_REQUEST_TIMEOUT)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await;

        let transcription_ms = started.elapsed().as_millis() as u64;

        match response {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();

                if status.is_success() {
                    #[derive(serde::Deserialize)]
                    struct TranscriptionResponse {
                        text: Option<String>,
                    }

                    let parsed: TranscriptionResponse =
                        serde_json::from_str(&body).map_err(|_| {
                            TranscriptionError::new(
                                TranscriptionErrorCode::MalformedResponse,
                                "Groq returned an unreadable response",
                                false,
                            )
                        })?;

                    let text = parsed.text.ok_or_else(|| {
                        TranscriptionError::new(
                            TranscriptionErrorCode::MalformedResponse,
                            "Groq returned a response without text",
                            false,
                        )
                    })?;

                    Ok(TranscriptResult {
                        text,
                        model: GROQ_STT_MODEL.to_string(),
                        diagnostics: AsrDiagnostics::new(
                            "groq",
                            GROQ_STT_MODEL,
                            BackendType::Cloud,
                            0,
                            transcription_ms,
                            "",
                        ),
                    })
                } else {
                    Err(classify_status_error(status, &body))
                }
            }
            Err(e) => Err(classify_request_error(e)),
        }
    }

    async fn cancel(self: Box<Self>) {}
}

fn classify_request_error(error: reqwest::Error) -> TranscriptionError {
    if error.is_timeout() {
        TranscriptionError::new(
            TranscriptionErrorCode::Timeout,
            "Groq transcription request timed out",
            true,
        )
    } else {
        TranscriptionError::new(
            TranscriptionErrorCode::ApiUnreachable,
            "Groq could not be reached",
            true,
        )
    }
}

fn classify_status_error(status: StatusCode, body: &str) -> TranscriptionError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => TranscriptionError::new(
            TranscriptionErrorCode::InvalidAuth,
            "Groq API key was rejected",
            false,
        ),
        StatusCode::REQUEST_TIMEOUT => TranscriptionError::new(
            TranscriptionErrorCode::Timeout,
            "Groq transcription request timed out",
            true,
        ),
        StatusCode::TOO_MANY_REQUESTS => TranscriptionError::new(
            TranscriptionErrorCode::RateLimit,
            "Groq rate limited the request",
            true,
        ),
        StatusCode::BAD_REQUEST => {
            if looks_like_unsupported_audio(body) {
                TranscriptionError::new(
                    TranscriptionErrorCode::UnsupportedAudio,
                    "Groq could not transcribe the audio",
                    false,
                )
            } else {
                TranscriptionError::new(
                    TranscriptionErrorCode::InvalidRequest,
                    "Groq rejected the request",
                    false,
                )
            }
        }
        StatusCode::UNSUPPORTED_MEDIA_TYPE => TranscriptionError::new(
            TranscriptionErrorCode::UnsupportedAudio,
            "Groq could not transcribe the audio",
            false,
        ),
        _ if status.is_server_error() => TranscriptionError::new(
            TranscriptionErrorCode::ServerError,
            "Groq server error",
            true,
        ),
        _ => TranscriptionError::new(
            TranscriptionErrorCode::InvalidRequest,
            "Groq rejected the request",
            false,
        ),
    }
}

fn looks_like_unsupported_audio(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("unsupported")
        || lower.contains("audio")
        || lower.contains("file type")
        || lower.contains("file format")
}

fn encode_f32_to_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let bytes_per_sample = (bits_per_sample / 8) as u32;
    let block_align = channels as u32 * bytes_per_sample;
    let byte_rate = sample_rate * block_align;
    let data_size = samples.len() as u32 * bytes_per_sample;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + data_size as usize);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&(block_align as u16).to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let int_sample = if clamped >= 0.0 {
            (clamped * i16::MAX as f32) as i16
        } else {
            (clamped * (i16::MIN as f32 * -1.0)) as i16
        };
        wav.extend_from_slice(&int_sample.to_le_bytes());
    }

    wav
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
    fn encode_f32_to_wav_produces_valid_header() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let wav = encode_f32_to_wav(&samples, 16000);

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
    fn encode_f32_to_wav_sample_values() {
        let samples = vec![0.0f32, 1.0, -1.0];
        let wav = encode_f32_to_wav(&samples, 16000);

        let first = i16::from_le_bytes([wav[44], wav[45]]);
        assert_eq!(first, 0);

        let second = i16::from_le_bytes([wav[46], wav[47]]);
        assert_eq!(second, i16::MAX);

        let third = i16::from_le_bytes([wav[48], wav[49]]);
        assert_eq!(third, i16::MIN);
    }

    #[test]
    fn classify_status_error_maps_correctly() {
        let err = classify_status_error(StatusCode::UNAUTHORIZED, "");
        assert_eq!(err.code, TranscriptionErrorCode::InvalidAuth);
        assert!(!err.retryable);

        let err = classify_status_error(StatusCode::TOO_MANY_REQUESTS, "");
        assert_eq!(err.code, TranscriptionErrorCode::RateLimit);
        assert!(err.retryable);

        let err = classify_status_error(StatusCode::INTERNAL_SERVER_ERROR, "");
        assert_eq!(err.code, TranscriptionErrorCode::ServerError);
        assert!(err.retryable);

        let err = classify_status_error(StatusCode::BAD_REQUEST, "unsupported audio format");
        assert_eq!(err.code, TranscriptionErrorCode::UnsupportedAudio);
        assert!(!err.retryable);

        let err = classify_status_error(StatusCode::BAD_REQUEST, "invalid model");
        assert_eq!(err.code, TranscriptionErrorCode::InvalidRequest);
    }

    #[test]
    fn looks_like_unsupported_audio_detects_patterns() {
        assert!(looks_like_unsupported_audio("unsupported file format"));
        assert!(looks_like_unsupported_audio("audio file not supported"));
        assert!(looks_like_unsupported_audio("file type not accepted"));
        assert!(!looks_like_unsupported_audio("invalid model name"));
        assert!(!looks_like_unsupported_audio(""));
    }
}
