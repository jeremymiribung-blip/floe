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

use super::cleanup::{GroqCleanupClient, CHAT_COMPLETIONS_PATH, GROQ_CLEANUP_MODEL};
use super::stt::{GroqTranscriptionClient, GROQ_STT_MODEL, TRANSCRIPTIONS_PATH};
use super::types::{GroqCleanupErrorCode, GroqTranscriptionErrorCode};

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
        .contains("You are an expert transcript cleanup engine for a push-to-talk dictation app."));
    assert!(request.contains("Preserve all languages and code-switching exactly as in the input."));
    assert!(request.contains("Absolute constraints:"));
    assert!(!request.contains("Clean this transcript"));
    assert!(request.contains("<transcript>"));
    assert!(request.contains("raw transcript"));
}

#[tokio::test]
async fn cleanup_request_uses_llama_and_sends_no_unsupported_parameters() {
    let server = MockServer::start(vec![MockResponse::json(
        200,
        r#"{"choices":[{"message":{"content":"Cleaned."}}]}"#,
    )]);
    let client = test_cleanup_client(server.base_url());

    let result = client
        .cleanup_transcript("gsk_test_key", "raw transcript")
        .await
        .expect("cleanup should succeed");

    assert_eq!(result.model, GROQ_CLEANUP_MODEL);

    let request = server.requests()[0].clone();
    let body = extract_request_body(&request);
    let body_lower = body.to_ascii_lowercase();
    assert!(!body_lower.contains("gpt-oss"));
    assert!(!body_lower.contains("reasoning_effort"));
    assert!(!body_lower.contains("\"reasoning\""));
    assert!(!body_lower.contains("qwen"));
    assert!(!body_lower.contains("cerebras"));
    assert!(body.contains(r#""temperature":0"#));
    assert!(body_lower.contains("\"max_tokens\":64"));
    assert!(body.contains(r#""model":"qwen/qwen3.6-27b""#));
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
    let client =
        GroqCleanupClient::for_test(server.base_url(), Duration::from_secs(2), 2, Duration::ZERO);

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
    assert_eq!(super::cleanup::cleanup_max_tokens_for("short text"), 64);

    let medium = "word ".repeat(100);
    assert_eq!(super::cleanup::cleanup_max_tokens_for(&medium), 200);

    let large = "word ".repeat(300);
    assert_eq!(super::cleanup::cleanup_max_tokens_for(&large), 600);

    let very_large = "word ".repeat(2_000);
    assert_eq!(super::cleanup::cleanup_max_tokens_for(&very_large), 1024);
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
    let client =
        GroqCleanupClient::for_test(server.base_url(), Duration::from_secs(1), 3, Duration::ZERO);

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
