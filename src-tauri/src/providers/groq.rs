use std::time::{Duration, Instant};

use reqwest::{header::HeaderMap, multipart, StatusCode};
use serde::{Deserialize, Serialize};

const GROQ_BASE_URL: &str = "https://api.groq.com";
const TRANSCRIPTIONS_PATH: &str = "/openai/v1/audio/transcriptions";
const CHAT_COMPLETIONS_PATH: &str = "/openai/v1/chat/completions";
const GROQ_STT_MODEL: &str = "whisper-large-v3-turbo";
const GROQ_CLEANUP_MODEL: &str = "llama-3.1-8b-instant";
const STT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CLEANUP_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_ATTEMPTS: usize = 3;
const INITIAL_RETRY_BACKOFF: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);
const CLEANUP_SYSTEM_PROMPT: &str = "You are a transcript cleanup engine for a push-to-talk dictation app.\n\nYour job:\n\n- Correct capitalization.\n- Correct punctuation.\n- Fix obvious speech-to-text transcription errors only when the intended word is clear.\n- Preserve the user's language.\n- Preserve the user's meaning.\n- Preserve the user's tone.\n- Do not summarize.\n- Do not rewrite stylistically.\n- Do not add new information.\n- Do not remove information.\n- Do not answer the text.\n- Return only the cleaned transcript text.";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqTranscription {
    pub text: String,
    pub model: String,
    pub retry_count: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqTranscriptionError {
    pub code: GroqTranscriptionErrorCode,
    pub message: String,
    pub model: String,
    pub retry_count: u32,
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
    max_attempts: usize,
    retry_backoff: Duration,
}

struct AttemptError {
    error: GroqTranscriptionError,
    retryable: bool,
}

#[derive(Deserialize)]
struct TranscriptionResponse {
    text: Option<String>,
}

impl GroqTranscriptionClient {
    pub fn new() -> Result<Self, GroqTranscriptionError> {
        Self::with_config(
            GROQ_BASE_URL.to_string(),
            STT_REQUEST_TIMEOUT,
            MAX_ATTEMPTS,
            INITIAL_RETRY_BACKOFF,
        )
    }

    fn with_config(
        base_url: String,
        timeout: Duration,
        max_attempts: usize,
        retry_backoff: Duration,
    ) -> Result<Self, GroqTranscriptionError> {
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|_| server_error())?;

        Ok(Self {
            http_client,
            base_url,
            max_attempts: max_attempts.max(1),
            retry_backoff,
        })
    }

    #[cfg(test)]
    fn for_test(
        base_url: String,
        timeout: Duration,
        max_attempts: usize,
        retry_backoff: Duration,
    ) -> Self {
        Self::with_config(base_url, timeout, max_attempts, retry_backoff)
            .expect("test client should build")
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
                    tokio::time::sleep(retry_delay(self.retry_backoff, attempt)).await;
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
            .part("file", file_part);
        let response = self
            .http_client
            .post(self.transcriptions_url())
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(classify_request_error)?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            return parse_transcription_response(&body).map_err(non_retryable);
        }

        Err(classify_http_error(status, &body))
    }

    fn transcriptions_url(&self) -> String {
        format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            TRANSCRIPTIONS_PATH
        )
    }
}

