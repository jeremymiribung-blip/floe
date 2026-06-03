use std::future::Future;

use serde::Serialize;

use crate::{
    providers::cerebras::{CerebrasCleanupClient, CerebrasCleanupError},
    settings::{CleanupMode, SettingsError, SettingsErrorCode, SettingsManager},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCleanupResult {
    pub text: String,
    pub mode: CleanupMode,
    pub warning: Option<String>,
}

pub async fn cleanup_transcript(
    manager: &SettingsManager,
    transcript: String,
) -> TranscriptCleanupResult {
    let client = match CerebrasCleanupClient::new() {
        Ok(client) => client,
        Err(error) => return fallback_to_fast(&transcript, Some(error.message)),
    };

    cleanup_transcript_with_provider(manager, transcript, |api_key, transcript| async move {
        client.clean_transcript(&api_key, &transcript).await
    })
    .await
}

pub async fn cleanup_transcript_with_provider<F, Fut>(
    manager: &SettingsManager,
    transcript: String,
    clean_with_cerebras: F,
) -> TranscriptCleanupResult
where
    F: FnOnce(String, String) -> Fut,
    Fut: Future<Output = Result<String, CerebrasCleanupError>>,
{
    let mode = match manager.get_cleanup_mode() {
        Ok(mode) => mode,
        Err(error) => return fallback_to_fast(&transcript, Some(error.message)),
    };

    match mode {
        CleanupMode::Raw => TranscriptCleanupResult {
            text: transcript,
            mode: CleanupMode::Raw,
            warning: None,
        },
        CleanupMode::Fast => fast_result(&transcript, None),
        CleanupMode::Clean => {
            if transcript.trim().is_empty() {
                return TranscriptCleanupResult {
                    text: String::new(),
                    mode: CleanupMode::Clean,
                    warning: None,
                };
            }

            let api_key = match manager.get_cerebras_api_key_secret() {
                Ok(Some(api_key)) => api_key,
                Ok(None) => {
                    let warning =
                        "Clean cleanup needs a Cerebras API key. Floe used Fast cleanup instead.";
                    return fallback_to_fast(&transcript, Some(warning.to_string()));
                }
                Err(error) => return fallback_to_fast(&transcript, Some(error.message)),
            };

            match clean_with_cerebras(api_key, transcript.clone()).await {
                Ok(text) => TranscriptCleanupResult {
                    text,
                    mode: CleanupMode::Clean,
                    warning: None,
                },
                Err(error) => fallback_to_fast(&transcript, Some(error.message)),
            }
        }
    }
}

pub fn cleanup_transcript_local(transcript: &str) -> String {
    let mut cleaned = transcript.split_whitespace().collect::<Vec<_>>().join(" ");

    if cleaned.is_empty() {
        return String::new();
    }

    cleaned = remove_spaces_before_punctuation(&cleaned);
    cleaned = normalize_spaces_after_punctuation(&cleaned);
    cleaned = capitalize_first_alphabetical_character(&cleaned);

    if !has_terminal_punctuation(&cleaned) {
        cleaned.push('.');
    }

    cleaned
}

fn fallback_to_fast(transcript: &str, warning: Option<String>) -> TranscriptCleanupResult {
    fast_result(transcript, warning)
}

fn fast_result(transcript: &str, warning: Option<String>) -> TranscriptCleanupResult {
    TranscriptCleanupResult {
        text: cleanup_transcript_local(transcript),
        mode: CleanupMode::Fast,
        warning,
    }
}

fn remove_spaces_before_punctuation(value: &str) -> String {
    let mut output = String::with_capacity(value.len());

    for character in value.chars() {
        if matches!(character, ',' | '.' | ';' | ':' | '!' | '?') {
            while output.ends_with(' ') {
                output.pop();
            }
        }

        output.push(character);
    }

    output
}

fn normalize_spaces_after_punctuation(value: &str) -> String {
    let characters: Vec<char> = value.chars().collect();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;

    while index < characters.len() {
        let character = characters[index];
        output.push(character);

        if matches!(character, ',' | '.' | ';' | ':' | '!' | '?')
            && characters
                .get(index + 1)
                .is_some_and(|next| next.is_alphabetic())
            && !(character == '.'
                && index > 0
                && characters[index - 1].is_ascii_digit()
                && characters
                    .get(index + 1)
                    .is_some_and(|next| next.is_ascii_digit()))
        {
            output.push(' ');
        }

        index += 1;
    }

    output
}

fn capitalize_first_alphabetical_character(value: &str) -> String {
    for (index, character) in value.char_indices() {
        if character.is_alphabetic() {
            let mut output = String::with_capacity(value.len());
            output.push_str(&value[..index]);
            output.extend(character.to_uppercase());
            output.push_str(&value[index + character.len_utf8()..]);
            return output;
        }
    }

    value.to_string()
}

fn has_terminal_punctuation(value: &str) -> bool {
    value
        .chars()
        .last()
        .is_some_and(|character| matches!(character, '.' | '!' | '?'))
}

#[allow(dead_code)]
fn settings_warning(error: SettingsError) -> Option<String> {
    match error.code {
        SettingsErrorCode::SecretStoreUnavailable
        | SettingsErrorCode::AppSettingsUnavailable
        | SettingsErrorCode::MissingCerebrasApiKey => Some(error.message),
        _ => None,
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

    use super::{cleanup_transcript_local, cleanup_transcript_with_provider};
    use crate::{
        providers::cerebras::{CerebrasCleanupError, CerebrasCleanupErrorCode},
        settings::{CleanupMode, SecretStore, SettingsError, SettingsManager},
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

    #[test]
    fn local_cleanup_matches_fast_cleanup_rules() {
        assert_eq!(cleanup_transcript_local("hallo welt"), "Hallo welt.");
        assert_eq!(
            cleanup_transcript_local(" ich   teste das  "),
            "Ich teste das."
        );
        assert_eq!(cleanup_transcript_local("okay danke!"), "Okay danke!");
        assert_eq!(cleanup_transcript_local(""), "");
        assert_eq!(cleanup_transcript_local("hallo , welt !"), "Hallo, welt!");
        assert_eq!(
            cleanup_transcript_local("okay,danke!weiter"),
            "Okay, danke! weiter."
        );
        assert_eq!(cleanup_transcript_local("pi ist 3.14"), "Pi ist 3.14.");
        assert_eq!(cleanup_transcript_local("... hallo"), "... Hallo.");
        assert_eq!(cleanup_transcript_local("uber floe"), "Uber floe.");
        assert_eq!(
            cleanup_transcript_local("version 1.2,weiter"),
            "Version 1.2, weiter."
        );
    }

    #[tokio::test]
    async fn raw_mode_returns_transcript_unchanged() {
        let manager = test_manager();
        manager.set_cleanup_mode(CleanupMode::Raw).unwrap();

        let result =
            cleanup_transcript_with_provider(&manager, "raw text".to_string(), |_, _| async {
                Ok("unused".to_string())
            })
            .await;

        assert_eq!(result.text, "raw text");
        assert_eq!(result.mode, CleanupMode::Raw);
        assert!(result.warning.is_none());
    }

    #[tokio::test]
    async fn fast_mode_uses_local_cleanup() {
        let manager = test_manager();

        let result =
            cleanup_transcript_with_provider(&manager, "fast text".to_string(), |_, _| async {
                Ok("unused".to_string())
            })
            .await;

        assert_eq!(result.text, "Fast text.");
        assert_eq!(result.mode, CleanupMode::Fast);
        assert!(result.warning.is_none());
    }

    #[tokio::test]
    async fn clean_mode_success_uses_cerebras_text() {
        let manager = test_manager();
        manager
            .save_cerebras_api_key("csk_12345678wxyz".to_string())
            .unwrap();
        manager.set_cleanup_mode(CleanupMode::Clean).unwrap();

        let result = cleanup_transcript_with_provider(
            &manager,
            "clean text".to_string(),
            |key, text| async move {
                assert_eq!(key, "csk_12345678wxyz");
                assert_eq!(text, "clean text");
                Ok("Clean text.".to_string())
            },
        )
        .await;

        assert_eq!(result.text, "Clean text.");
        assert_eq!(result.mode, CleanupMode::Clean);
        assert!(result.warning.is_none());
    }

    #[tokio::test]
    async fn clean_mode_missing_key_falls_back_to_fast() {
        let manager = test_manager();
        manager
            .save_app_settings(crate::settings::AppSettings {
                hotkey: crate::settings::HotkeySettings::default(),
                cleanup_mode: CleanupMode::Clean,
            })
            .unwrap();

        let result =
            cleanup_transcript_with_provider(&manager, "clean text".to_string(), |_, _| async {
                Ok("unused".to_string())
            })
            .await;

        assert_eq!(result.text, "Clean text.");
        assert_eq!(result.mode, CleanupMode::Fast);
        assert!(result.warning.unwrap().contains("Cerebras API key"));
    }

    #[tokio::test]
    async fn clean_mode_provider_failures_fall_back_to_fast_with_warning() {
        for code in [
            CerebrasCleanupErrorCode::Timeout,
            CerebrasCleanupErrorCode::RateLimit,
            CerebrasCleanupErrorCode::ServerError,
            CerebrasCleanupErrorCode::MalformedResponse,
            CerebrasCleanupErrorCode::ValidationFailed,
        ] {
            let manager = test_manager();
            manager
                .save_cerebras_api_key("csk_12345678wxyz".to_string())
                .unwrap();
            manager.set_cleanup_mode(CleanupMode::Clean).unwrap();

            let result =
                cleanup_transcript_with_provider(&manager, "fallback text".to_string(), |_, _| {
                    let code = code.clone();
                    async move {
                        Err(CerebrasCleanupError::new(
                            code,
                            "Cerebras cleanup failed. Floe used Fast cleanup instead.",
                        ))
                    }
                })
                .await;

            assert_eq!(result.text, "Fallback text.");
            assert_eq!(result.mode, CleanupMode::Fast);
            assert!(result.warning.is_some());
        }
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
