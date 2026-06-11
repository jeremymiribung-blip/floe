use async_trait::async_trait;

use super::error::SessionError;
use super::types::{
    AudioChunk, HealthStatus, ModelSpec, ProviderCapabilities, SessionConfig, TranscriptResult,
};

#[async_trait]
pub trait AsrProvider: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> ProviderCapabilities;
    fn default_model(&self) -> &'static str;
    fn available_models(&self) -> &[ModelSpec];

    async fn create_session(
        &self,
        config: SessionConfig,
    ) -> Result<Box<dyn AsrSession>, SessionError>;

    async fn health_check(&self) -> Result<HealthStatus, ()>;
}

#[async_trait]
pub trait AsrSession: Send + Sync + std::fmt::Debug {
    fn model(&self) -> &str;
    fn provider_id(&self) -> &'static str;

    async fn submit_audio(&self, chunk: AudioChunk) -> Result<(), ()>;

    async fn partial_transcript(&self) -> Option<String>;

    async fn finalize(self: Box<Self>) -> Result<TranscriptResult, TranscriptionError>;

    async fn cancel(self: Box<Self>);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionError {
    pub code: TranscriptionErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptionErrorCode {
    Timeout,
    ApiUnreachable,
    RateLimit,
    InvalidAuth,
    InvalidRequest,
    UnsupportedAudio,
    MalformedResponse,
    ServerError,
    Internal,
}

impl TranscriptionError {
    pub fn new(code: TranscriptionErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestProvider;

    #[async_trait]
    impl AsrProvider for TestProvider {
        fn id(&self) -> &'static str {
            "test"
        }
        fn name(&self) -> &'static str {
            "Test Provider"
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities::default()
        }
        fn default_model(&self) -> &'static str {
            "test-model"
        }
        fn available_models(&self) -> &[ModelSpec] {
            &[]
        }
        async fn create_session(
            &self,
            _config: SessionConfig,
        ) -> Result<Box<dyn AsrSession>, SessionError> {
            Err(SessionError::new(
                super::super::error::SessionErrorCode::Internal,
                "not implemented",
            ))
        }
        async fn health_check(&self) -> Result<HealthStatus, ()> {
            Ok(HealthStatus::Healthy)
        }
    }

    #[tokio::test]
    async fn provider_trait_basics() {
        let p = TestProvider;
        assert_eq!(p.id(), "test");
        assert_eq!(p.name(), "Test Provider");
        assert!(p.health_check().await.is_ok());
    }

    #[test]
    fn transcription_error_retryable_flag() {
        let err = TranscriptionError::new(TranscriptionErrorCode::RateLimit, "rate limited", true);
        assert!(err.retryable);
        let err = TranscriptionError::new(TranscriptionErrorCode::InvalidAuth, "bad key", false);
        assert!(!err.retryable);
    }
}
