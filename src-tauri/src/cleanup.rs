use std::future::Future;

use serde::Serialize;

use crate::{providers::cerebras::CerebrasCleanupError, settings::SettingsManager};

const CLEANUP_FAILED_WARNING: &str = "Cleanup failed";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCleanupResult {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

impl TranscriptCleanupResult {
    fn from_cleaned(text: String) -> Self {
        Self {
            text,
            warning: None,
        }
    }

    fn fallback(text: String) -> Self {
        Self {
            text,
            warning: Some(CLEANUP_FAILED_WARNING.to_string()),
        }
    }
}

pub async fn cleanup_transcript(
    manager: &SettingsManager,
    transcript: String,
) -> TranscriptCleanupResult {
    cleanup_transcript_with(manager, transcript, |api_key, transcript| async move {
        crate::providers::cerebras::CerebrasCleanupClient::new()?
            .clean_transcript(&api_key, &transcript)
            .await
    })
    .await
}

pub async fn cleanup_transcript_with<F, Fut>(
    manager: &SettingsManager,
    transcript: String,
    clean_with_cerebras: F,
) -> TranscriptCleanupResult
where
    F: FnOnce(String, String) -> Fut,
    Fut: Future<Output = Result<String, CerebrasCleanupError>>,
{
    let api_key = match manager.get_cerebras_api_key_secret() {
        Ok(Some(api_key)) => api_key,
        Ok(None) => return TranscriptCleanupResult::fallback(transcript),
        Err(_) => return TranscriptCleanupResult::fallback(transcript),
    };

    match clean_with_cerebras(api_key, transcript.clone()).await {
        Ok(cleaned) => TranscriptCleanupResult::from_cleaned(cleaned),
        Err(_) => TranscriptCleanupResult::fallback(transcript),
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
        providers::cerebras::{CerebrasCleanupError, CerebrasCleanupErrorCode},
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
    async fn returns_cleaned_text_on_cerebras_success() {
        let manager = test_manager();
        manager
            .save_cerebras_api_key("csk_12345678wxyz".to_string())
            .unwrap();

        let result = cleanup_transcript_with(
            &manager,
            "raw transcript".to_string(),
            |api_key, transcript| async move {
                assert_eq!(api_key, "csk_12345678wxyz");
                assert_eq!(transcript, "raw transcript");
                Ok("Cleaned transcript.".to_string())
            },
        )
        .await;

        assert_eq!(
            result,
            super::TranscriptCleanupResult {
                text: "Cleaned transcript.".to_string(),
                warning: None,
            }
        );
    }

    #[tokio::test]
    async fn falls_back_to_raw_transcript_when_cerebras_key_is_missing() {
        let manager = test_manager();

        let result =
            cleanup_transcript_with(&manager, "raw transcript".to_string(), |_, _| async {
                panic!("cerebras must not be called without a key")
            })
            .await;

        assert_eq!(result.text, "raw transcript");
        assert_eq!(result.warning.as_deref(), Some("Cleanup failed"));
    }

    #[tokio::test]
    async fn falls_back_to_raw_transcript_when_cerebras_request_fails() {
        for code in [
            CerebrasCleanupErrorCode::Timeout,
            CerebrasCleanupErrorCode::RateLimit,
            CerebrasCleanupErrorCode::ServerError,
            CerebrasCleanupErrorCode::MalformedResponse,
            CerebrasCleanupErrorCode::ValidationFailed,
            CerebrasCleanupErrorCode::InvalidApiKey,
            CerebrasCleanupErrorCode::ApiUnreachable,
        ] {
            let manager = test_manager();
            manager
                .save_cerebras_api_key("csk_12345678wxyz".to_string())
                .unwrap();

            let result = cleanup_transcript_with(&manager, "fallback text".to_string(), |_, _| {
                let code = code.clone();
                async move {
                    Err(CerebrasCleanupError::new(
                        code,
                        "cerebras returned an error",
                    ))
                }
            })
            .await;

            assert_eq!(result.text, "fallback text");
            assert_eq!(result.warning.as_deref(), Some("Cleanup failed"));
        }
    }

    #[tokio::test]
    async fn missing_key_does_not_call_cerebras() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static CALLS: AtomicUsize = AtomicUsize::new(0);
        CALLS.store(0, Ordering::SeqCst);

        let manager = test_manager();

        let result =
            cleanup_transcript_with(&manager, "untouched text".to_string(), |_, _| async move {
                CALLS.fetch_add(1, Ordering::SeqCst);
                Ok("unused".to_string())
            })
            .await;

        assert_eq!(CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(result.text, "untouched text");
        assert_eq!(result.warning.as_deref(), Some("Cleanup failed"));
    }

    fn test_manager() -> SettingsManager {
        SettingsManager::with_secret_stores(
            Box::<MemorySecretStore>::default(),
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
