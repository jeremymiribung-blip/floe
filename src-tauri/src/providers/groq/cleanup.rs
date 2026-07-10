use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::prompts::cleanup::CLEANUP_SYSTEM_PROMPT;
use crate::providers::cleanup::{CleanupError, CleanupProvider, CleanupSuccess, RateLimitMetadata};

pub use super::types::{GroqCleanup, GroqCleanupError, GroqCleanupErrorCode, GROQ_CLEANUP_MODEL};
use super::util::{elapsed_ms, rate_limit_metadata, retry_after, retry_count_for_attempt};

pub const CHAT_COMPLETIONS_PATH: &str = "/openai/v1/chat/completions";
pub const CLEANUP_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

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

struct CleanupAttemptError {
    error: GroqCleanupError,
    retryable: bool,
    retry_after: Option<Duration>,
}

pub struct GroqCleanupClient {
    http_client: reqwest::Client,
    base_url: String,
    request_timeout: Duration,
    max_attempts: usize,
    retry_backoff: Duration,
}

impl GroqCleanupClient {
    pub fn new(http_client: reqwest::Client) -> Self {
        Self::with_config(
            http_client,
            super::stt::GROQ_BASE_URL.to_string(),
            CLEANUP_REQUEST_TIMEOUT,
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
                        .unwrap_or_else(|| super::util::retry_delay(self.retry_backoff, attempt))
                        .min(super::util::MAX_RETRY_DELAY);
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

fn map_rate_limit(rl: Box<RateLimitMetadata>) -> Box<RateLimitMetadata> {
    rl
}

#[async_trait]
impl CleanupProvider for GroqCleanupClient {
    async fn cleanup(
        &self,
        api_key: &str,
        transcript: &str,
    ) -> Result<CleanupSuccess, CleanupError> {
        match self.cleanup_transcript(api_key, transcript).await {
            Ok(result) => {
                let rate_limit = result.rate_limit.map(map_rate_limit);
                Ok(CleanupSuccess {
                    text: result.text,
                    model: result.model,
                    retry_count: result.retry_count,
                    validation_ms: result.validation_ms,
                    rate_limit,
                })
            }
            Err(error) => {
                let rate_limit = error.rate_limit.map(map_rate_limit);
                Err(CleanupError {
                    message: error.message,
                    model: error.model,
                    retry_count: error.retry_count,
                    validation_ms: error.validation_ms,
                    rate_limit,
                    error_code: Some(format!("{:?}", error.code)),
                })
            }
        }
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
        // AGENTS.md pins cleanup to Groq Qwen 3.6 27B (currently
        // Preview-tier on Groq). The Qwen-specific
        // `chat_template_kwargs: {"enable_thinking": false}` payload is
        // intentionally NOT set: the strict `validate_cleanup_output`
        // validator in this file already rejects Markdown / JSON / YAML /
        // commentary wrappers and would catch stray Qwen thinking tags,
        // triggering the documented `Cleanup failed` + raw-paste fallback.
        // If Qwen's output format ever forces it past the validator, add
        // it back here and update AGENTS.md.
    }
}

pub(crate) fn cleanup_max_tokens_for(transcript: &str) -> u32 {
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
    rate_limit: Option<RateLimitMetadata>,
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
    rate_limit: Option<RateLimitMetadata>,
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
    rate_limit: Option<RateLimitMetadata>,
) -> GroqCleanupError {
    GroqCleanupError {
        domain: "cleanup",
        code,
        message: message.to_string(),
        model: GROQ_CLEANUP_MODEL.to_string(),
        retry_count: 0,
        validation_ms: 0,
        rate_limit: rate_limit.map(Box::new),
    }
}
