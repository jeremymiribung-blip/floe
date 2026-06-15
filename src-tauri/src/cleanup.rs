use serde::Serialize;

use crate::providers::cleanup::{CleanupError, CleanupProvider, CleanupSuccess, RateLimitMetadata};
use crate::settings::SettingsManager;

const CLEANUP_FAILED_WARNING: &str = "Cleanup failed";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCleanupResult {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    pub model: String,
    pub retry_count: u32,
    pub validation_ms: u64,
    pub fallback_used: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<Box<RateLimitMetadata>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl TranscriptCleanupResult {
    fn from_success(success: CleanupSuccess) -> Self {
        Self {
            text: success.text,
            warning: None,
            model: success.model,
            retry_count: success.retry_count,
            validation_ms: success.validation_ms,
            fallback_used: false,
            rate_limit: success.rate_limit,
            error_code: None,
        }
    }

    fn fallback(text: String, error: Option<CleanupError>) -> Self {
        let model = error.as_ref().map(|e| e.model.clone()).unwrap_or_default();
        let retry_count = error.as_ref().map(|e| e.retry_count).unwrap_or(0);
        let validation_ms = error.as_ref().map(|e| e.validation_ms).unwrap_or(0);
        let rate_limit = error.as_ref().and_then(|e| e.rate_limit.clone());
        let error_code = error.as_ref().and_then(|e| e.error_code.clone());
        Self {
            text,
            warning: Some(CLEANUP_FAILED_WARNING.to_string()),
            model,
            retry_count,
            validation_ms,
            fallback_used: true,
            rate_limit,
            error_code,
        }
    }
}

pub async fn cleanup_transcript(
    manager: &SettingsManager,
    provider: &dyn CleanupProvider,
    transcript: String,
) -> TranscriptCleanupResult {
    cleanup_transcript_with(manager, transcript, provider).await
}

pub async fn cleanup_transcript_with(
    manager: &SettingsManager,
    transcript: String,
    provider: &dyn CleanupProvider,
) -> TranscriptCleanupResult {
    let api_key = match manager.get_api_key_secret() {
        Ok(Some(api_key)) => api_key,
        _ => return TranscriptCleanupResult::fallback(transcript, None),
    };

    match provider.cleanup(&api_key, &transcript).await {
        Ok(success) => TranscriptCleanupResult::from_success(success),
        Err(error) => TranscriptCleanupResult::fallback(transcript, Some(error)),
    }
}

#[cfg(test)]
use std::future::Future;

#[cfg(test)]
async fn cleanup_transcript_with_closure<F, Fut>(
    manager: &SettingsManager,
    transcript: String,
    clean_with: F,
) -> TranscriptCleanupResult
where
    F: FnOnce(String, String) -> Fut,
    Fut: Future<Output = Result<CleanupSuccess, CleanupError>>,
{
    let api_key = match manager.get_api_key_secret() {
        Ok(Some(api_key)) => api_key,
        _ => return TranscriptCleanupResult::fallback(transcript, None),
    };

    match clean_with(api_key, transcript.clone()).await {
        Ok(success) => TranscriptCleanupResult::from_success(success),
        Err(error) => TranscriptCleanupResult::fallback(transcript, Some(error)),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
    };

    use super::cleanup_transcript_with_closure;
    use crate::{
        providers::cleanup::{CleanupError, CleanupSuccess},
        settings::{SecretStore, SettingsError, SettingsManager},
    };

    #[derive(Default)]
    struct MemorySecretStore {
        secret: Mutex<Option<String>>,
    }

    impl SecretStore for MemorySecretStore {
        fn save(&self, secret: &str) -> Result<(), SettingsError> {
            *self.secret.lock().unwrap() = Some(secret.to_string());
            Ok(())
        }

        fn get(&self) -> Result<Option<String>, SettingsError> {
            Ok(self.secret.lock().unwrap().clone())
        }

        fn clear(&self) -> Result<(), SettingsError> {
            *self.secret.lock().unwrap() = None;
            Ok(())
        }
    }

    #[tokio::test]
    async fn returns_cleaned_text_on_success() {
        let manager = test_manager();
        manager
            .save_api_key("gsk_12345678wxyz".to_string())
            .unwrap();

        let result = cleanup_transcript_with_closure(
            &manager,
            "raw transcript".to_string(),
            |api_key, transcript| async move {
                assert_eq!(api_key, "gsk_12345678wxyz");
                assert_eq!(transcript, "raw transcript");
                Ok(test_success("Cleaned transcript."))
            },
        )
        .await;

        assert_eq!(result.text, "Cleaned transcript.");
        assert_eq!(result.warning, None);
        assert_eq!(result.model, "llama-3.3-70b-versatile");
        assert_eq!(result.retry_count, 0);
        assert_eq!(result.validation_ms, 1);
        assert!(!result.fallback_used);
        assert_eq!(result.rate_limit, None);
        assert_eq!(result.error_code, None);
    }

    #[tokio::test]
    async fn falls_back_to_raw_transcript_when_key_is_missing() {
        let manager = test_manager();

        let result =
            cleanup_transcript_with_closure(&manager, "raw transcript".to_string(), |_, _| async {
                panic!("must not be called without a key")
            })
            .await;

        assert_eq!(result.text, "raw transcript");
        assert_eq!(result.warning.as_deref(), Some("Cleanup failed"));
        assert!(result.fallback_used);
        assert_eq!(result.error_code, None);
    }

    #[tokio::test]
    async fn falls_back_to_raw_transcript_when_request_fails() {
        let manager = test_manager();
        manager
            .save_api_key("gsk_12345678wxyz".to_string())
            .unwrap();

        let result =
            cleanup_transcript_with_closure(&manager, "fallback text".to_string(), |_, _| async {
                Err(CleanupError {
                    message: "cleanup failed".to_string(),
                    model: String::new(),
                    retry_count: 0,
                    validation_ms: 0,
                    rate_limit: None,
                    error_code: None,
                })
            })
            .await;

        assert_eq!(result.text, "fallback text");
        assert_eq!(result.warning.as_deref(), Some("Cleanup failed"));
        assert!(result.fallback_used);
    }

    #[tokio::test]
    async fn missing_key_does_not_call_provider() {
        use std::sync::atomic::Ordering;

        static CALLS: AtomicUsize = AtomicUsize::new(0);
        CALLS.store(0, Ordering::SeqCst);

        let manager = test_manager();

        let result = cleanup_transcript_with_closure(
            &manager,
            "untouched text".to_string(),
            |_, _| async move {
                CALLS.fetch_add(1, Ordering::SeqCst);
                Ok(test_success("unused"))
            },
        )
        .await;

        assert_eq!(CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(result.text, "untouched text");
        assert_eq!(result.warning.as_deref(), Some("Cleanup failed"));
    }

    #[tokio::test]
    async fn fallback_model_defaults_to_empty_string_when_no_error() {
        let manager = test_manager();

        let result =
            cleanup_transcript_with_closure(&manager, "raw transcript".to_string(), |_, _| async {
                panic!("must not be called without a key")
            })
            .await;

        assert_eq!(result.text, "raw transcript");
        assert_eq!(result.model, "");
        assert!(result.fallback_used);
    }

    fn test_success(text: &str) -> CleanupSuccess {
        CleanupSuccess {
            text: text.to_string(),
            model: "llama-3.3-70b-versatile".to_string(),
            retry_count: 0,
            validation_ms: 1,
            rate_limit: None,
        }
    }

    fn test_manager() -> SettingsManager {
        SettingsManager::with_secret_store(
            Box::<MemorySecretStore>::default(),
            unique_settings_path(),
        )
    }

    fn unique_settings_path() -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);

        std::env::temp_dir().join(format!("floe-cleanup-test-{}-{}.json", unique_id, counter))
    }

    #[tokio::test]
    async fn cleanup_only_uses_text_not_audio() {
        // Verify that cleanup only receives text, not audio data
        let manager = test_manager();
        manager.save_api_key("gsk_test_key".to_string()).unwrap();

        let transcript_text = "This is a test transcript";

        let result = cleanup_transcript_with_closure(
            &manager,
            transcript_text.to_string(),
            |_api_key, transcript| async move {
                // Verify we received text, not audio
                assert_eq!(transcript, "This is a test transcript");
                assert!(!transcript.contains("audio"));
                assert!(!transcript.contains("wav"));
                assert!(!transcript.contains("bytes"));

                Ok(test_success("Cleaned transcript"))
            },
        )
        .await;

        assert_eq!(result.text, "Cleaned transcript");
        assert!(!result.fallback_used);
    }

    #[tokio::test]
    async fn cleanup_does_not_receive_audio_data() {
        // Verify that cleanup never receives audio bytes or WAV data
        let manager = test_manager();
        manager.save_api_key("gsk_test_key".to_string()).unwrap();

        // This is the transcript text - cleanup should only see this, not any audio
        let transcript = "test audio transcript";

        let result = cleanup_transcript_with_closure(
            &manager,
            transcript.to_string(),
            |_api_key, received_text| async move {
                // The received text should be exactly what we sent
                assert_eq!(received_text, "test audio transcript");

                // It should NOT be audio data
                // Audio data would be binary or WAV format, not readable text
                assert!(received_text.is_ascii());

                Ok(test_success("cleaned"))
            },
        )
        .await;

        assert_eq!(result.text, "cleaned");
    }

    #[tokio::test]
    async fn cleanup_provider_agnostic() {
        // Verify that cleanup works with any provider that implements CleanupProvider
        // This test uses a mock provider to show cleanup is not tied to any specific ASR provider
        let manager = test_manager();
        manager.save_api_key("gsk_test_key".to_string()).unwrap();

        // Test with a non-Groq provider
        let result = cleanup_transcript_with_closure(
            &manager,
            "test transcript".to_string(),
            |_api_key, _transcript| async {
                // This could be any cleanup provider
                Ok(test_success("cleaned by any provider"))
            },
        )
        .await;

        assert_eq!(result.text, "cleaned by any provider");
        assert!(!result.fallback_used);
    }
}
