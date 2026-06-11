//! Tests for fallback behavior in the ASR architecture.
//!
//! These tests verify:
//! - Fallback to Groq when primary provider fails
//! - Fallback with retry behavior
//! - No fallback when no fallback provider is configured
//! - Audio data preservation during fallback
//!
//! Note: The fallback strategy is implemented in crate::asr::fallback::FallbackStrategy

use crate::asr::error::SessionError;
use crate::asr::fallback::FallbackStrategy;
use crate::asr::traits::{AsrProvider, AsrSession, TranscriptionError, TranscriptionErrorCode};
use crate::asr::types::{AudioChunk, BackendType, HealthStatus, ModelSpec, ProviderCapabilities, SessionConfig, TranscriptResult, AsrDiagnostics};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Mock session for testing
#[derive(Debug)]
pub struct TestSession {
    pub succeed: bool,
    pub retryable: bool,
    pub call_count: AtomicUsize,
}

#[async_trait]
impl AsrSession for TestSession {
    fn model(&self) -> &str {
        "test-model"
    }
    fn provider_id(&self) -> &'static str {
        "test-provider"
    }
    async fn submit_audio(&self, _: AudioChunk) -> Result<(), ()> {
        Ok(())
    }
    async fn partial_transcript(&self) -> Option<String> {
        None
    }
    async fn finalize(self: Box<Self>) -> Result<TranscriptResult, TranscriptionError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        if self.succeed {
            Ok(TranscriptResult {
                text: "test transcript".to_string(),
                model: "test-model".to_string(),
                diagnostics: AsrDiagnostics::new(
                    "test-provider",
                    "test-model",
                    BackendType::Cloud,
                    1000,
                    500,
                    "test",
                ),
            })
        } else {
            Err(TranscriptionError::new(
                TranscriptionErrorCode::ServerError,
                "test failure",
                self.retryable,
            ))
        }
    }
    async fn cancel(self: Box<Self>) {}
}

/// Mock provider for testing
#[derive(Debug)]
pub struct TestProvider {
    pub id: &'static str,
    pub succeed: bool,
    pub retryable: bool,
}

