use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::{multipart, StatusCode};
use serde::Deserialize;

use crate::providers::stt::{
    SttProviderDiagnostics, SttProviderFailure, SttProviderTranscription,
    SttTranscriptionClient, GROQ_WHISPER_PROVIDER_NAME,
};

pub use super::types::{
    GroqRateLimitMetadata, GroqTranscription, GroqTranscriptionError, GroqTranscriptionErrorCode,
    GROQ_STT_MODEL,
};
use super::util::{elapsed_ms, rate_limit_metadata, retry_after, retry_count_for_attempt};

pub const GROQ_BASE_URL: &str = "https://api.groq.com";
pub const TRANSCRIPTIONS_PATH: &str = "/openai/v1/audio/transcriptions";
pub const STT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: Option<String>,
}

pub struct AttemptError {
    pub error: GroqTranscriptionError,
    pub retryable: bool,
    pub retry_after: Option<Duration>,
}

impl AttemptError {
    pub fn delay(&self, base: Duration, attempt: usize) -> Duration {
        self.retry_after
            .unwrap_or_else(|| super::util::retry_delay(base, attempt))
            .min(super::util::MAX_RETRY_DELAY)
    }
}

pub struct GroqTranscriptionClient {
    http_client: reqwest::Client,
    base_url: String,
    request_timeout: Duration,
    max_attempts: usize,
    retry_backoff: Duration,
}

impl GroqTranscriptionClient {
    pub fn new(http_client: reqwest::Client) -> Self {
        Self::with_config(
            http_client,
            GROQ_BASE_URL.to_string(),
            STT_REQUEST_TIMEOUT,
            super::util::MAX_ATTEMPTS,
            super::util::INITIAL_RETRY_BACKOFF,
        )
    }

    fn with_config(
        http_client: reqwest::Client,
        base_url: String,
        timeout: Duration,
        max_attempts: usize,
        retry_backoff: Duration,
    ) -> Self {
        Self {
            http_client,
            base_url,
            request_timeout: timeout,
            max_attempts: max_attempts.max(1),
            retry_backoff,
        }
    }

    #[cfg(test)]
    pub fn for_test(
        base_url: String,
        timeout: Duration,
        max_attempts: usize,
        retry_backoff: Duration,
    ) -> Self {
        Self::with_config(
            crate::providers::http::build_shared_http_client()
                .expect("test client should build"),
            base_url,
            timeout,
            max_attempts,
            retry_backoff,
        )
    }

    pub async fn transcribe_wav(
        &self,
        api_key: &str,
        wav_bytes: Vec<u8>,
    ) -> Result<GroqTranscription, GroqTranscriptionError> {
        if api_key.trim().is_empty() {
            return Err(groq_error(
                GroqTranscriptionErrorCode::MissingApiKey,
                "Configure a Groq API key before transcribing.",
            ));
        }

        if wav_bytes.is_empty() {
            return Err(groq_error(
                GroqTranscriptionErrorCode::EmptyAudio,
                "Record audio before requesting a transcription.",
            ));
        }

        for attempt in 1..=self.max_attempts {
            match self.send_once(api_key.trim(), &wav_bytes).await {
                Ok(mut transcription) => {
                    transcription.retry_count = retry_count_for_attempt(attempt);
                    return Ok(transcription);
                }
                Err(attempt_error)
                    if attempt_error.retryable && attempt < self.max_attempts =>
                {
                    tokio::time::sleep(attempt_error.delay(self.retry_backoff, attempt)).await;
                }
                Err(mut attempt_error) => {
                    attempt_error.error.retry_count = retry_count_for_attempt(attempt);
                    return Err(attempt_error.error);
                }
            }
        }

        Err(server_error())
    }

