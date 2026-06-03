use std::time::Duration;

use reqwest::{header::HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};

const CEREBRAS_BASE_URL: &str = "https://api.cerebras.ai/v1";
const CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const CEREBRAS_MODEL: &str = "gpt-oss-120b";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_ATTEMPTS: usize = 3;
const INITIAL_RETRY_BACKOFF: Duration = Duration::from_millis(250);
const SYSTEM_PROMPT: &str = "You are a transcript cleanup engine for a push-to-talk dictation app.\n\nYour job:\n\n- Correct capitalization.\n- Correct punctuation.\n- Fix obvious speech-to-text transcription errors only when the intended word is clear.\n- Preserve the user's language.\n- Preserve the user's meaning.\n- Preserve the user's tone.\n- Do not summarize.\n- Do not rewrite stylistically.\n- Do not add new information.\n- Do not remove information.\n- Do not answer the text.\n- Return only the cleaned transcript text.";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CerebrasCleanupError {
    pub code: CerebrasCleanupErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CerebrasCleanupErrorCode {
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

impl CerebrasCleanupError {
    #[cfg(test)]
    pub fn new(code: CerebrasCleanupErrorCode, message: &'static str) -> Self {
        cerebras_error(code, message)
    }
}

pub struct CerebrasCleanupClient {
    http_client: reqwest::Client,
    base_url: String,
    max_attempts: usize,
    retry_backoff: Duration,
}

struct AttemptError {
    error: CerebrasCleanupError,
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

impl CerebrasCleanupClient {
    pub fn new() -> Result<Self, CerebrasCleanupError> {
        Self::with_config(
            CEREBRAS_BASE_URL.to_string(),
            REQUEST_TIMEOUT,
            MAX_ATTEMPTS,
            INITIAL_RETRY_BACKOFF,
        )
    }

    fn with_config(
        base_url: String,
        timeout: Duration,
        max_attempts: usize,
        retry_backoff: Duration,
    ) -> Result<Self, CerebrasCleanupError> {
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

    pub async fn clean_transcript(
        &self,
        api_key: &str,
        transcript: &str,
    ) -> Result<String, CerebrasCleanupError> {
        let api_key = api_key.trim();

        if api_key.is_empty() {
            return Err(cerebras_error(
                CerebrasCleanupErrorCode::MissingApiKey,
                "Configure a Cerebras API key before using Clean cleanup.",
            ));
        }

        if transcript.trim().is_empty() {
            return Err(cerebras_error(
                CerebrasCleanupErrorCode::EmptyTranscript,
                "There is no transcript text to clean.",
            ));
        }

        for attempt in 1..=self.max_attempts {
            match self.send_once(api_key, transcript).await {
                Ok(cleaned) => return Ok(cleaned),
                Err(attempt_error) if attempt_error.retryable && attempt < self.max_attempts => {
                    tokio::time::sleep(attempt_error.delay(self.retry_backoff, attempt)).await;
                }
                Err(attempt_error) => return Err(attempt_error.error),
            }
        }

        Err(server_error())
    }

    async fn send_once(&self, api_key: &str, transcript: &str) -> Result<String, AttemptError> {
        let response = self
            .http_client
            .post(self.chat_completions_url())
            .bearer_auth(api_key)
            .json(&request_body(transcript))
            .send()
            .await
            .map_err(classify_request_error)?;
        let status = response.status();
        let retry_after = retry_after(response.headers());
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            return parse_cleanup_response(&body, transcript).map_err(non_retryable);
        }

        Err(classify_http_error(status, retry_after))
    }

    fn chat_completions_url(&self) -> String {
        format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            CHAT_COMPLETIONS_PATH
        )
    }
}

impl AttemptError {
    fn delay(&self, base: Duration, attempt: usize) -> Duration {
        self.retry_after
            .unwrap_or_else(|| retry_delay(base, attempt))
            .min(Duration::from_secs(5))
    }
}

fn request_body(transcript: &str) -> ChatCompletionRequest {
    ChatCompletionRequest {
        model: CEREBRAS_MODEL,
        messages: vec![
            ChatMessage {
                role: "system",
                content: SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user",
                content: format!(
                    "Clean this transcript:\n\n<transcript>\n{transcript}\n</transcript>"
                ),
            },
        ],
        temperature: 0.0,
        max_tokens: max_tokens_for(transcript),
    }
}

fn max_tokens_for(transcript: &str) -> u32 {
    let estimated_tokens = transcript.chars().count().saturating_div(3) + 64;

    estimated_tokens.clamp(64, 2048) as u32
}

fn parse_cleanup_response(body: &str, input: &str) -> Result<String, CerebrasCleanupError> {
    let response: ChatCompletionResponse = serde_json::from_str(body).map_err(|_| {
        cerebras_error(
            CerebrasCleanupErrorCode::MalformedResponse,
            "Cerebras returned a cleanup response Floe could not read.",
        )
    })?;
    let content = response
        .choices
        .and_then(|choices| choices.into_iter().next())
        .and_then(|choice| choice.message)
        .and_then(|message| message.content)
        .ok_or_else(|| {
            cerebras_error(
                CerebrasCleanupErrorCode::MalformedResponse,
                "Cerebras returned a cleanup response without text.",
            )
        })?;

    validate_cleanup_output(input, &content)
}

pub fn validate_cleanup_output(input: &str, output: &str) -> Result<String, CerebrasCleanupError> {
    let trimmed = output.trim();

    if trimmed.is_empty() {
        return Err(validation_error());
    }

    let input_len = input.chars().count();
    let output_len = trimmed.chars().count();
    let max_len = input_len.saturating_mul(2).max(input_len + 200);

    if output_len > max_len || looks_like_markdown(trimmed) || looks_like_commentary(trimmed) {
        return Err(validation_error());
    }

    Ok(trimmed.to_string())
}

fn looks_like_markdown(value: &str) -> bool {
    let trimmed = value.trim_start();

    trimmed.starts_with("```")
        || trimmed.starts_with('#')
        || trimmed.contains("\n```")
        || trimmed.contains("\n- ")
        || trimmed.contains("\n* ")
}

fn looks_like_commentary(value: &str) -> bool {
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

fn classify_request_error(error: reqwest::Error) -> AttemptError {
    if error.is_timeout() {
        return retryable(
            cerebras_error(
                CerebrasCleanupErrorCode::Timeout,
                "The Cerebras cleanup request timed out.",
            ),
            None,
        );
    }

    retryable(
        cerebras_error(
            CerebrasCleanupErrorCode::ApiUnreachable,
            "Cerebras could not be reached. Floe used Fast cleanup instead.",
        ),
        None,
    )
}

fn classify_http_error(status: StatusCode, retry_after: Option<Duration>) -> AttemptError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => non_retryable(cerebras_error(
            CerebrasCleanupErrorCode::InvalidApiKey,
            "The configured Cerebras API key was rejected.",
        )),
        StatusCode::BAD_REQUEST => non_retryable(cerebras_error(
            CerebrasCleanupErrorCode::InvalidRequest,
            "Cerebras rejected the cleanup request.",
        )),
        StatusCode::REQUEST_TIMEOUT => retryable(
            cerebras_error(
                CerebrasCleanupErrorCode::Timeout,
                "The Cerebras cleanup request timed out.",
            ),
            retry_after,
        ),
        StatusCode::TOO_MANY_REQUESTS => retryable(
            cerebras_error(
                CerebrasCleanupErrorCode::RateLimit,
                "Cerebras rate limited cleanup. Floe used Fast cleanup instead.",
            ),
            retry_after,
        ),
        status if status.is_server_error() => retryable(
            cerebras_error(
                CerebrasCleanupErrorCode::ServerError,
                "Cerebras could not complete cleanup. Floe used Fast cleanup instead.",
            ),
            retry_after,
        ),
        _ => non_retryable(cerebras_error(
            CerebrasCleanupErrorCode::InvalidRequest,
            "Cerebras rejected the cleanup request.",
        )),
    }
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn retry_delay(base: Duration, attempt: usize) -> Duration {
    let multiplier = 1_u32.checked_shl((attempt - 1) as u32).unwrap_or(u32::MAX);
    base.saturating_mul(multiplier)
}

fn retryable(error: CerebrasCleanupError, retry_after: Option<Duration>) -> AttemptError {
    AttemptError {
        error,
        retryable: true,
        retry_after,
    }
}

fn non_retryable(error: CerebrasCleanupError) -> AttemptError {
    AttemptError {
        error,
        retryable: false,
        retry_after: None,
    }
}

fn validation_error() -> CerebrasCleanupError {
    cerebras_error(
        CerebrasCleanupErrorCode::ValidationFailed,
        "Cerebras cleanup returned text Floe could not safely use.",
    )
}

fn server_error() -> CerebrasCleanupError {
    cerebras_error(
        CerebrasCleanupErrorCode::ServerError,
        "Cerebras cleanup could not be initialized.",
    )
}

fn cerebras_error(code: CerebrasCleanupErrorCode, message: &'static str) -> CerebrasCleanupError {
    CerebrasCleanupError {
        code,
        message: message.to_string(),
    }
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
        validate_cleanup_output, CerebrasCleanupClient, CerebrasCleanupErrorCode, CEREBRAS_MODEL,
        CHAT_COMPLETIONS_PATH,
    };

    #[tokio::test]
    async fn successful_request_sends_expected_chat_payload() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"Hello, world."}}]}"#,
        )]);
        let client = test_client(server.base_url());

        let result = client
            .clean_transcript("csk_test_key", "hello world")
            .await
            .expect("cleanup should succeed");

        assert_eq!(result, "Hello, world.");
        assert_eq!(server.request_count(), 1);

        let request = server.requests()[0].clone();
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with(&format!("POST {CHAT_COMPLETIONS_PATH} HTTP/1.1")));
        assert!(request_lower.contains("authorization: bearer csk_test_key"));
        assert!(request.contains(CEREBRAS_MODEL));
        assert!(request.contains("Clean this transcript"));
        assert!(request.contains("<transcript>"));
        assert!(request.contains("hello world"));
    }

    #[tokio::test]
    async fn missing_key_and_empty_transcript_are_not_retried() {
        let server = MockServer::start(vec![MockResponse::json(
            200,
            r#"{"choices":[{"message":{"content":"unused"}}]}"#,
        )]);
        let client = test_client(server.base_url());

        let missing_key = client
            .clean_transcript("   ", "hello")
            .await
            .expect_err("missing key should fail");
        let empty = client
            .clean_transcript("csk_test_key", "   ")
            .await
            .expect_err("empty transcript should fail");

        assert_eq!(missing_key.code, CerebrasCleanupErrorCode::MissingApiKey);
        assert_eq!(empty.code, CerebrasCleanupErrorCode::EmptyTranscript);
        assert_eq!(server.request_count(), 0);
    }

    #[tokio::test]
    async fn invalid_key_and_bad_request_are_not_retried() {
        for (status, expected) in [
            (401, CerebrasCleanupErrorCode::InvalidApiKey),
            (403, CerebrasCleanupErrorCode::InvalidApiKey),
            (400, CerebrasCleanupErrorCode::InvalidRequest),
        ] {
            let server = MockServer::start(vec![MockResponse::json(status, r#"{"error":{}}"#)]);
            let client = test_client(server.base_url());

            let error = client
                .clean_transcript("csk_test_key", "hello")
                .await
                .expect_err("non-retryable status should fail");

            assert_eq!(error.code, expected);
            assert_eq!(server.request_count(), 1);
        }
    }

    #[tokio::test]
    async fn retryable_statuses_use_bounded_attempts() {
        for (status, expected) in [
            (408, CerebrasCleanupErrorCode::Timeout),
            (429, CerebrasCleanupErrorCode::RateLimit),
            (500, CerebrasCleanupErrorCode::ServerError),
            (503, CerebrasCleanupErrorCode::ServerError),
        ] {
            let server = MockServer::start(vec![
                MockResponse::json(status, r#"{"error":"temporary"}"#),
                MockResponse::json(status, r#"{"error":"temporary"}"#),
                MockResponse::json(status, r#"{"error":"temporary"}"#),
            ]);
            let client = test_client(server.base_url());

            let error = client
                .clean_transcript("csk_test_key", "hello")
                .await
                .expect_err("retryable status should exhaust attempts");

            assert_eq!(error.code, expected);
            assert_eq!(server.request_count(), 3);
        }
    }

    #[tokio::test]
    async fn retryable_statuses_can_recover() {
        let server = MockServer::start(vec![
            MockResponse::json(429, r#"{"error":"rate limited"}"#),
            MockResponse::json(200, r#"{"choices":[{"message":{"content":"Recovered."}}]}"#),
        ]);
        let client = test_client(server.base_url());

        let result = client
            .clean_transcript("csk_test_key", "recovered")
            .await
            .expect("retry should recover");

        assert_eq!(result, "Recovered.");
        assert_eq!(server.request_count(), 2);
    }

    #[tokio::test]
    async fn timeout_retries_and_reports_timeout() {
        let server = MockServer::start(vec![
            MockResponse::slow_json(
                200,
                r#"{"choices":[{"message":{"content":"Late."}}]}"#,
                Duration::from_millis(100),
            ),
            MockResponse::slow_json(
                200,
                r#"{"choices":[{"message":{"content":"Late."}}]}"#,
                Duration::from_millis(100),
            ),
            MockResponse::slow_json(
                200,
                r#"{"choices":[{"message":{"content":"Late."}}]}"#,
                Duration::from_millis(100),
            ),
        ]);
        let client = CerebrasCleanupClient::for_test(
            server.base_url(),
            Duration::from_millis(20),
            3,
            Duration::ZERO,
        );

        let error = client
            .clean_transcript("csk_test_key", "hello")
            .await
            .expect_err("timeouts should fail after retries");

        assert_eq!(error.code, CerebrasCleanupErrorCode::Timeout);
        assert_eq!(server.request_count(), 3);
    }

    #[tokio::test]
    async fn malformed_or_invalid_success_response_fails_without_retry() {
        for body in [
            r#"{"value":"no choices"}"#,
            r#"{"choices":[{"message":{"content":""}}]}"#,
            r#"{"choices":[{"message":{"content":"Corrected: Hello."}}]}"#,
            r#"{"choices":[{"message":{"content":"```text\nHello.\n```"}}]}"#,
        ] {
            let server = MockServer::start(vec![MockResponse::json(200, body)]);
            let client = test_client(server.base_url());

            let error = client
                .clean_transcript("csk_test_key", "hello")
                .await
                .expect_err("invalid success response should fail");

            assert!(
                matches!(
                    error.code,
                    CerebrasCleanupErrorCode::MalformedResponse
                        | CerebrasCleanupErrorCode::ValidationFailed
                ),
                "unexpected code: {:?}",
                error.code
            );
            assert_eq!(server.request_count(), 1);
        }
    }

    #[test]
    fn validation_rejects_empty_long_markdown_and_commentary() {
        assert_eq!(
            validate_cleanup_output("hello", " Hello. ").unwrap(),
            "Hello."
        );
        assert_eq!(
            validate_cleanup_output("hello", "").unwrap_err().code,
            CerebrasCleanupErrorCode::ValidationFailed
        );
        assert_eq!(
            validate_cleanup_output("hello", &"x".repeat(250))
                .unwrap_err()
                .code,
            CerebrasCleanupErrorCode::ValidationFailed
        );
        assert_eq!(
            validate_cleanup_output("hello", "# Hello")
                .unwrap_err()
                .code,
            CerebrasCleanupErrorCode::ValidationFailed
        );
        assert_eq!(
            validate_cleanup_output("hello", "Output: Hello.")
                .unwrap_err()
                .code,
            CerebrasCleanupErrorCode::ValidationFailed
        );
    }

    fn test_client(base_url: String) -> CerebrasCleanupClient {
        CerebrasCleanupClient::for_test(base_url, Duration::from_secs(2), 3, Duration::ZERO)
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
            408 => "Request Timeout",
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