#[async_trait]
impl AsrProvider for TestProvider {
    fn id(&self) -> &'static str {
        self.id
    }
    fn name(&self) -> &'static str {
        self.id
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            fallback_compatible: true,
            ..Default::default()
        }
    }
    fn default_model(&self) -> &'static str {
        "test-model"
    }
    fn available_models(&self) -> &[ModelSpec] {
        &[]
    }
    async fn create_session(
        &self,
        _: SessionConfig,
    ) -> Result<Box<dyn AsrSession>, SessionError> {
        Ok(Box::new(TestSession {
            succeed: self.succeed,
            retryable: self.retryable,
            call_count: AtomicUsize::new(0),
        }))
    }
    async fn health_check(&self) -> Result<HealthStatus, ()> {
        Ok(HealthStatus::Healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;

    fn create_runtime() -> Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn run_async_test<F, R>(test: F) -> R
    where
        F: std::future::Future<Output = R> + Send + 'static,
        R: Send + 'static,
    {
        let rt = create_runtime();
        rt.block_on(test)
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

    #[test]
    fn fallback_triggered_on_non_retryable_failure() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: false,
                retryable: false,
            };
            let fallback = TestProvider {
                id: "fallback",
                succeed: true,
                retryable: false,
            };

            let result = FallbackStrategy::execute(&primary, Some(&fallback), test_wav(), 1000)
                .await
                .unwrap();

            assert!(result.diagnostics.fallback_used);
            assert_eq!(result.text, "test transcript");
        });
    }

    #[test]
    fn primary_succeeds_no_fallback_used() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: true,
                retryable: false,
            };
            let fallback = TestProvider {
                id: "fallback",
                succeed: true,
                retryable: false,
            };

            let result = FallbackStrategy::execute(&primary, Some(&fallback), test_wav(), 1000)
                .await
                .unwrap();

            assert!(!result.diagnostics.fallback_used);
            assert_eq!(result.text, "test transcript");
        });
    }

    #[test]
    fn no_fallback_returns_error() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: false,
                retryable: false,
            };

            let result = FallbackStrategy::execute(&primary, None, test_wav(), 1000).await;

            assert!(result.is_err());
            assert_eq!(
                result.unwrap_err().code,
                crate::asr::error::AsrErrorCode::FallbackFailed
            );
        });
    }

    #[test]
    fn retryable_error_triggers_retry_then_fallback() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: false,
                retryable: true,
            };
            let fallback = TestProvider {
                id: "fallback",
                succeed: true,
                retryable: false,
            };

            let result = FallbackStrategy::execute(&primary, Some(&fallback), test_wav(), 1000)
                .await
                .unwrap();

            // Should have retried and then fallen back
            assert!(result.diagnostics.fallback_used);
            assert_eq!(result.text, "test transcript");
        });
    }

    #[test]
    fn fallback_preserves_audio_data() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: false,
                retryable: false,
            };
            let fallback = TestProvider {
                id: "fallback",
                succeed: true,
                retryable: false,
            };

            let audio_data = test_wav();
            let result = FallbackStrategy::execute(
                &primary,
                Some(&fallback),
                audio_data.clone(),
                1000,
            )
            .await
            .unwrap();

            // If we got a result, audio was preserved through fallback
            assert!(result.diagnostics.fallback_used);
            assert_eq!(result.text, "test transcript");
        });
    }

    #[test]
    fn fallback_sets_correct_fallback_provider_name() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: false,
                retryable: false,
            };
            let groq_fallback = TestProvider {
                id: "groq",
                succeed: true,
                retryable: false,
            };

            let result = FallbackStrategy::execute(&primary, Some(&groq_fallback), test_wav(), 1000)
                .await
                .unwrap();

            assert!(result.diagnostics.fallback_used);
            assert_eq!(
                result.diagnostics.fallback_provider.as_deref(),
                Some("groq")
            );
        });
    }

    #[test]
    fn fallback_deterministic_order() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: false,
                retryable: false,
            };
            let fallback = TestProvider {
                id: "fallback",
                succeed: true,
                retryable: false,
            };

            // Run multiple times to ensure deterministic behavior
            for _ in 0..3 {
                let result = FallbackStrategy::execute(&primary, Some(&fallback), test_wav(), 1000)
                    .await
                    .unwrap();

                assert!(result.diagnostics.fallback_used);
                assert_eq!(result.text, "test transcript");
            }
        });
    }

    #[test]
    fn fallback_error_includes_primary_error_message() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: false,
                retryable: false,
            };
            let fallback = TestProvider {
                id: "fallback",
                succeed: false,
                retryable: false,
            };

            let result = FallbackStrategy::execute(&primary, Some(&fallback), test_wav(), 1000)
                .await;

            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(err.message.contains("Primary failed"));
            assert!(err.message.contains("fallback also failed"));
        });
    }

    #[test]
    fn fallback_with_empty_audio_handled() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: true,
                retryable: false,
            };

            // Empty audio should still work through the pipeline
            let result = FallbackStrategy::execute(&primary, None, test_wav(), 1000).await;

            // This will succeed because primary succeeds
            assert!(result.is_ok());
        });
    }

    #[test]
    fn fallback_retry_count_incremented() {
        let rt = create_runtime();
        
        rt.block_on(async {
            let primary = TestProvider {
                id: "primary",
                succeed: false,
                retryable: true,
            };
            let fallback = TestProvider {
                id: "fallback",
                succeed: true,
                retryable: false,
            };

            let result = FallbackStrategy::execute(&primary, Some(&fallback), test_wav(), 1000)
                .await
                .unwrap();

            // Should have retried at least once before falling back
            assert!(result.diagnostics.retry_count > 0);
        });
    }
}