    async fn send_once(
        &self,
        api_key: &str,
        wav_bytes: &[u8],
    ) -> Result<GroqTranscription, AttemptError> {
        let file_part = multipart::Part::bytes(wav_bytes.to_vec())
            .file_name("recording.wav")
            .mime_str("audio/wav")
            .map_err(|_| non_retryable(server_error()))?;
        let form = multipart::Form::new()
            .text("model", GROQ_STT_MODEL)
            .text("temperature", "0")
            .part("file", file_part);
        let response = self
            .http_client
            .post(self.transcriptions_url())
            .timeout(self.request_timeout)
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(classify_request_error)?;
        let status = response.status();
        let rate_limit = rate_limit_metadata(response.headers());
        let retry_after = retry_after(response.headers());
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            return parse_transcription_response(&body, rate_limit).map_err(non_retryable);
        }

        Err(classify_http_error(status, &body, retry_after, rate_limit))
    }

    fn transcriptions_url(&self) -> String {
        format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            TRANSCRIPTIONS_PATH
        )
    }
}

#[async_trait]
#[allow(dead_code)]
impl SttTranscriptionClient for GroqTranscriptionClient {
    fn provider_name(&self) -> &'static str {
        GROQ_WHISPER_PROVIDER_NAME
    }

    async fn transcribe(
        &self,
        api_key: &str,
        wav_bytes: Vec<u8>,
        audio_duration_ms: u64,
    ) -> Result<SttProviderTranscription, SttProviderFailure> {
        let started = Instant::now();

        match self.transcribe_wav(api_key, wav_bytes).await {
            Ok(transcription) => {
                let transcription_ms = elapsed_ms(started);
                Ok(SttProviderTranscription {
                    text: transcription.text,
                    model: transcription.model,
                    diagnostics: SttProviderDiagnostics::success(
                        GROQ_WHISPER_PROVIDER_NAME,
                        audio_duration_ms,
                        transcription_ms,
                    ),
                })
            }
            Err(error) => {
                let transcription_ms = elapsed_ms(started);
                let code = match error.code {
                    GroqTranscriptionErrorCode::MissingApiKey => "missing_api_key",
                    GroqTranscriptionErrorCode::InvalidApiKey => "invalid_api_key",
                    GroqTranscriptionErrorCode::RateLimit => "rate_limit",
                    GroqTranscriptionErrorCode::Timeout => "timeout",
                    GroqTranscriptionErrorCode::ApiUnreachable => "api_unreachable",
                    GroqTranscriptionErrorCode::MalformedResponse => "malformed_response",
                    GroqTranscriptionErrorCode::UnsupportedAudio => "unsupported_audio",
                    GroqTranscriptionErrorCode::InvalidRequest => "invalid_request",
                    GroqTranscriptionErrorCode::EmptyAudio => "empty_audio",
                    GroqTranscriptionErrorCode::ServerError => "server_error",
                };
                Err(SttProviderFailure {
                    diagnostics: SttProviderDiagnostics::failure(
                        GROQ_WHISPER_PROVIDER_NAME,
                        audio_duration_ms,
                        transcription_ms,
                        code,
                    ),
                })
            }
        }
    }
}

fn parse_transcription_response(
    body: &str,
    rate_limit: Option<GroqRateLimitMetadata>,
) -> Result<GroqTranscription, GroqTranscriptionError> {
    let response: TranscriptionResponse = serde_json::from_str(body).map_err(|_| {
        groq_error(
            GroqTranscriptionErrorCode::MalformedResponse,
            "Groq returned a transcription response Floe could not read.",
        )
    })?;

    let Some(text) = response.text else {
        return Err(groq_error(
            GroqTranscriptionErrorCode::MalformedResponse,
            "Groq returned a transcription response without text.",
        ));
    };

    Ok(GroqTranscription {
        text,
        model: GROQ_STT_MODEL.to_string(),
        retry_count: 0,
        rate_limit: rate_limit.map(Box::new),
        stt_provider: None,
    })
}

fn classify_request_error(error: reqwest::Error) -> AttemptError {
    if error.is_timeout() {
        return retryable(groq_error(
            GroqTranscriptionErrorCode::Timeout,
            "The Groq transcription request timed out.",
        ));
    }

    retryable(groq_error(
        GroqTranscriptionErrorCode::ApiUnreachable,
        "Groq could not be reached. Check your network connection and try again.",
    ))
}

