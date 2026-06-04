use std::future::Future;

use serde::Serialize;

use crate::{
    providers::groq::{GroqCleanup, GroqCleanupError, GroqCleanupErrorCode},
    settings::SettingsManager,
};

const CLEANUP_FAILED_WARNING: &str = "Cleanup failed";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
    pub error_code: Option<GroqCleanupErrorCode>,
}

impl TranscriptCleanupResult {
    fn from_cleaned(cleaned: GroqCleanup) -> Self {
        Self {
            text: cleaned.text,
            warning: None,
            model: cleaned.model,
            retry_count: cleaned.retry_count,
            validation_ms: cleaned.validation_ms,
            fallback_used: false,
            error_code: None,
        }
    }

    fn fallback(text: String, error: Option<GroqCleanupError>) -> Self {
        let model = error
            .as_ref()
            .map(|error| error.model.clone())
            .unwrap_or_else(|| "openai/gpt-oss-20b".to_string());
        let retry_count = error.as_ref().map(|error| error.retry_count).unwrap_or(0);
        let validation_ms = error.as_ref().map(|error| error.validation_ms).unwrap_or(0);
        let error_code = error.map(|error| error.code);

        Self {
            text,
            warning: Some(CLEANUP_FAILED_WARNING.to_string()),
            model,
            retry_count,
            validation_ms,
            fallback_used: true,
            error_code,
        }
    }
}

pub async fn cleanup_transcript(
    manager: &SettingsManager,
    transcript: String,
) -> TranscriptCleanupResult {
    cleanup_transcript_with(manager, transcript, |api_key, transcript| async move {
        crate::providers::groq::GroqCleanupClient::new()?
            .cleanup_transcript(&api_key, &transcript)
            .await
    })
    .await
}

pub async fn cleanup_transcript_with<F, Fut>(
    manager: &SettingsManager,
    transcript: String,
    clean_with_groq: F,
) -> TranscriptCleanupResult
where
    F: FnOnce(String, String) -> Fut,
    Fut: Future<Output = Result<GroqCleanup, GroqCleanupError>>,
{
    let api_key = match manager.get_groq_api_key_secret() {
        Ok(Some(api_key)) => api_key,
        Ok(None) => return TranscriptCleanupResult::fallback(transcript, None),
        Err(_) => return TranscriptCleanupResult::fallback(transcript, None),
    };

    match clean_with_groq(api_key, transcript.clone()).await {
        Ok(cleaned) => TranscriptCleanupResult::from_cleaned(cleaned),
        Err(error) => TranscriptCleanupResult::fallback(transcript, Some(error)),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::{atomic::AtomicUsize, atomic::Ordering, Mutex},
    };

    use super::cleanup_transcript_with;
    use crate::{
        providers::groq::{GroqCleanup, GroqCleanupError, GroqCleanupErrorCode},
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
    async fn returns_cleaned_text_on_groq_success() {
        let manager = test_manager();
        manager
            .save_groq_api_key("gsk_12345678wxyz".to_string())
            .unwrap();

        let result = cleanup_transcript_with(
            &manager,
            "raw transcript".to_string(),
            |api_key, transcript| async move {
                assert_eq!(api_key, "gsk_12345678wxyz");
                assert_eq!(transcript, "raw transcript");
                Ok(test_cleanup("Cleaned transcript."))
            },
        )
        .await;

        assert_eq!(
            result,
            super::TranscriptCleanupResult {
                text: "Cleaned transcript.".to_string(),
                warning: None,
                model: "openai/gpt-oss-20b".to_string(),
                retry_count: 0,
                validation_ms: 1,
                fallback_used: false,
                error_code: None,
            }
        );
    }

    #[tokio::test]
    async fn falls_back_to_raw_transcript_when_groq_key_is_missing() {
        let manager = test_manager();

        let result =
            cleanup_transcript_with(&manager, "raw transcript".to_string(), |_, _| async {
                panic!("groq must not be called without a key")
            })
            .await;

        assert_eq!(result.text, "raw transcript");
        assert_eq!(result.warning.as_deref(), Some("Cleanup failed"));
        assert!(result.fallback_used);
        assert_eq!(result.error_code, None);
    }

    #[tokio::test]
    async fn falls_back_to_raw_transcript_when_groq_request_fails() {
        for code in [
            GroqCleanupErrorCode::Timeout,
            GroqCleanupErrorCode::RateLimit,
            GroqCleanupErrorCode::ServerError,
            GroqCleanupErrorCode::MalformedResponse,
            GroqCleanupErrorCode::ValidationFailed,
            GroqCleanupErrorCode::InvalidApiKey,
            GroqCleanupErrorCode::ApiUnreachable,
        ] {
            let manager = test_manager();
            manager
                .save_groq_api_key("gsk_12345678wxyz".to_string())
                .unwrap();

            let result = cleanup_transcript_with(&manager, "fallback text".to_string(), |_, _| {
                let code = code.clone();
                async move { Err(GroqCleanupError::new(code, "groq returned an error")) }
            })
            .await;

            assert_eq!(result.text, "fallback text");
            assert_eq!(result.warning.as_deref(), Some("Cleanup failed"));
            assert!(result.fallback_used);
            assert_eq!(result.error_code, Some(code));
        }
    }

    #[tokio::test]
    async fn missing_key_does_not_call_groq() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static CALLS: AtomicUsize = AtomicUsize::new(0);
        CALLS.store(0, Ordering::SeqCst);

        let manager = test_manager();

        let result =
            cleanup_transcript_with(&manager, "untouched text".to_string(), |_, _| async move {
                CALLS.fetch_add(1, Ordering::SeqCst);
                Ok(test_cleanup("unused"))
            })
            .await;

        assert_eq!(CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(result.text, "untouched text");
        assert_eq!(result.warning.as_deref(), Some("Cleanup failed"));
    }

    fn test_cleanup(text: &str) -> GroqCleanup {
        GroqCleanup {
            text: text.to_string(),
            model: "openai/gpt-oss-20b".to_string(),
            retry_count: 0,
            validation_ms: 1,
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
}
