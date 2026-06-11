use std::sync::Arc;

use super::error::AsrError;
use super::fallback::FallbackStrategy;
use super::policy::ResourcePolicy;
use super::registry::ProviderRegistry;
use super::state::RuntimeState;
use super::traits::AsrSession;
use super::types::{SessionConfig, TranscriptResult};

pub struct AsrBackend {
    pub registry: Arc<ProviderRegistry>,
    pub policy: ResourcePolicy,
    pub runtime: RuntimeState,
}

impl AsrBackend {
    pub fn new(
        registry: Arc<ProviderRegistry>,
        policy: ResourcePolicy,
        runtime: RuntimeState,
    ) -> Self {
        Self {
            registry,
            policy,
            runtime,
        }
    }

    pub async fn transcribe(
        &self,
        audio: Vec<u8>,
        audio_duration_ms: u64,
        preferred_provider: Option<&str>,
    ) -> Result<TranscriptResult, AsrError> {
        let audio_secs = (audio_duration_ms / 1000).max(1);
        self.policy
            .validate_audio(audio_secs, audio.len() as u64)
            .map_err(|violation| {
                AsrError::new(
                    match violation {
                        super::policy::PolicyViolation::AudioTooLong { .. } => {
                            super::error::AsrErrorCode::AudioTooLong
                        }
                        super::policy::PolicyViolation::AudioTooLarge { .. } => {
                            super::error::AsrErrorCode::AudioTooLarge
                        }
                    },
                    format!("Audio validation failed: {:?}", violation),
                )
            })?;

        let criteria = super::types::SelectionCriteria {
            preferred: preferred_provider.map(|s| s.to_string()),
            audio_duration_ms,
            requires_fallback_compatible: false,
            requires_local: false,
            requires_streaming: false,
        };

        let primary = self.registry.select(criteria).map_err(|sel_err| {
            AsrError::new(
                super::error::AsrErrorCode::NoProvider,
                format!("Provider selection failed: {}", sel_err.message),
            )
        })?;

        // Enforce local model policy
        let primary_caps = primary.capabilities();
        if matches!(
            primary_caps.deployment,
            super::types::Deployment::Local
        ) && !self.policy.allow_local_models
        {
            return Err(AsrError::new(
                super::error::AsrErrorCode::ProviderRejected,
                "local models are not enabled in the resource policy",
            ));
        }

        let fallback = self.registry.fallback_provider_excluding(Some(primary.id()));

        FallbackStrategy::execute(primary, fallback, audio, audio_duration_ms).await
    }