fn classify_http_error(
    status: StatusCode,
    body: &str,
    retry_after: Option<Duration>,
    rate_limit: Option<GroqRateLimitMetadata>,
) -> AttemptError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            non_retryable(groq_error_with_rate_limit(
                GroqTranscriptionErrorCode::InvalidApiKey,
                "The configured Groq API key was rejected.",
                rate_limit,
            ))
        }
        StatusCode::REQUEST_TIMEOUT => retryable_with_retry_after(
            groq_error_with_rate_limit(
                GroqTranscriptionErrorCode::Timeout,
                "The Groq transcription request timed out.",
                rate_limit,
            ),
            retry_after,
        ),
        StatusCode::TOO_MANY_REQUESTS => retryable_with_retry_after(
            groq_error_with_rate_limit(
                GroqTranscriptionErrorCode::RateLimit,
                "Groq rate limited the transcription request. Try again shortly.",
                rate_limit,
            ),
            retry_after,
        ),
        StatusCode::BAD_REQUEST => non_retryable(groq_error_with_rate_limit(
            invalid_request_code(body),
            invalid_request_message(body),
            rate_limit,
        )),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => non_retryable(groq_error_with_rate_limit(
            GroqTranscriptionErrorCode::UnsupportedAudio,
            "Groq could not transcribe the uploaded audio file.",
            rate_limit,
        )),
        status if status.is_server_error() => retryable_with_retry_after(
            groq_error_with_rate_limit(
                GroqTranscriptionErrorCode::ServerError,
                "Groq could not complete the transcription request.",
                rate_limit,
            ),
            retry_after,
        ),
        _ => non_retryable(groq_error_with_rate_limit(
            GroqTranscriptionErrorCode::InvalidRequest,
            "Groq rejected the transcription request.",
            rate_limit,
        )),
    }
}

fn invalid_request_code(body: &str) -> GroqTranscriptionErrorCode {
    if looks_like_unsupported_audio(body) {
        GroqTranscriptionErrorCode::UnsupportedAudio
    } else {
        GroqTranscriptionErrorCode::InvalidRequest
    }
}

fn invalid_request_message(body: &str) -> &'static str {
    if looks_like_unsupported_audio(body) {
        "Groq could not transcribe the uploaded audio file."
    } else {
        "Groq rejected the transcription request."
    }
}

fn looks_like_unsupported_audio(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("unsupported")
        || lower.contains("audio")
        || lower.contains("file type")
        || lower.contains("file format")
}

fn retryable(error: GroqTranscriptionError) -> AttemptError {
    AttemptError {
        error,
        retryable: true,
        retry_after: None,
    }
}

fn retryable_with_retry_after(
    error: GroqTranscriptionError,
    retry_after: Option<Duration>,
) -> AttemptError {
    AttemptError {
        error,
        retryable: true,
        retry_after,
    }
}

fn non_retryable(error: GroqTranscriptionError) -> AttemptError {
    AttemptError {
        error,
        retryable: false,
        retry_after: None,
    }
}

fn server_error() -> GroqTranscriptionError {
    groq_error(
        GroqTranscriptionErrorCode::ServerError,
        "Groq transcription could not be initialized.",
    )
}

pub fn groq_error(
    code: GroqTranscriptionErrorCode,
    message: &'static str,
) -> GroqTranscriptionError {
    groq_error_with_rate_limit(code, message, None)
}

fn groq_error_with_rate_limit(
    code: GroqTranscriptionErrorCode,
    message: &'static str,
    rate_limit: Option<GroqRateLimitMetadata>,
) -> GroqTranscriptionError {
    GroqTranscriptionError {
        code,
        message: message.to_string(),
        model: GROQ_STT_MODEL.to_string(),
        retry_count: 0,
        rate_limit: rate_limit.map(Box::new),
        stt_provider: None,
    }
}
