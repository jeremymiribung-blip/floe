#[derive(Debug, Clone)]
pub struct ResourcePolicy {
    pub max_concurrent_sessions: usize,
    pub session_timeout_secs: u64,
    pub gpu_memory_limit_mb: Option<u64>,
    pub allow_local_models: bool,
    pub allow_streaming: bool,
    pub max_audio_duration_secs: u64,
    pub max_audio_bytes: u64,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: 1,
            session_timeout_secs: 60,
            gpu_memory_limit_mb: None,
            allow_local_models: false,
            allow_streaming: false,
            max_audio_duration_secs: 120,
            max_audio_bytes: 25_000_000,
        }
    }
}

impl ResourcePolicy {
    pub fn with_local_models(mut self, allow: bool) -> Self {
        self.allow_local_models = allow;
        self
    }

    pub fn with_streaming(mut self, allow: bool) -> Self {
        self.allow_streaming = allow;
        self
    }

    pub fn validate_audio(&self, duration_secs: u64, bytes: u64) -> Result<(), PolicyViolation> {
        if duration_secs > self.max_audio_duration_secs {
            return Err(PolicyViolation::AudioTooLong {
                actual: duration_secs,
                max: self.max_audio_duration_secs,
            });
        }
        if bytes > self.max_audio_bytes {
            return Err(PolicyViolation::AudioTooLarge {
                actual: bytes,
                max: self.max_audio_bytes,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyViolation {
    AudioTooLong { actual: u64, max: u64 },
    AudioTooLarge { actual: u64, max: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_values() {
        let p = ResourcePolicy::default();
        assert_eq!(p.max_concurrent_sessions, 1);
        assert!(!p.allow_local_models);
        assert!(!p.allow_streaming);
    }

    #[test]
    fn builder_methods() {
        let p = ResourcePolicy::default()
            .with_local_models(true)
            .with_streaming(true);
        assert!(p.allow_local_models);
        assert!(p.allow_streaming);
    }

    #[test]
    fn validate_audio_accepts_valid() {
        let p = ResourcePolicy::default();
        assert!(p.validate_audio(30, 1_000_000).is_ok());
    }

    #[test]
    fn validate_audio_rejects_too_long() {
        let p = ResourcePolicy::default();
        let err = p.validate_audio(200, 1_000_000).unwrap_err();
        assert!(matches!(err, PolicyViolation::AudioTooLong { .. }));
    }

    #[test]
    fn validate_audio_rejects_too_large() {
        let p = ResourcePolicy::default();
        let err = p.validate_audio(30, 50_000_000).unwrap_err();
        assert!(matches!(err, PolicyViolation::AudioTooLarge { .. }));
    }
}
