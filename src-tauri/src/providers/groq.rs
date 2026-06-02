use std::time::Duration;

use reqwest::{multipart, StatusCode};
use serde::{Deserialize, Serialize};

const GROQ_BASE_URL: &str = "https://api.groq.com";
const TRANSCRIPTIONS_PATH: &str = "/openai/v1/audio/transcriptions";
const GROQ_STT_MODEL: &str = "whisper-large-v3-turbo";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ATTEMPTS: usize = 3;
const INITIAL_RETRY_BACKOFF: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqTranscription {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqTranscriptionError {
    pub code: GroqTranscriptionErrorCode,
    pub message: String,
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
                Ok(transcription) => return Ok(transcription),
                Err(attempt_error) if attempt_error.retryable && attempt < self.max_attempts => {
                    tokio::time::sleep(retry_delay(self.retry_backoff, attempt)).await;
                }
                Err(attempt_error) => return Err(attempt_error.error),
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

    Ok(GroqTranscription { text })
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
        GroqTranscriptionClient, GroqTranscriptionErrorCode, GROQ_STT_MODEL, TRANSCRIPTIONS_PATH,
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
        assert_eq!(server.request_count(), 3);
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

    fn test_client(base_url: String) -> GroqTranscriptionClient {
        GroqTranscriptionClient::for_test(base_url, Duration::from_secs(2), 3, Duration::ZERO)
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