    pub async fn start_session(
        &self,
        provider_id: &str,
        model_id: Option<&str>,
    ) -> Result<Box<dyn AsrSession>, AsrError> {
        let provider = self.registry.get(provider_id).ok_or_else(|| {
            AsrError::new(
                super::error::AsrErrorCode::NoProvider,
                format!("Provider '{}' not found.", provider_id),
            )
        })?;

        let model = model_id.unwrap_or(provider.default_model());

        let config = SessionConfig {
            model: Some(model.to_string()),
            sample_rate: 16_000,
            max_duration_secs: self.policy.max_audio_duration_secs,
        };

        provider
            .create_session(config)
            .await
            .map_err(|session_err| {
                AsrError::new(
                    super::error::AsrErrorCode::TranscriptionFailed,
                    format!("Session creation failed: {}", session_err.message),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::error::*;
    use crate::asr::traits::AsrProvider;
    use crate::asr::error::SessionError;
    use crate::asr::traits::{TranscriptionError, TranscriptionErrorCode};
    use crate::asr::types::*;
    use async_trait::async_trait;

    #[derive(Debug)]
    struct OkSession;

    #[async_trait]
    impl AsrSession for OkSession {
        fn model(&self) -> &str {
            "model"
        }
        fn provider_id(&self) -> &'static str {
            "ok"
        }
        async fn submit_audio(&self, _: AudioChunk) -> Result<(), ()> {
            Ok(())
        }
        async fn partial_transcript(&self) -> Option<String> {
            None
        }
        async fn finalize(self: Box<Self>) -> Result<TranscriptResult, TranscriptionError> {
            Ok(TranscriptResult {
                text: "hello".into(),
                model: "model".into(),
                diagnostics: AsrDiagnostics::new(
                    "ok",
                    "model",
                    BackendType::Cloud,
                    1000,
                    500,
                    "test",
                ),
            })
        }
        async fn cancel(self: Box<Self>) {}
    }

    #[derive(Debug)]
    struct OkProvider;

    #[async_trait]
    impl AsrProvider for OkProvider {
        fn id(&self) -> &'static str {
            "ok"
        }
        fn name(&self) -> &'static str {
            "OK Provider"
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                fallback_compatible: true,
                ..Default::default()
            }
        }
        fn default_model(&self) -> &'static str {
            "model"
        }
        fn available_models(&self) -> &[ModelSpec] {
            &[]
        }
        async fn create_session(
            &self,
            _: SessionConfig,
        ) -> Result<Box<dyn AsrSession>, SessionError> {
            Ok(Box::new(OkSession))
        }
        async fn health_check(&self) -> Result<HealthStatus, ()> {
            Ok(HealthStatus::Healthy)
        }
    }

    #[derive(Debug)]
    struct FailSession;

    #[async_trait]
    impl AsrSession for FailSession {
        fn model(&self) -> &str {
            "model"
        }
        fn provider_id(&self) -> &'static str {
            "fail"
        }
        async fn submit_audio(&self, _: AudioChunk) -> Result<(), ()> {
            Err(())
        }
        async fn partial_transcript(&self) -> Option<String> {
            None
        }
        async fn finalize(self: Box<Self>) -> Result<TranscriptResult, TranscriptionError> {
            Err(TranscriptionError::new(
                TranscriptionErrorCode::ServerError,
                "failed",
                false,
            ))
        }
        async fn cancel(self: Box<Self>) {}
    }

    #[derive(Debug)]
    struct FailProvider;

    #[async_trait]
    impl AsrProvider for FailProvider {
        fn id(&self) -> &'static str {
            "fail"
        }
        fn name(&self) -> &'static str {
            "Fail Provider"
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                fallback_compatible: false,
                ..Default::default()
            }
        }
        fn default_model(&self) -> &'static str {
            "model"
        }
        fn available_models(&self) -> &[ModelSpec] {
            &[]
        }
        async fn create_session(
            &self,
            _: SessionConfig,
        ) -> Result<Box<dyn AsrSession>, SessionError> {
            Ok(Box::new(FailSession))
        }
        async fn health_check(&self) -> Result<HealthStatus, ()> {
            Ok(HealthStatus::Healthy)
        }
    }

    fn test_wav() -> Vec<u8> {
        let mut wav = Vec::with_capacity(48);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&40u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&32000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&4u32.to_le_bytes());
        wav.extend_from_slice(&0i16.to_le_bytes());
        wav.extend_from_slice(&0i16.to_le_bytes());
        wav
    }

    fn test_backend() -> AsrBackend {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(OkProvider)).unwrap();
        AsrBackend::new(
            Arc::new(registry),
            ResourcePolicy::default(),
            RuntimeState::new("ok".into()),
        )
    }

    fn test_backend_with_fallback() -> AsrBackend {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(FailProvider)).unwrap();
        registry.register(Box::new(OkProvider)).unwrap();
        AsrBackend::new(
            Arc::new(registry),
            ResourcePolicy::default(),
            RuntimeState::new("fail".into()),
        )
    }

    #[tokio::test]
    async fn backend_rejects_audio_too_long() {
        let backend = test_backend();
        let err = backend.transcribe(vec![], 200_000, None).await.unwrap_err();
        assert_eq!(err.code, AsrErrorCode::AudioTooLong);
    }

    #[tokio::test]
    async fn backend_rejects_audio_too_large() {
        let backend = test_backend();
        let err = backend
            .transcribe(vec![0u8; 30_000_000], 1000, None)
            .await
            .unwrap_err();
        assert_eq!(err.code, AsrErrorCode::AudioTooLarge);
    }

    #[tokio::test]
    async fn backend_returns_transcription_result() {
        let backend = test_backend();
        let result = backend.transcribe(test_wav(), 1000, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().text, "hello");
    }

    #[tokio::test]
    async fn backend_fallback_to_ok_provider() {
        let backend = test_backend_with_fallback();
        let result = backend
            .transcribe(test_wav(), 1000, Some("fail"))
            .await;
        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.diagnostics.fallback_used);
        assert_eq!(r.text, "hello");
    }

    #[tokio::test]
    async fn backend_start_session_unknown_provider() {
        let backend = test_backend();
        let err = backend
            .start_session("nonexistent", None)
            .await
            .unwrap_err();
        assert_eq!(err.code, AsrErrorCode::NoProvider);
    }
}