fn parse_transcription_response(body: &str) -> Result<GroqTranscription, GroqTranscriptionError> {
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

fn classify_http_error(status: StatusCode, body: &str) -> AttemptError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => non_retryable(groq_error(
            GroqTranscriptionErrorCode::InvalidApiKey,
            "The configured Groq API key was rejected.",
        )),
        StatusCode::REQUEST_TIMEOUT => retryable(groq_error(
            GroqTranscriptionErrorCode::Timeout,
            "The Groq transcription request timed out.",
        )),
        StatusCode::TOO_MANY_REQUESTS => retryable(groq_error(
            GroqTranscriptionErrorCode::RateLimit,
            "Groq rate limited the transcription request. Try again shortly.",
        )),
        StatusCode::BAD_REQUEST => non_retryable(groq_error(
            invalid_request_code(body),
            invalid_request_message(body),
        )),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => non_retryable(groq_error(
            GroqTranscriptionErrorCode::UnsupportedAudio,
            "Groq could not transcribe the uploaded audio file.",
        )),
        status if status.is_server_error() => retryable(groq_error(
            GroqTranscriptionErrorCode::ServerError,
            "Groq could not complete the transcription request.",
        )),
        _ => non_retryable(groq_error(
            GroqTranscriptionErrorCode::InvalidRequest,
            "Groq rejected the transcription request.",
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
    }
}

fn non_retryable(error: GroqTranscriptionError) -> AttemptError {
    AttemptError {
        error,
        retryable: false,
    }
}

fn server_error() -> GroqTranscriptionError {
    groq_error(
        GroqTranscriptionErrorCode::ServerError,
        "Groq transcription could not be initialized.",
    )
}

fn groq_error(code: GroqTranscriptionErrorCode, message: &'static str) -> GroqTranscriptionError {
    GroqTranscriptionError {
        code,
        message: message.to_string(),
        model: GROQ_STT_MODEL.to_string(),
        retry_count: 0,
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqCleanup {
    pub text: String,
    pub model: String,
    pub retry_count: u32,
    pub validation_ms: u64,
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
    pub fn new() -> Result<Self, GroqCleanupError> {
        Self::with_config(
            GROQ_BASE_URL.to_string(),
            CLEANUP_REQUEST_TIMEOUT,
            MAX_ATTEMPTS,
            INITIAL_RETRY_BACKOFF,
        )
    }

    fn with_config(
        base_url: String,
        timeout: Duration,
        max_attempts: usize,
        retry_backoff: Duration,
    ) -> Result<Self, GroqCleanupError> {
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|_| cleanup_server_error())?;

        Ok(Self {
            http_client,
            base_url,
            max_attempts: max_attempts.max(1),
            retry_backoff,
        })
    }

    #[cfg(test)]
    fn for_test(
        base_url: String,
        timeout: Duration,
        max_attempts: usize,
        retry_backoff: Duration,
    ) -> Self {
        Self::with_config(base_url, timeout, max_attempts, retry_backoff)
            .expect("test client should build")
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
            .bearer_auth(api_key)
            .json(&cleanup_request_body(transcript))
            .send()
            .await
            .map_err(classify_cleanup_request_error)?;
        let status = response.status();
        let retry_after = cleanup_retry_after(response.headers());
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            return parse_cleanup_response(&body, transcript).map_err(cleanup_non_retryable);
        }

        Err(classify_cleanup_http_error(status, retry_after))
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
                content: format!(
                    "Clean this transcript:\n\n<transcript>\n{transcript}\n</transcript>"
                ),
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
        .clamp(64, 512)
        .try_into()
        .unwrap_or(512)
}

fn parse_cleanup_response(body: &str, input: &str) -> Result<GroqCleanup, GroqCleanupError> {
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
        "here's the cleaned text:",
        "here is the cleaned text:",
        "cleaned transcript:",
        "the cleaned transcript is:",
    ];

    rejected_prefixes
        .iter()
        .any(|prefix| lower.starts_with(prefix))
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
) -> CleanupAttemptError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            cleanup_non_retryable(groq_cleanup_error(
                GroqCleanupErrorCode::InvalidApiKey,
                "The configured Groq API key was rejected.",
            ))
        }
        StatusCode::BAD_REQUEST => cleanup_non_retryable(groq_cleanup_error(
            GroqCleanupErrorCode::InvalidRequest,
            "Groq rejected the cleanup request.",
        )),
        StatusCode::REQUEST_TIMEOUT => cleanup_retryable_with_retry_after(
            groq_cleanup_error(
                GroqCleanupErrorCode::Timeout,
                "The Groq cleanup request timed out.",
            ),
            retry_after,
        ),
        StatusCode::TOO_MANY_REQUESTS => cleanup_retryable_with_retry_after(
            groq_cleanup_error(
                GroqCleanupErrorCode::RateLimit,
                "Groq rate limited the cleanup request. Try again shortly.",
            ),
            retry_after,
        ),
        status if status.is_server_error() => cleanup_retryable_with_retry_after(
            groq_cleanup_error(
                GroqCleanupErrorCode::ServerError,
                "Groq could not complete the cleanup request.",
            ),
            retry_after,
        ),
        _ => cleanup_non_retryable(groq_cleanup_error(
            GroqCleanupErrorCode::InvalidRequest,
            "Groq rejected the cleanup request.",
        )),
    }
}

fn cleanup_retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
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
    GroqCleanupError {
        code,
        message: message.to_string(),
        model: GROQ_CLEANUP_MODEL.to_string(),
        retry_count: 0,
        validation_ms: 0,
    }
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
        assert!(request.contains("recording.wav"));
        assert!(request.contains("audio/wav"));
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
        assert!(request.contains("Clean this transcript"));
        assert!(request.contains("<transcript>"));
        assert!(request.contains("raw transcript"));
    }

    #[tokio::test]
    async fn cleanup_request_uses_llama_instant_and_sends_no_gpt_oss_parameters() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"Cleaned."}}]}"#,
        )]);
        let client = test_cleanup_client(server.base_url());

        let result = client
            .cleanup_transcript("gsk_test_key", "raw transcript")
            .await
            .expect("cleanup should succeed");

        assert_eq!(result.model, "llama-3.1-8b-instant");
        assert_eq!(GROQ_CLEANUP_MODEL, "llama-3.1-8b-instant");

        let request = server.requests()[0].clone();
        let body = extract_request_body(&request);
        let body_lower = body.to_ascii_lowercase();
        assert!(!body_lower.contains("gpt-oss"));
        assert!(!body_lower.contains("reasoning_effort"));
        assert!(!body_lower.contains("\"reasoning\""));
        assert!(body.contains(r#""temperature":0"#));
        assert!(body_lower.contains("\"max_tokens\":64"));
        assert!(body.contains(r#""model":"llama-3.1-8b-instant""#));
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

    #[test]
    fn cleanup_token_limit_uses_bounded_word_count() {
        assert_eq!(cleanup_max_tokens_for("short text"), 64);

        let medium = "word ".repeat(100);
        assert_eq!(cleanup_max_tokens_for(&medium), 200);

        let large = "word ".repeat(300);
        assert_eq!(cleanup_max_tokens_for(&large), 512);
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
        GroqTranscriptionClient::for_test(base_url, Duration::from_secs(2), 3, Duration::ZERO)
    }

    fn test_cleanup_client(base_url: String) -> GroqCleanupClient {
        GroqCleanupClient::for_test(base_url, Duration::from_secs(2), 3, Duration::ZERO)
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
    }

    impl MockResponse {
        fn json(status: u16, body: &'static str) -> Self {
            Self {
                status,
                body,
                delay: Duration::ZERO,
            }
        }

        fn slow_json(status: u16, body: &'static str, delay: Duration) -> Self {
            Self {
                status,
                body,
                delay,
            }
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
                        .find_map(|line| line.strip_prefix("Content-Length: "))
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
        let raw = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            response.status,
            status_text,
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
