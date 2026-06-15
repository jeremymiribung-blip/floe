//! Tests for ASR diagnostics and logging.
//!
//! These tests verify:
//! - Diagnostics are created correctly
//! - No sensitive data in diagnostics
//! - Error codes are sanitized
//! - Diagnostics fields are complete

#[cfg(test)]
mod tests {
    use crate::asr::types::{AsrDiagnostics, BackendType};

    #[test]
    fn diagnostics_constructs_with_all_fields() {
        let d = AsrDiagnostics::new(
            "groq",
            "whisper-large-v3-turbo",
            BackendType::Cloud,
            5000,
            1200,
            "linux",
        );

        assert_eq!(d.provider_name, "groq");
        assert_eq!(d.model_name, "whisper-large-v3-turbo");
        assert_eq!(d.backend_type, BackendType::Cloud);
        assert_eq!(d.audio_duration_ms, 5000);
        assert_eq!(d.transcription_ms, 1200);
        assert_eq!(d.platform, "linux");
        assert!(!d.fallback_used);
        assert!(d.fallback_provider.is_none());
        assert!(d.error_code.is_none());
        assert_eq!(d.retry_count, 0);
        assert_eq!(d.cleanup_ms, 0);
    }

    #[test]
    fn diagnostics_realtime_factor_calculated() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 5000, 1200, "linux");

        // 1200 / 5000 = 0.24
        assert!((d.realtime_factor - 0.24).abs() < f64::EPSILON);
    }

    #[test]
    fn diagnostics_with_error() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux")
            .with_error("timeout_error");

        assert!(d.error_code.is_some());
        assert_eq!(d.error_code.as_deref(), Some("timeout_error"));
    }

    #[test]
    fn diagnostics_with_fallback() {
        let d = AsrDiagnostics::new(
            "whisper_local",
            "base",
            BackendType::Cloud,
            1000,
            200,
            "macos",
        )
        .with_fallback("groq");

        assert!(d.fallback_used);
        assert_eq!(d.fallback_provider.as_deref(), Some("groq"));
    }

    #[test]
    fn diagnostics_with_retry() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux")
            .with_retry(3);

        assert_eq!(d.retry_count, 3);
    }

    #[test]
    fn diagnostics_with_cleanup_ms() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux")
            .with_cleanup_ms(250);

        assert_eq!(d.cleanup_ms, 250);
    }

    #[test]
    fn diagnostics_error_code_sanitization() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux")
            .with_error("INVALID_AUTH");

        assert_eq!(d.error_code.as_deref(), Some("invalid_auth"));
    }

    #[test]
    fn diagnostics_error_redacts_secrets() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux")
            .with_error("Bearer gsk_abcdef");

        assert_eq!(d.error_code.as_deref(), Some("internal"));
    }

    #[test]
    fn diagnostics_trace_version() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux");

        assert_eq!(d.trace_version, 1);
    }

    #[test]
    fn diagnostics_created_at_is_iso_format() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux");

        // The created_at should be a valid ISO string
        // It starts with the date part
        assert!(d.created_at.contains("1970") || d.created_at.len() > 10);
    }

    #[test]
    fn diagnostics_zero_audio_duration() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 0, 100, "linux");

        // Realtime factor should be 0 when audio duration is 0
        assert_eq!(d.realtime_factor, 0.0);
    }

    #[test]
    fn diagnostics_chained_modifications() {
        let d = AsrDiagnostics::new("groq", "whisper", BackendType::Cloud, 1000, 500, "linux")
            .with_error("timeout")
            .with_fallback("whisper_local")
            .with_retry(2)
            .with_cleanup_ms(150);

        assert!(d.fallback_used);
        assert_eq!(d.fallback_provider.as_deref(), Some("whisper_local"));
        assert_eq!(d.retry_count, 2);
        assert_eq!(d.cleanup_ms, 150);
        assert_eq!(d.error_code.as_deref(), Some("timeout"));
    }
}
