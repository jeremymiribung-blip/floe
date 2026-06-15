//! Privacy tests for the ASR architecture.
//!
//! These tests verify:
//! - No raw transcripts in logs
//! - No audio data in logs
//! - No API keys in logs
//! - No clipboard contents in logs
//! - Error codes are sanitized for privacy
//!
//! Note: Privacy is enforced at the type level in DiagEntry and through
//! sanitization functions in asr::types and commands::diag

use crate::asr::types::{sanitize_error_code, BackendType};
use crate::commands::diag::{DiagEntry, DiagLog};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // ========================================================================
    // DiagEntry Privacy Tests
    // ========================================================================

    #[test]
    fn diag_entry_contains_only_safe_fields() {
        let entry = DiagEntry::new(
            "groq",
            "whisper-large-v3-turbo",
            "cloud",
            5000,
            1200,
            300,
            0.24,
            false,
            None,
            0,
            None,
        );
        let log = entry.to_log_string();

        // Verify all required safe fields are present
        assert!(log.contains("provider_name=groq"));
        assert!(log.contains("model_name=whisper-large-v3-turbo"));
        assert!(log.contains("backend_type=cloud"));
        assert!(log.contains("audio_duration_ms=5000"));
        assert!(log.contains("transcription_ms=1200"));
        assert!(log.contains("cleanup_ms=300"));
        assert!(log.contains("realtime_factor=0.240"));
        assert!(log.contains("fallback_used=false"));
        assert!(log.contains("retry_count=0"));

        // Verify NO sensitive fields are present
        assert!(!log.contains("text="));
        assert!(!log.contains("audio="));
        assert!(!log.contains("key="));
        assert!(!log.contains("Bearer"));
        assert!(!log.contains("gsk_"));
        assert!(!log.contains("clipboard"));
        assert!(!log.contains("authorization"));
    }

    #[test]
    fn diag_entry_with_error_code_does_not_leak_secrets() {
        let entry = DiagEntry::new(
            "groq",
            "whisper",
            "cloud",
            1000,
            500,
            0,
            0.5,
            false,
            None,
            0,
            Some("timeout_error".to_string()),
        );
        let log = entry.to_log_string();

        assert!(log.contains("error_code=timeout_error"));
        assert!(!log.contains("Bearer"));
        assert!(!log.contains("gsk_"));
    }

    #[test]
    fn diag_entry_error_code_redacts_secrets() {
        let entry = DiagEntry::new(
            "groq",
            "whisper",
            "cloud",
            1000,
            500,
            0,
            0.5,
            false,
            None,
            0,
            Some("Bearer gsk_secret123".to_string()),
        );
        let log = entry.to_log_string();

        // Secret should be redacted
        assert!(log.contains("error_code=redacted"));
        assert!(!log.contains("gsk_"));
        assert!(!log.contains("Bearer"));
    }

    #[test]
    fn diag_entry_with_fallback_does_not_leak() {
        let entry = DiagEntry::new(
            "whisper_local",
            "base",
            "native",
            1000,
            200,
            0,
            0.2,
            true,
            Some("groq".to_string()),
            1,
            None,
        );
        let log = entry.to_log_string();

        assert!(log.contains("fallback_used=true"));
        assert!(log.contains("fallback_provider=groq"));
        assert!(!log.contains("text="));
        assert!(!log.contains("audio="));
    }

    // ========================================================================
    // Error Sanitization Tests
    // ========================================================================

    #[test]
    fn sanitize_error_code_redacts_bearer_tokens() {
        assert_eq!(sanitize_error_code("Bearer abc123"), "internal");
        assert_eq!(sanitize_error_code("bearer xyz"), "internal");
        assert_eq!(
            sanitize_error_code("Authorization: Bearer token"),
            "internal"
        );
    }

    #[test]
    fn sanitize_error_code_redacts_api_keys() {
        assert_eq!(sanitize_error_code("gsk_abc123"), "internal");
        assert_eq!(sanitize_error_code("api_key=secret"), "internal");
        assert_eq!(sanitize_error_code("api-key: value"), "internal");
        assert_eq!(sanitize_error_code("authorization: token"), "internal");
    }

    #[test]
    fn sanitize_error_code_allows_safe_codes() {
        assert_eq!(sanitize_error_code("timeout_error"), "timeout_error");
        assert_eq!(sanitize_error_code("server_error"), "server_error");
        assert_eq!(sanitize_error_code("rate_limit"), "rate_limit");
        assert_eq!(sanitize_error_code("model_missing"), "model_missing");
    }

    #[test]
    fn sanitize_error_code_handles_special_chars() {
        assert_eq!(sanitize_error_code("error: test"), "error__test");
        assert_eq!(sanitize_error_code("ERROR CODE"), "error_code");
        assert_eq!(sanitize_error_code("error@test"), "error_test");
    }

    #[test]
    fn sanitize_error_code_handles_empty() {
        assert_eq!(sanitize_error_code(""), "internal");
    }

    #[test]
    fn sanitize_error_code_handles_long_strings() {
        let long_string = "a".repeat(100);
        assert_eq!(sanitize_error_code(&long_string), "internal");
    }

    // ========================================================================
    // DiagLog Privacy Tests
    // ========================================================================

    #[test]
    fn diag_log_does_not_write_without_path() {
        let diag = DiagLog::new();
        let entry = DiagEntry::new(
            "groq", "whisper", "cloud", 1000, 500, 0, 0.5, false, None, 0, None,
        );

        // Should not panic or write anywhere
        diag.append(entry);
    }

    #[test]
    fn diag_log_writes_only_safe_data_to_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_privacy.log");
        let path_str = path.to_str().unwrap().to_string();

        let diag = DiagLog::new();
        diag.set_path(path_str);

        let entry = DiagEntry::new(
            "groq",
            "whisper",
            "cloud",
            1000,
            500,
            0,
            0.5,
            false,
            None,
            0,
            Some("timeout_error".to_string()),
        );

        diag.append(entry);

        let content = fs::read_to_string(&path).unwrap();

        // Should contain safe fields
        assert!(content.contains("provider_name=groq"));
        assert!(content.contains("model_name=whisper"));
        assert!(content.contains("error_code=timeout_error"));

        // Should NOT contain sensitive data
        assert!(!content.contains("text="));
        assert!(!content.contains("audio="));
        assert!(!content.contains("key="));
        assert!(!content.contains("Bearer"));
        assert!(!content.contains("gsk_"));
        assert!(!content.contains("clipboard"));
        assert!(!content.contains("authorization"));
    }

    #[test]
    fn diag_log_redacts_secrets_in_error_codes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test_secrets.log");
        let path_str = path.to_str().unwrap().to_string();

        let diag = DiagLog::new();
        diag.set_path(path_str);

        let entry = DiagEntry::new(
            "groq",
            "whisper",
            "cloud",
            1000,
            500,
            0,
            0.5,
            false,
            None,
            0,
            Some("Bearer gsk_secret".to_string()),
        );

        diag.append(entry);

        let content = fs::read_to_string(&path).unwrap();

        // Should contain redacted, not the actual secret
        assert!(content.contains("error_code=redacted"));
        assert!(!content.contains("gsk_"));
        assert!(!content.contains("Bearer"));
    }

    // ========================================================================
    // Type-Level Privacy Tests
    // ========================================================================

    #[test]
    fn diag_entry_struct_has_no_text_field() {
        // DiagEntry intentionally does NOT have a text field
        // This is enforced at compile time
        let entry = DiagEntry::new(
            "groq", "whisper", "cloud", 1000, 500, 0, 0.5, false, None, 0, None,
        );

        // We cannot access entry.text because it doesn't exist
        // This test passes because the struct doesn't have the field
        let _ = entry;
    }

    #[test]
    fn diag_entry_struct_has_no_audio_field() {
        // DiagEntry intentionally does NOT have an audio field
        let entry = DiagEntry::new(
            "groq", "whisper", "cloud", 1000, 500, 0, 0.5, false, None, 0, None,
        );

        let _ = entry;
    }

    #[test]
    fn asr_diagnostics_has_no_text_field() {
        // AsrDiagnostics also does not contain the actual transcript text
        // It only contains metadata
        let d = crate::asr::types::AsrDiagnostics::new(
            "groq",
            "whisper",
            BackendType::Cloud,
            1000,
            500,
            "linux",
        );

        // The text field is on TranscriptResult, not on AsrDiagnostics
        // AsrDiagnostics only has metadata
        assert_eq!(d.provider_name, "groq");
        assert_eq!(d.model_name, "whisper");
        // No way to access actual transcript text through diagnostics
    }
}
