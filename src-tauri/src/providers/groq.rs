use std::time::{Duration, Instant};

use reqwest::{header::HeaderMap, multipart, StatusCode};
use serde::{Deserialize, Serialize};

use crate::prompts::cleanup::CLEANUP_SYSTEM_PROMPT;

const GROQ_BASE_URL: &str = "https://api.groq.com";
const TRANSCRIPTIONS_PATH: &str = "/openai/v1/audio/transcriptions";
const CHAT_COMPLETIONS_PATH: &str = "/openai/v1/chat/completions";
pub const GROQ_STT_MODEL: &str = "whisper-large-v3-turbo";
pub const GROQ_CLEANUP_MODEL: &str = "llama-3.3-70b-versatile";
const STT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CLEANUP_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_ATTEMPTS: usize = 3;
const INITIAL_RETRY_BACKOFF: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GroqRateLimitMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_requests: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_tokens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_requests: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_tokens: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqTranscription {
    pub text: String,
    pub model: String,
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Box<GroqRateLimitMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_asr: Option<Box<crate::asr::LocalAsrDiagnostics>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqTranscriptionError {
    pub code: GroqTranscriptionErrorCode,
    pub message: String,
    pub model: String,
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Box<GroqRateLimitMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_asr: Option<Box<crate::asr::LocalAsrDiagnostics>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GroqTranscriptionErrorCode {
    MissingApiKey,
    InvalidApiKey,
    RateLimit,
    Timeout,
    ApiUnreachable,
    MalformedResponse,
    UnsupportedAudio,
    InvalidRequest,
    EmptyAudio,
    ServerError,
}

impl GroqTranscriptionError {
    pub fn new(code: GroqTranscriptionErrorCode, message: &'static str) -> Self {
        groq_error(code, message)
    }
}

pub struct GroqTranscriptionClient {
    http_client: reqwest::Client,
    base_url: String,
    request_timeout: Duration,
    max_attempts: usize,
    retry_backoff: Duration,
}

struct AttemptError {
    error: GroqTranscriptionError,
    retryable: bool,
    retry_after: Option<Duration>,
}

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: Option<String>,
}

impl GroqTranscriptionClient {
    pub fn new(http_client: reqwest::Client) -> Self {
        Self::with_config(
            http_client,
            GROQ_BASE_URL.to_string(),
            STT_REQUEST_TIMEOUT,
            MAX_ATTEMPTS,
            INITIAL_RETRY_BACKOFF,
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
    fn for_test(
        base_url: String,
        timeout: Duration,
        max_attempts: usize,
        retry_backoff: Duration,
    ) -> Self {
        Self::with_config(
            crate::providers::http::build_shared_http_client().expect("test client should build"),
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
                Err(attempt_error) if attempt_error.retryable && attempt < self.max_attempts => {
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

impl AttemptError {
    fn delay(&self, base: Duration, attempt: usize) -> Duration {
        self.retry_after
            .unwrap_or_else(|| retry_delay(base, attempt))
            .min(MAX_RETRY_DELAY)
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
        local_asr: None,
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

fn retry_delay(base: Duration, attempt: usize) -> Duration {
    let multiplier = 1_u32.checked_shl((attempt - 1) as u32).unwrap_or(u32::MAX);
    base.saturating_mul(multiplier)
}

fn retry_count_for_attempt(attempt: usize) -> u32 {
    attempt.saturating_sub(1).try_into().unwrap_or(u32::MAX)
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

fn groq_error(code: GroqTranscriptionErrorCode, message: &'static str) -> GroqTranscriptionError {
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
        local_asr: None,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqCleanup {
    pub text: String,
    pub model: String,
    pub retry_count: u32,
    pub validation_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Box<GroqRateLimitMetadata>>,
}

impl PartialEq<&str> for GroqCleanup {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqCleanupError {
    pub code: GroqCleanupErrorCode,
    pub message: String,
    pub model: String,
    pub retry_count: u32,
    pub validation_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Box<GroqRateLimitMetadata>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GroqCleanupErrorCode {
    MissingApiKey,
    InvalidApiKey,
    RateLimit,
    Timeout,
    ApiUnreachable,
    MalformedResponse,
    InvalidRequest,
    EmptyTranscript,
    ValidationFailed,
    ServerError,
}

impl GroqCleanupError {
    #[cfg(test)]
    pub fn new(code: GroqCleanupErrorCode, message: &'static str) -> Self {
        groq_cleanup_error(code, message)
    }
}

pub struct GroqCleanupClient {
    http_client: reqwest::Client,
    base_url: String,
    request_timeout: Duration,
    max_attempts: usize,
    retry_backoff: Duration,
}

struct CleanupAttemptError {
    error: GroqCleanupError,
    retryable: bool,
    retry_after: Option<Duration>,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: &'static str,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Option<Vec<ChatChoice>>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: Option<ChatResponseMessage>,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

impl GroqCleanupClient {
    pub fn new(http_client: reqwest::Client) -> Self {
        Self::with_config(
            http_client,
            GROQ_BASE_URL.to_string(),
            CLEANUP_REQUEST_TIMEOUT,
            MAX_ATTEMPTS,
            INITIAL_RETRY_BACKOFF,
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
    fn for_test(
        base_url: String,
        timeout: Duration,
        max_attempts: usize,
        retry_backoff: Duration,
    ) -> Self {
        Self::with_config(
            crate::providers::http::build_shared_http_client().expect("test client should build"),
            base_url,
            timeout,
            max_attempts,
            retry_backoff,
        )
    }

    pub async fn cleanup_transcript(
        &self,
        api_key: &str,
        transcript: &str,
    ) -> Result<GroqCleanup, GroqCleanupError> {
        let api_key = api_key.trim();

        if api_key.is_empty() {
            return Err(groq_cleanup_error(
                GroqCleanupErrorCode::MissingApiKey,
                "Configure a Groq API key before cleaning a transcript.",
            ));
        }

        if transcript.trim().is_empty() {
            return Err(groq_cleanup_error(
                GroqCleanupErrorCode::EmptyTranscript,
                "There is no transcript text to clean.",
            ));
        }

        for attempt in 1..=self.max_attempts {
            match self.send_once(api_key, transcript).await {
                Ok(mut cleaned) => {
                    cleaned.retry_count = retry_count_for_attempt(attempt);
                    return Ok(cleaned);
                }
                Err(attempt_error) if attempt_error.retryable && attempt < self.max_attempts => {
                    let delay = attempt_error
                        .retry_after
                        .unwrap_or_else(|| retry_delay(self.retry_backoff, attempt))
                        .min(MAX_RETRY_DELAY);
                    tokio::time::sleep(delay).await;
                }
                Err(mut attempt_error) => {
                    attempt_error.error.retry_count = retry_count_for_attempt(attempt);
                    return Err(attempt_error.error);
                }
            }
        }

        Err(cleanup_server_error())
    }

    async fn send_once(
        &self,
        api_key: &str,
        transcript: &str,
    ) -> Result<GroqCleanup, CleanupAttemptError> {
        let response = self
            .http_client
            .post(self.chat_completions_url())
            .timeout(self.request_timeout)
            .bearer_auth(api_key)
            .json(&cleanup_request_body(transcript))
            .send()
            .await
            .map_err(classify_cleanup_request_error)?;
        let status = response.status();
        let retry_after = retry_after(response.headers());
        let rate_limit = rate_limit_metadata(response.headers());
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            return parse_cleanup_response(&body, transcript, rate_limit)
                .map_err(cleanup_non_retryable);
        }

        Err(classify_cleanup_http_error(status, retry_after, rate_limit))
    }

    fn chat_completions_url(&self) -> String {
        format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            CHAT_COMPLETIONS_PATH
        )
    }
}

impl CleanupAttemptError {
    #[allow(dead_code)]
    #[cfg(test)]
    fn delay(&self, base: Duration, attempt: usize) -> Duration {
        self.retry_after
            .unwrap_or_else(|| retry_delay(base, attempt))
            .min(MAX_RETRY_DELAY)
    }
}

fn cleanup_request_body(transcript: &str) -> ChatCompletionRequest {
    ChatCompletionRequest {
        model: GROQ_CLEANUP_MODEL,
        messages: vec![
            ChatMessage {
                role: "system",
                content: CLEANUP_SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user",
                content: format!("<transcript>\n{transcript}\n</transcript>"),
            },
        ],
        temperature: 0.0,
        max_tokens: cleanup_max_tokens_for(transcript),
    }
}

fn cleanup_max_tokens_for(transcript: &str) -> u32 {
    let input_word_count = transcript.split_whitespace().count();
    input_word_count
        .saturating_mul(2)
        .clamp(64, 1024)
        .try_into()
        .unwrap_or(1024)
}

fn parse_cleanup_response(
    body: &str,
    input: &str,
    rate_limit: Option<GroqRateLimitMetadata>,
) -> Result<GroqCleanup, GroqCleanupError> {
    let response: ChatCompletionResponse = serde_json::from_str(body).map_err(|_| {
        groq_cleanup_error(
            GroqCleanupErrorCode::MalformedResponse,
            "Groq returned a cleanup response Floe could not read.",
        )
    })?;
    let content = response
        .choices
        .and_then(|choices| choices.into_iter().next())
        .and_then(|choice| choice.message)
        .and_then(|message| message.content)
        .ok_or_else(|| {
            groq_cleanup_error(
                GroqCleanupErrorCode::MalformedResponse,
                "Groq returned a cleanup response without text.",
            )
        })?;

    let validation_started = Instant::now();
    let text = validate_cleanup_output(input, &content).map_err(|mut error| {
        error.validation_ms = elapsed_ms(validation_started);
        error
    })?;

    Ok(GroqCleanup {
        text,
        model: GROQ_CLEANUP_MODEL.to_string(),
        retry_count: 0,
        validation_ms: elapsed_ms(validation_started),
        rate_limit: rate_limit.map(Box::new),
    })
}

pub fn validate_cleanup_output(input: &str, output: &str) -> Result<String, GroqCleanupError> {
    let trimmed = output.trim();

    if trimmed.is_empty() {
        return Err(cleanup_validation_error());
    }

    let input_len = input.chars().count();
    let output_len = trimmed.chars().count();
    let max_len = input_len.saturating_mul(2).max(input_len + 200);

    if output_len > max_len
        || cleanup_looks_like_markdown(trimmed)
        || cleanup_looks_like_json(trimmed)
        || cleanup_looks_like_yaml(trimmed)
        || cleanup_looks_like_commentary(trimmed)
    {
        return Err(cleanup_validation_error());
    }

    Ok(trimmed.to_string())
}

fn cleanup_looks_like_markdown(value: &str) -> bool {
    let trimmed = value.trim_start();

    trimmed.starts_with("```")
        || trimmed.starts_with('#')
        || trimmed.contains("\n```")
        || trimmed.contains("\n- ")
        || trimmed.contains("\n* ")
}

fn cleanup_looks_like_commentary(value: &str) -> bool {
    let lower = value.trim_start().to_ascii_lowercase();
    let rejected_prefixes = [
        "corrected:",
        "output:",
        "here is",
        "here's",
        "here’s",
        "here's the cleaned text:",
        "here is the cleaned text:",
        "cleaned transcript:",
        "the cleaned transcript is:",
        "cleaned:",
        "sure,",
        "okay,",
        "here you go,",
    ];

    rejected_prefixes
        .iter()
        .any(|prefix| lower.starts_with(prefix))
        || lower.contains("i corrected")
        || lower.contains("i have corrected")
}

fn cleanup_looks_like_json(value: &str) -> bool {
    let trimmed = value.trim();
    (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || trimmed.starts_with("\"text\"")
        || trimmed.contains("\":")
}

fn cleanup_looks_like_yaml(value: &str) -> bool {
    let trimmed = value.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("---")
        || lower.starts_with("transcript:")
        || lower.starts_with("cleaned:")
        || lower.starts_with("text:")
}

fn classify_cleanup_request_error(error: reqwest::Error) -> CleanupAttemptError {
    if error.is_timeout() {
        return cleanup_retryable(groq_cleanup_error(
            GroqCleanupErrorCode::Timeout,
            "The Groq cleanup request timed out.",
        ));
    }

    cleanup_retryable(groq_cleanup_error(
        GroqCleanupErrorCode::ApiUnreachable,
        "Groq could not be reached. Check your network connection and try again.",
    ))
}

fn classify_cleanup_http_error(
    status: StatusCode,
    retry_after: Option<Duration>,
    rate_limit: Option<GroqRateLimitMetadata>,
) -> CleanupAttemptError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            cleanup_non_retryable(groq_cleanup_error_with_rate_limit(
                GroqCleanupErrorCode::InvalidApiKey,
                "The configured Groq API key was rejected.",
                rate_limit,
            ))
        }
        StatusCode::BAD_REQUEST => cleanup_non_retryable(groq_cleanup_error_with_rate_limit(
            GroqCleanupErrorCode::InvalidRequest,
            "Groq rejected the cleanup request.",
            rate_limit,
        )),
        StatusCode::REQUEST_TIMEOUT => cleanup_retryable_with_retry_after(
            groq_cleanup_error_with_rate_limit(
                GroqCleanupErrorCode::Timeout,
                "The Groq cleanup request timed out.",
                rate_limit,
            ),
            retry_after,
        ),
        StatusCode::TOO_MANY_REQUESTS => cleanup_retryable_with_retry_after(
            groq_cleanup_error_with_rate_limit(
                GroqCleanupErrorCode::RateLimit,
                "Groq rate limited the cleanup request. Try again shortly.",
                rate_limit,
            ),
            retry_after,
        ),
        status if status.is_server_error() => cleanup_retryable_with_retry_after(
            groq_cleanup_error_with_rate_limit(
                GroqCleanupErrorCode::ServerError,
                "Groq could not complete the cleanup request.",
                rate_limit,
            ),
            retry_after,
        ),
        _ => cleanup_non_retryable(groq_cleanup_error_with_rate_limit(
            GroqCleanupErrorCode::InvalidRequest,
            "Groq rejected the cleanup request.",
            rate_limit,
        )),
    }
}

fn cleanup_retryable(error: GroqCleanupError) -> CleanupAttemptError {
    CleanupAttemptError {
        error,
        retryable: true,
        retry_after: None,
    }
}

fn cleanup_retryable_with_retry_after(
    error: GroqCleanupError,
    retry_after: Option<Duration>,
) -> CleanupAttemptError {
    CleanupAttemptError {
        error,
        retryable: true,
        retry_after,
    }
}

fn cleanup_non_retryable(error: GroqCleanupError) -> CleanupAttemptError {
    CleanupAttemptError {
        error,
        retryable: false,
        retry_after: None,
    }
}

fn cleanup_server_error() -> GroqCleanupError {
    groq_cleanup_error(
        GroqCleanupErrorCode::ServerError,
        "Groq cleanup could not be initialized.",
    )
}

fn cleanup_validation_error() -> GroqCleanupError {
    groq_cleanup_error(
        GroqCleanupErrorCode::ValidationFailed,
        "Groq cleanup returned text Floe could not safely use.",
    )
}

fn groq_cleanup_error(code: GroqCleanupErrorCode, message: &'static str) -> GroqCleanupError {
    groq_cleanup_error_with_rate_limit(code, message, None)
}

fn groq_cleanup_error_with_rate_limit(
    code: GroqCleanupErrorCode,
    message: &'static str,
    rate_limit: Option<GroqRateLimitMetadata>,
) -> GroqCleanupError {
    GroqCleanupError {
        code,
        message: message.to_string(),
        model: GROQ_CLEANUP_MODEL.to_string(),
        retry_count: 0,
        validation_ms: 0,
        rate_limit: rate_limit.map(Box::new),
    }
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn rate_limit_metadata(headers: &HeaderMap) -> Option<GroqRateLimitMetadata> {
    let metadata = GroqRateLimitMetadata {
        remaining_requests: header_value(headers, "x-ratelimit-remaining-requests"),
        remaining_tokens: header_value(headers, "x-ratelimit-remaining-tokens"),
        reset_requests: header_value(headers, "x-ratelimit-reset-requests"),
        reset_tokens: header_value(headers, "x-ratelimit-reset-tokens"),
        retry_after_seconds: retry_after(headers).map(|duration| duration.as_secs()),
    };

    if metadata == GroqRateLimitMetadata::default() {
        None
    } else {
        Some(metadata)
    }
}

fn header_value(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        thread,
        time::Duration,
    };

    use super::{
        cleanup_max_tokens_for, GroqCleanupClient, GroqCleanupErrorCode, GroqTranscriptionClient,
        GroqTranscriptionErrorCode, CHAT_COMPLETIONS_PATH, GROQ_CLEANUP_MODEL, GROQ_STT_MODEL,
        TRANSCRIPTIONS_PATH,
    };

    #[tokio::test]
    async fn successful_request_sends_expected_multipart_payload() {
        let server = MockServer::start(vec![MockResponse::json(200, r#"{"text":"hello"}"#)]);
        let client = test_client(server.base_url());

        let result = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect("transcription should succeed");

        assert_eq!(result.text, "hello");
        assert_eq!(result.model, GROQ_STT_MODEL);
        assert_eq!(result.retry_count, 0);
        assert_eq!(server.request_count(), 1);
        let request = server.requests()[0].clone();
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with(&format!("POST {TRANSCRIPTIONS_PATH} HTTP/1.1")));
        assert!(request_lower.contains("authorization: bearer gsk_test_key"));
        assert!(request.contains(GROQ_STT_MODEL));
        assert!(request.contains("temperature"));
        assert!(request.contains("\r\n0\r\n"));
        assert!(!request_lower.contains("name=\"language\""));
        assert!(!request_lower.contains("name=\"de\""));
        assert!(request.contains("recording.wav"));
        assert!(request.contains("audio/wav"));
    }

    #[tokio::test]
    async fn stt_request_omits_language_field_by_default() {
        let server = MockServer::start(vec![MockResponse::json(200, r#"{"text":"hello"}"#)]);
        let client = test_client(server.base_url());

        let result = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect("transcription should succeed");

        assert_eq!(result.text, "hello");
        assert_eq!(result.model, GROQ_STT_MODEL);
        assert_eq!(result.retry_count, 0);
        assert_eq!(server.request_count(), 1);
        let request = server.requests()[0].clone();
        let request_lower = request.to_ascii_lowercase();
        assert!(request.contains(GROQ_STT_MODEL));
        assert!(request.contains("temperature"));
        assert!(request.contains("\r\n0\r\n"));
        assert!(!request_lower.contains("name=\"language\""));
        assert!(!request_lower.contains("name=\"en\""));
        assert!(!request_lower.contains("name=\"de\""));
        assert!(!request_lower.contains("name=\"auto\""));
    }

    #[tokio::test]
    async fn stt_rate_limit_respects_retry_after_and_tracks_metadata() {
        let server = MockServer::start(vec![
            MockResponse::json(429, r#"{"error":"rate limited"}"#)
                .with_header("Retry-After", "1")
                .with_header("x-ratelimit-remaining-requests", "0"),
            MockResponse::json(200, r#"{"text":"ok"}"#)
                .with_header("x-ratelimit-remaining-requests", "12")
                .with_header("x-ratelimit-remaining-tokens", "42"),
        ]);
        let client = GroqTranscriptionClient::for_test(
            server.base_url(),
            Duration::from_secs(2),
            2,
            Duration::from_millis(10),
        );

        let result = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect("retry should succeed");

        assert_eq!(result.retry_count, 1);
        let rate_limit = result
            .rate_limit
            .expect("rate limit metadata should be captured");
        assert_eq!(rate_limit.remaining_requests.as_deref(), Some("12"));
        assert_eq!(rate_limit.remaining_tokens.as_deref(), Some("42"));
        assert_eq!(server.request_count(), 2);
    }

    #[tokio::test]
    async fn missing_api_key_is_not_retried() {
        let server = MockServer::start(vec![MockResponse::json(200, r#"{"text":"unused"}"#)]);
        let client = test_client(server.base_url());

        let error = client
            .transcribe_wav("   ", minimal_wav())
            .await
            .expect_err("missing key should fail");

        assert_eq!(error.code, GroqTranscriptionErrorCode::MissingApiKey);
        assert_eq!(server.request_count(), 0);
    }

    #[tokio::test]
    async fn empty_audio_is_not_retried() {
        let server = MockServer::start(vec![MockResponse::json(200, r#"{"text":"unused"}"#)]);
        let client = test_client(server.base_url());

        let error = client
            .transcribe_wav("gsk_test_key", Vec::new())
            .await
            .expect_err("empty audio should fail");

        assert_eq!(error.code, GroqTranscriptionErrorCode::EmptyAudio);
        assert_eq!(server.request_count(), 0);
    }

    #[tokio::test]
    async fn invalid_key_is_not_retried() {
        for status in [401, 403] {
            let server = MockServer::start(vec![MockResponse::json(status, r#"{"error":{}}"#)]);
            let client = test_client(server.base_url());

            let error = client
                .transcribe_wav("gsk_test_key", minimal_wav())
                .await
                .expect_err("invalid key should fail");

            assert_eq!(error.code, GroqTranscriptionErrorCode::InvalidApiKey);
            assert_eq!(server.request_count(), 1);
        }
    }

    #[tokio::test]
    async fn mapped_error_messages_do_not_expose_api_key_or_response_body() {
        let api_key = "gsk_super_secret_test_key";
        let server = MockServer::start(vec![MockResponse::json(
            401,
            r#"{"error":"gsk_super_secret_test_key was rejected"}"#,
        )]);
        let client = test_client(server.base_url());

        let error = client
            .transcribe_wav(api_key, minimal_wav())
            .await
            .expect_err("invalid key should fail");

        assert_eq!(error.code, GroqTranscriptionErrorCode::InvalidApiKey);
        assert!(!error.message.contains(api_key));
        assert!(!error
            .message
            .contains("gsk_super_secret_test_key was rejected"));
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn invalid_request_is_not_retried() {
        let server = MockServer::start(vec![MockResponse::json(400, r#"{"error":"bad"}"#)]);
        let client = test_client(server.base_url());

        let error = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect_err("invalid request should fail");

        assert_eq!(error.code, GroqTranscriptionErrorCode::InvalidRequest);
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn unsupported_audio_is_not_retried() {
        let server = MockServer::start(vec![MockResponse::json(
            400,
            r#"{"error":"unsupported audio file"}"#,
        )]);
        let client = test_client(server.base_url());

        let error = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect_err("unsupported audio should fail");

        assert_eq!(error.code, GroqTranscriptionErrorCode::UnsupportedAudio);
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn malformed_success_response_fails_without_retry() {
        let server = MockServer::start(vec![MockResponse::json(200, r#"{"value":"no text"}"#)]);
        let client = test_client(server.base_url());

        let error = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect_err("malformed response should fail");

        assert_eq!(error.code, GroqTranscriptionErrorCode::MalformedResponse);
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn rate_limit_retries_with_bounded_attempts() {
        let server = MockServer::start(vec![
            MockResponse::json(429, r#"{"error":"rate limited"}"#),
            MockResponse::json(429, r#"{"error":"rate limited"}"#),
            MockResponse::json(429, r#"{"error":"rate limited"}"#),
        ]);
        let client = test_client(server.base_url());

        let error = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect_err("rate limit should fail after retries");

        assert_eq!(error.code, GroqTranscriptionErrorCode::RateLimit);
        assert_eq!(error.model, GROQ_STT_MODEL);
        assert_eq!(error.retry_count, 2);
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn rate_limit_retries_until_success() {
        let server = MockServer::start(vec![
            MockResponse::json(429, r#"{"error":"rate limited"}"#),
            MockResponse::json(200, r#"{"text":"ok after rate limit"}"#),
        ]);
        let client = test_client(server.base_url());

        let result = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect("rate-limit retry should succeed");

        assert_eq!(result.text, "ok after rate limit");
        assert_eq!(result.retry_count, 1);
        assert_eq!(server.request_count(), 2);
        assert!(server
            .requests()
            .iter()
            .all(|request| request.starts_with(&format!("POST {TRANSCRIPTIONS_PATH} HTTP/1.1"))));
    }

    #[tokio::test]
    async fn request_timeout_status_retries_with_bounded_attempts() {
        let server = MockServer::start(vec![
            MockResponse::json(408, r#"{"error":"request timeout"}"#),
            MockResponse::json(408, r#"{"error":"request timeout"}"#),
            MockResponse::json(408, r#"{"error":"request timeout"}"#),
        ]);
        let client = test_client(server.base_url());

        let error = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect_err("request timeout should fail after retries");

        assert_eq!(error.code, GroqTranscriptionErrorCode::Timeout);
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn request_timeout_status_retries_until_success() {
        let server = MockServer::start(vec![
            MockResponse::json(408, r#"{"error":"request timeout"}"#),
            MockResponse::json(200, r#"{"text":"ok after timeout"}"#),
        ]);
        let client = test_client(server.base_url());

        let result = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect("timeout-status retry should succeed");

        assert_eq!(result.text, "ok after timeout");
        assert_eq!(server.request_count(), 2);
    }

    #[tokio::test]
    async fn server_errors_retry_until_success() {
        let server = MockServer::start(vec![
            MockResponse::json(500, r#"{"error":"server"}"#),
            MockResponse::json(503, r#"{"error":"server"}"#),
            MockResponse::json(200, r#"{"text":"ok"}"#),
        ]);
        let client = test_client(server.base_url());

        let result = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect("final retry should succeed");

        assert_eq!(result.text, "ok");
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn server_errors_fail_after_retry_exhaustion() {
        let server = MockServer::start(vec![
            MockResponse::json(500, r#"{"error":"server"}"#),
            MockResponse::json(500, r#"{"error":"server"}"#),
            MockResponse::json(500, r#"{"error":"server"}"#),
        ]);
        let client = test_client(server.base_url());

        let error = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect_err("server errors should exhaust retries");

        assert_eq!(error.code, GroqTranscriptionErrorCode::ServerError);
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn timeout_retries_and_reports_timeout() {
        let server = MockServer::start(vec![
            MockResponse::slow_json(200, r#"{"text":"late"}"#, Duration::from_millis(100)),
            MockResponse::slow_json(200, r#"{"text":"late"}"#, Duration::from_millis(100)),
            MockResponse::slow_json(200, r#"{"text":"late"}"#, Duration::from_millis(100)),
        ]);
        let client = GroqTranscriptionClient::for_test(
            server.base_url(),
            Duration::from_millis(20),
            3,
            Duration::ZERO,
        );

        let error = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect_err("timeouts should fail after retries");

        assert_eq!(error.code, GroqTranscriptionErrorCode::Timeout);
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn unreachable_api_retries_and_reports_unreachable() {
        let server = BrokenServer::start(3);
        let client = GroqTranscriptionClient::for_test(
            server.base_url(),
            Duration::from_secs(1),
            3,
            Duration::ZERO,
        );

        let error = client
            .transcribe_wav("gsk_test_key", minimal_wav())
            .await
            .expect_err("unreachable api should fail");

        assert_eq!(error.code, GroqTranscriptionErrorCode::ApiUnreachable);
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn cleanup_successful_request_sends_expected_payload() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"Cleaned transcript."}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let result = client
            .cleanup_transcript("gsk_test_key", "raw transcript")
            .await
            .expect("cleanup should succeed");

        assert_eq!(result, "Cleaned transcript.");
        assert_eq!(result.text, "Cleaned transcript.");
        assert_eq!(result.model, GROQ_CLEANUP_MODEL);
        assert_eq!(result.retry_count, 0);
        assert_eq!(server.request_count(), 1);
        let request = server.requests()[0].clone();
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with(&format!("POST {CHAT_COMPLETIONS_PATH} HTTP/1.1")));
        assert!(request_lower.contains("authorization: bearer gsk_test_key"));
        assert!(request.contains(GROQ_CLEANUP_MODEL));
        assert!(request.contains(r#""role":"system""#));
        assert!(request.contains(r#""role":"user""#));
        assert!(request.contains(r#""temperature":0"#));
        assert!(request
            .contains("You are a transcript cleanup engine for a push-to-talk dictation app."));
        assert!(request.contains("Preserve the original language."));
        assert!(request.contains("Technical vocabulary:"));
        assert!(!request.contains("Clean this transcript"));
        assert!(request.contains("<transcript>"));
        assert!(request.contains("raw transcript"));
    }

    #[tokio::test]
    async fn cleanup_request_uses_llama_3_3_and_sends_no_unsupported_parameters() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"Cleaned."}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let result = client
            .cleanup_transcript("gsk_test_key", "raw transcript")
            .await
            .expect("cleanup should succeed");

        assert_eq!(result.model, "llama-3.3-70b-versatile");
        assert_eq!(GROQ_CLEANUP_MODEL, "llama-3.3-70b-versatile");

        let request = server.requests()[0].clone();
        let body = extract_request_body(&request);
        let body_lower = body.to_ascii_lowercase();
        assert!(!body_lower.contains("gpt-oss"));
        assert!(!body_lower.contains("reasoning_effort"));
        assert!(!body_lower.contains("\"reasoning\""));
        assert!(!body_lower.contains("qwen"));
        assert!(body.contains(r#""temperature":0"#));
        assert!(body_lower.contains("\"max_tokens\":64"));
        assert!(body.contains(r#""model":"llama-3.3-70b-versatile""#));
    }

    #[tokio::test]
    async fn cleanup_rate_limit_respects_retry_after_and_tracks_metadata() {
        let server = MockServer::start(vec![
            MockResponse::json(429, r#"{"error":{"message":"slow down"}}"#)
                .with_header("Retry-After", "1")
                .with_header("x-ratelimit-remaining-requests", "0"),
            MockResponse::json(200, r#"{"choices":[{"message":{"content":"Cleaned."}}]}"#)
                .with_header("x-ratelimit-remaining-requests", "8")
                .with_header("x-ratelimit-reset-requests", "2s"),
        ]);
        let client = GroqCleanupClient::for_test(
            server.base_url(),
            Duration::from_secs(2),
            2,
            Duration::ZERO,
        );

        let result = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect("cleanup should eventually succeed");

        assert_eq!(result.retry_count, 1);
        let rate_limit = result
            .rate_limit
            .expect("rate limit metadata should be captured");
        assert_eq!(rate_limit.remaining_requests.as_deref(), Some("8"));
        assert_eq!(rate_limit.reset_requests.as_deref(), Some("2s"));
        assert_eq!(server.request_count(), 2);
    }

    #[tokio::test]
    async fn cleanup_missing_api_key_is_not_retried() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"unused"}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("   ", "raw")
            .await
            .expect_err("missing key should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::MissingApiKey);
        assert_eq!(server.request_count(), 0);
    }

    #[tokio::test]
    async fn cleanup_empty_transcript_is_not_retried() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"unused"}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "   \n  ")
            .await
            .expect_err("empty transcript should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::EmptyTranscript);
        assert_eq!(server.request_count(), 0);
    }

    #[tokio::test]
    async fn cleanup_invalid_api_key_is_not_retried() {
        let server = MockServer::start(vec![MockResponse::json(
            401,
            r#"{"error":{"message":"unauthorized"}}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("invalid key should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::InvalidApiKey);
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn cleanup_mapped_error_messages_do_not_expose_api_key_or_response_body() {
        let server = MockServer::start(vec![MockResponse::json(
            401,
            r#"{"error":{"message":"leaked secret response body"}}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("invalid key should fail");

        assert!(!error.message.contains("gsk_test_key"));
        assert!(!error.message.contains("leaked secret response body"));
    }

    #[tokio::test]
    async fn cleanup_malformed_success_response_fails_without_retry() {
        let server = MockServer::start(vec![MockResponse::json(200, r#"{"unexpected":"shape"}"#)]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("malformed response should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::MalformedResponse);
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn cleanup_rate_limit_retries_with_bounded_attempts() {
        let server = MockServer::start(vec![
            MockResponse::json(429, r#"{"error":{"message":"slow down"}}"#),
            MockResponse::json(429, r#"{"error":{"message":"slow down"}}"#),
            MockResponse::json(429, r#"{"error":{"message":"slow down"}}"#),
        ]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("rate limit should fail after retries");

        assert_eq!(error.code, GroqCleanupErrorCode::RateLimit);
        assert_eq!(error.model, GROQ_CLEANUP_MODEL);
        assert_eq!(error.retry_count, 2);
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn cleanup_rate_limit_retries_until_success() {
        let server = MockServer::start(vec![
            MockResponse::json(429, r#"{"error":{"message":"slow down"}}"#),
            MockResponse::json(200, r#"{"choices":[{"message":{"content":"Cleaned."}}]}"#),
        ]);
        let client = test_cleanup_client(server.base_url());

        let result = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect("cleanup should eventually succeed");

        assert_eq!(result, "Cleaned.");
        assert_eq!(result.retry_count, 1);
        assert_eq!(server.request_count(), 2);
    }

    #[tokio::test]
    async fn cleanup_server_errors_retry_until_success() {
        let server = MockServer::start(vec![
            MockResponse::json(503, r#"{"error":{"message":"down"}}"#),
            MockResponse::json(200, r#"{"choices":[{"message":{"content":"Cleaned."}}]}"#),
        ]);
        let client = test_cleanup_client(server.base_url());

        let result = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect("cleanup should eventually succeed");

        assert_eq!(result, "Cleaned.");
        assert_eq!(server.request_count(), 2);
    }

    #[tokio::test]
    async fn cleanup_server_errors_fail_after_retry_exhaustion() {
        let server = MockServer::start(vec![
            MockResponse::json(500, r#"{"error":{"message":"down"}}"#),
            MockResponse::json(500, r#"{"error":{"message":"down"}}"#),
            MockResponse::json(500, r#"{"error":{"message":"down"}}"#),
        ]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("server errors should fail after retries");

        assert_eq!(error.code, GroqCleanupErrorCode::ServerError);
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn cleanup_rejects_markdown_fenced_output() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"```\nCleaned.\n```"}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("markdown output should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ValidationFailed);
        assert_eq!(error.model, GROQ_CLEANUP_MODEL);
        assert_eq!(server.request_count(), 1);
    }

    #[tokio::test]
    async fn cleanup_rejects_json_like_output() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"{\"text\":\"Cleaned.\"}"}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("json output should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn cleanup_rejects_yaml_like_output() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"text: Cleaned."}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("yaml output should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn cleanup_rejects_overly_long_output() {
        let long = "word ".repeat(100);
        let body = format!(r#"{{"choices":[{{"message":{{"content":"{}"}}}}]}}"#, long);
        let leaked: &'static str = Box::leak(body.into_boxed_str());
        let server = MockServer::start(vec![MockResponse::json(200, leaked)]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("overly long output should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ValidationFailed);
    }

    #[test]
    fn cleanup_token_limit_uses_bounded_word_count() {
        assert_eq!(cleanup_max_tokens_for("short text"), 64);

        let medium = "word ".repeat(100);
        assert_eq!(cleanup_max_tokens_for(&medium), 200);

        let large = "word ".repeat(300);
        assert_eq!(cleanup_max_tokens_for(&large), 600);

        let very_large = "word ".repeat(2_000);
        assert_eq!(cleanup_max_tokens_for(&very_large), 1024);
    }

    #[tokio::test]
    async fn cleanup_rejects_commentary_prefixed_output() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"Corrected: Cleaned."}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("commentary output should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn cleanup_rejects_sure_prefixed_output() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"Sure, here is the cleaned text."}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("sure-prefixed output should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn cleanup_rejects_okay_prefixed_output() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"Okay, ich habe den Text bereinigt."}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("okay-prefixed output should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn cleanup_rejects_here_you_go_prefixed_output() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"Here you go, Cleaned transcript."}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("here-you-go-prefixed output should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn cleanup_rejects_empty_output() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"   "}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("empty output should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn cleanup_unreachable_api_retries_and_reports_unreachable() {
        let server = BrokenServer::start(3);
        let client = GroqCleanupClient::for_test(
            server.base_url(),
            Duration::from_secs(1),
            3,
            Duration::ZERO,
        );

        let error = client
            .cleanup_transcript("gsk_test_key", "raw")
            .await
            .expect_err("unreachable api should fail");

        assert_eq!(error.code, GroqCleanupErrorCode::ApiUnreachable);
        assert_eq!(server.request_count(), 3);
    }

    fn test_client(base_url: String) -> GroqTranscriptionClient {
        GroqTranscriptionClient::for_test(
            base_url,
            Duration::from_secs(5),
            3,
            Duration::from_millis(10),
        )
    }

    fn test_cleanup_client(base_url: String) -> GroqCleanupClient {
        GroqCleanupClient::for_test(base_url, Duration::from_secs(5), 3, Duration::ZERO)
    }

    fn extract_request_body(request: &str) -> String {
        let marker = "\r\n\r\n";
        if let Some(index) = request.find(marker) {
            return request[index + marker.len()..].to_string();
        }
        String::new()
    }

    fn minimal_wav() -> Vec<u8> {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&38_u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&32_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&2_u32.to_le_bytes());
        wav.extend_from_slice(&0_i16.to_le_bytes());
        wav
    }

    struct MockServer {
        addr: String,
        requests: Arc<Mutex<Vec<String>>>,
        request_count: Arc<AtomicUsize>,
    }

    struct MockResponse {
        status: u16,
        body: &'static str,
        delay: Duration,
        headers: Vec<(&'static str, &'static str)>,
    }

    impl MockResponse {
        fn json(status: u16, body: &'static str) -> Self {
            Self {
                status,
                body,
                delay: Duration::ZERO,
                headers: Vec::new(),
            }
        }

        fn slow_json(status: u16, body: &'static str, delay: Duration) -> Self {
            Self {
                status,
                body,
                delay,
                headers: Vec::new(),
            }
        }

        fn with_header(mut self, name: &'static str, value: &'static str) -> Self {
            self.headers.push((name, value));
            self
        }
    }

    impl MockServer {
        fn start(responses: Vec<MockResponse>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("mock server should bind");
            let addr = listener.local_addr().unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let request_count = Arc::new(AtomicUsize::new(0));
            let thread_requests = Arc::clone(&requests);
            let thread_count = Arc::clone(&request_count);

            thread::spawn(move || {
                for response in responses {
                    if let Ok((mut stream, _)) = listener.accept() {
                        let connection_requests = Arc::clone(&thread_requests);
                        let connection_count = Arc::clone(&thread_count);
                        thread::spawn(move || {
                            let request = read_request(&mut stream);
                            connection_count.fetch_add(1, Ordering::SeqCst);
                            connection_requests.lock().unwrap().push(request);
                            if !response.delay.is_zero() {
                                thread::sleep(response.delay);
                            }
                            write_response(&mut stream, response);
                        });
                    }
                }
            });

            Self {
                addr: format!("http://{addr}"),
                requests,
                request_count,
            }
        }

        fn base_url(&self) -> String {
            self.addr.clone()
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }

        fn request_count(&self) -> usize {
            self.request_count.load(Ordering::SeqCst)
        }
    }

    struct BrokenServer {
        addr: String,
        request_count: Arc<AtomicUsize>,
    }

    impl BrokenServer {
        fn start(max_connections: usize) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("broken server should bind");
            let addr = listener.local_addr().unwrap();
            let request_count = Arc::new(AtomicUsize::new(0));
            let thread_count = Arc::clone(&request_count);

            thread::spawn(move || {
                for _ in 0..max_connections {
                    if let Ok((mut stream, _)) = listener.accept() {
                        let _ = read_request(&mut stream);
                        thread_count.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });

            Self {
                addr: format!("http://{addr}"),
                request_count,
            }
        }

        fn base_url(&self) -> String {
            self.addr.clone()
        }

        fn request_count(&self) -> usize {
            self.request_count.load(Ordering::SeqCst)
        }
    }

    fn read_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout should set");
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 4096];
        let mut expected_len = None;

        loop {
            let read = stream.read(&mut chunk).unwrap_or(0);
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);

            if expected_len.is_none() {
                if let Some(header_end) = find_subsequence(&buffer, b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&buffer[..header_end]);
                    let content_len = headers
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value)
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    expected_len = Some(header_end + 4 + content_len);
                }
            }

            if expected_len.is_some_and(|len| buffer.len() >= len) {
                break;
            }
        }

        String::from_utf8_lossy(&buffer).to_string()
    }

    fn write_response(stream: &mut TcpStream, response: MockResponse) {
        let status_text = match response.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            503 => "Service Unavailable",
            _ => "Error",
        };
        let headers = response
            .headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let raw = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nConnection: close\r\n{}Content-Length: {}\r\n\r\n{}",
            response.status,
            status_text,
            headers,
            response.body.len(),
            response.body
        );
        let _ = stream.write_all(raw.as_bytes());
        let _ = stream.flush();
    }

    fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }
}
