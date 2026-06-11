//! Tests for resource policy enforcement in the ASR architecture.
//!
//! These tests verify:
//! - Audio validation (duration and size limits)
//! - Policy configuration
//! - Resource limits enforcement

#[cfg(test)]
mod tests {
    use crate::asr::policy::{PolicyViolation, ResourcePolicy};
    use std::time::Duration;

    #[test]
    fn default_policy_values() {
        let policy = ResourcePolicy::default();
        
        assert_eq!(policy.max_concurrent_sessions, 1);
        assert_eq!(policy.session_timeout_secs, 60);
        assert!(policy.gpu_memory_limit_mb.is_none());
        assert!(!policy.allow_local_models);
        assert!(!policy.allow_streaming);
        assert_eq!(policy.max_audio_duration_secs, 120);
        assert_eq!(policy.max_audio_bytes, 25_000_000);
    }

    #[test]
    fn policy_allows_local_models() {
        let policy = ResourcePolicy::default().with_local_models(true);
        assert!(policy.allow_local_models);
    }

    #[test]
    fn policy_allows_streaming() {
        let policy = ResourcePolicy::default().with_streaming(true);
        assert!(policy.allow_streaming);
    }

    #[test]
    fn policy_both_flags() {
        let policy = ResourcePolicy::default()
            .with_local_models(true)
            .with_streaming(true);
        assert!(policy.allow_local_models);
        assert!(policy.allow_streaming);
    }

    #[test]
    fn validate_audio_accepts_valid_audio() {
        let policy = ResourcePolicy::default();
        
        // Valid: 30 seconds, 1MB
        assert!(policy.validate_audio(30, 1_000_000).is_ok());
        
        // Valid: exactly at limit
        assert!(policy.validate_audio(120, 25_000_000).is_ok());
    }

    #[test]
    fn validate_audio_rejects_too_long() {
        let policy = ResourcePolicy::default();
        
        let result = policy.validate_audio(200, 1_000_000);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PolicyViolation::AudioTooLong { .. }));
    }

    #[test]
    fn validate_audio_rejects_too_large() {
        let policy = ResourcePolicy::default();
        
        let result = policy.validate_audio(30, 50_000_000);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PolicyViolation::AudioTooLarge { .. }));
    }

    #[test]
    fn validate_audio_rejects_both_violations() {
        let policy = ResourcePolicy::default();
        
        // Too long AND too large
        let result = policy.validate_audio(200, 50_000_000);
        assert!(result.is_err());
        
        // Should report the first violation (AudioTooLong)
        assert!(matches!(result.unwrap_err(), PolicyViolation::AudioTooLong { .. }));
    }

    #[test]
    fn validate_audio_edge_cases() {
        let policy = ResourcePolicy::default();
        
        // Zero duration - should be valid (min is 1 second internally)
        assert!(policy.validate_audio(0, 1).is_ok());
        
        // Exactly at limits
        assert!(policy.validate_audio(120, 25_000_000).is_ok());
        
        // Just over limits
        assert!(policy.validate_audio(121, 25_000_000).is_err());
        assert!(policy.validate_audio(120, 25_000_001).is_err());
    }

    #[test]
    fn policy_with_custom_limits() {
        let policy = ResourcePolicy {
            max_concurrent_sessions: 8,
            session_timeout_secs: 90,
            gpu_memory_limit_mb: Some(4096),
            allow_local_models: true,
            allow_streaming: true,
            max_audio_duration_secs: 300,
            max_audio_bytes: 100_000_000,
        };
        
        assert_eq!(policy.max_concurrent_sessions, 8);
        assert_eq!(policy.session_timeout_secs, 90);
        assert_eq!(policy.gpu_memory_limit_mb, Some(4096));
        assert!(policy.allow_local_models);
        assert!(policy.allow_streaming);
        assert_eq!(policy.max_audio_duration_secs, 300);
        assert_eq!(policy.max_audio_bytes, 100_000_000);
    }

    #[test]
    fn policy_violation_display() {
        let violation = PolicyViolation::AudioTooLong {
            actual: 200,
            max: 120,
        };
        let display = format!("{:?}", violation);
        assert!(display.contains("AudioTooLong"));
    }

    #[test]
    fn policy_enforces_strict_limits() {
        let policy = ResourcePolicy {
            max_audio_duration_secs: 60,
            max_audio_bytes: 10_000_000,
            ..Default::default()
        };
        
        // 61 seconds should fail
        assert!(policy.validate_audio(61, 1).is_err());
        
        // 10,000,001 bytes should fail
        assert!(policy.validate_audio(1, 10_000_001).is_err());
        
        // 60 seconds and 10MB should pass
        assert!(policy.validate_audio(60, 10_000_000).is_ok());
    }
}
