use std::{fs, path::PathBuf, sync::Mutex};

use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "com.floe.app";
const GROQ_API_KEY_USER: &str = "groq-api-key";
const DEFAULT_HOTKEY_ACCELERATOR: &str = "Ctrl+Space";
const DEFAULT_HOTKEY_LABEL: &str = "Ctrl+Space";
const MAX_GROQ_API_KEY_LEN: usize = 256;
const MAX_HOTKEY_ACCELERATOR_LEN: usize = 80;
const MAX_HOTKEY_LABEL_LEN: usize = 80;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GroqApiKeyStatus {
    pub configured: bool,
    pub masked_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub hotkey: HotkeySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeySettings {
    pub accelerator: String,
    pub label: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: HotkeySettings::default(),
        }
    }
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            accelerator: DEFAULT_HOTKEY_ACCELERATOR.to_string(),
            label: DEFAULT_HOTKEY_LABEL.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SettingsError {
    pub code: SettingsErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SettingsErrorCode {
    InvalidGroqApiKey,
    InvalidAppSettings,
    SecretStoreUnavailable,
    AppSettingsUnavailable,
}

pub trait SecretStore: Send + Sync + 'static {
    fn save(&self, secret: &str) -> Result<(), SettingsError>;
    fn get(&self) -> Result<Option<String>, SettingsError>;
    fn clear(&self) -> Result<(), SettingsError>;
}

pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn save(&self, secret: &str) -> Result<(), SettingsError> {
        keyring_entry()?
            .set_password(secret)
            .map_err(map_keyring_error)
    }

    fn get(&self) -> Result<Option<String>, SettingsError> {
        match keyring_entry()?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(error)),
        }
    }

    fn clear(&self) -> Result<(), SettingsError> {
        match keyring_entry()?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

pub struct SettingsManager {
    secret_store: Box<dyn SecretStore>,
    app_settings_store: AppSettingsStore,
}

impl SettingsManager {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            secret_store: Box::new(KeyringSecretStore),
            app_settings_store: AppSettingsStore::new(config_dir.join("settings.json")),
        }
    }

    #[cfg(test)]
    fn with_secret_store(secret_store: Box<dyn SecretStore>, settings_path: PathBuf) -> Self {
        Self {
            secret_store,
            app_settings_store: AppSettingsStore::new(settings_path),
        }
    }

    pub fn save_groq_api_key(&self, api_key: String) -> Result<GroqApiKeyStatus, SettingsError> {
        let api_key = validate_groq_api_key(&api_key)?;
        self.secret_store.save(&api_key)?;

        Ok(status_from_secret(Some(api_key)))
    }

    pub fn clear_groq_api_key(&self) -> Result<GroqApiKeyStatus, SettingsError> {
        self.secret_store.clear()?;

        Ok(status_from_secret(None))
    }

    pub fn get_groq_api_key_status(&self) -> Result<GroqApiKeyStatus, SettingsError> {
        match self.secret_store.get() {
            Ok(secret) => Ok(status_from_secret(secret)),
            Err(error) if error.code == SettingsErrorCode::SecretStoreUnavailable => {
                Ok(status_from_secret(None))
            }
            Err(error) => Err(error),
        }
    }

    pub fn get_groq_api_key_secret(&self) -> Result<Option<String>, SettingsError> {
        self.secret_store.get()
    }

    pub fn get_app_settings(&self) -> Result<AppSettings, SettingsError> {
        self.app_settings_store.load()
    }

    pub fn save_app_settings(&self, settings: AppSettings) -> Result<AppSettings, SettingsError> {
        let settings = validate_app_settings(settings)?;
        self.app_settings_store.save(&settings)?;

        Ok(settings)
    }
}

struct AppSettingsStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl AppSettingsStore {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    fn load(&self) -> Result<AppSettings, SettingsError> {
        let _guard = self.settings_lock()?;

        if !self.path.exists() {
            return Ok(AppSettings::default());
        }

        let raw = fs::read_to_string(&self.path).map_err(|_| app_settings_error())?;
        let settings =
            serde_json::from_str::<AppSettings>(&raw).map_err(|_| app_settings_error())?;

        validate_app_settings(settings)
    }

    fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        let _guard = self.settings_lock()?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|_| app_settings_error())?;
        }

        let raw = serde_json::to_string_pretty(settings).map_err(|_| app_settings_error())?;
        fs::write(&self.path, raw).map_err(|_| app_settings_error())
    }

    fn settings_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, SettingsError> {
        self.lock.lock().map_err(|_| app_settings_error())
    }
}

fn keyring_entry() -> Result<Entry, SettingsError> {
    Entry::new(KEYRING_SERVICE, GROQ_API_KEY_USER).map_err(map_keyring_error)
}

fn map_keyring_error(_error: KeyringError) -> SettingsError {
    settings_error(
        SettingsErrorCode::SecretStoreUnavailable,
        "Secure key storage is unavailable. Floe did not store the API key.",
    )
}

fn app_settings_error() -> SettingsError {
    settings_error(
        SettingsErrorCode::AppSettingsUnavailable,
        "App settings could not be loaded or saved.",
    )
}

fn validate_groq_api_key(api_key: &str) -> Result<String, SettingsError> {
    let trimmed = api_key.trim();

    if trimmed.is_empty()
        || trimmed.len() > MAX_GROQ_API_KEY_LEN
        || trimmed.chars().any(char::is_control)
    {
        return Err(settings_error(
            SettingsErrorCode::InvalidGroqApiKey,
            "Enter a valid Groq API key.",
        ));
    }

    Ok(trimmed.to_string())
}

fn validate_app_settings(settings: AppSettings) -> Result<AppSettings, SettingsError> {
    let hotkey_accelerator = settings.hotkey.accelerator.trim();
    let hotkey_label = settings.hotkey.label.trim();

    if hotkey_accelerator.is_empty()
        || hotkey_accelerator.len() > MAX_HOTKEY_ACCELERATOR_LEN
        || hotkey_accelerator.chars().any(char::is_control)
        || hotkey_label.is_empty()
        || hotkey_label.len() > MAX_HOTKEY_LABEL_LEN
        || hotkey_label.chars().any(char::is_control)
    {
        return Err(settings_error(
            SettingsErrorCode::InvalidAppSettings,
            "Enter a valid hotkey label.",
        ));
    }

    Ok(AppSettings {
        hotkey: HotkeySettings {
            accelerator: hotkey_accelerator.to_string(),
            label: hotkey_label.to_string(),
        },
    })
}

fn status_from_secret(secret: Option<String>) -> GroqApiKeyStatus {
    GroqApiKeyStatus {
        configured: secret.is_some(),
        masked_preview: secret.as_deref().and_then(mask_api_key),
    }
}

fn mask_api_key(api_key: &str) -> Option<String> {
    if api_key.chars().count() < 12 {
        return Some("Configured key".to_string());
    }

    let start: String = api_key.chars().take(4).collect();
    let end: String = api_key
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();

    Some(format!("{start}...{end}"))
}

fn settings_error(code: SettingsErrorCode, message: &'static str) -> SettingsError {
    SettingsError {
        code,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{
        mask_api_key, AppSettings, GroqApiKeyStatus, HotkeySettings, SecretStore, SettingsError,
        SettingsErrorCode, SettingsManager,
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

    struct UnavailableSecretStore;

    impl SecretStore for UnavailableSecretStore {
        fn save(&self, _secret: &str) -> Result<(), SettingsError> {
            Err(secret_store_unavailable_error())
        }

        fn get(&self) -> Result<Option<String>, SettingsError> {
            Err(secret_store_unavailable_error())
        }

        fn clear(&self) -> Result<(), SettingsError> {
            Err(secret_store_unavailable_error())
        }
    }

    #[test]
    fn masks_api_keys_without_exposing_short_values() {
        assert_eq!(mask_api_key("gsk_12345678abcd").unwrap(), "gsk_...abcd");
        assert_eq!(mask_api_key("short").unwrap(), "Configured key");
    }

    #[test]
    fn missing_key_reports_unconfigured_status() {
        let manager = test_manager();

        let status = manager
            .get_groq_api_key_status()
            .expect("status should load");

        assert_eq!(
            status,
            GroqApiKeyStatus {
                configured: false,
                masked_preview: None,
            }
        );
    }

    #[test]
    fn unavailable_keyring_keeps_status_unconfigured() {
        let manager = SettingsManager::with_secret_store(
            Box::new(UnavailableSecretStore),
            unique_settings_path(),
        );

        let status = manager
            .get_groq_api_key_status()
            .expect("status should not fail when keyring is unavailable");

        assert_eq!(
            status,
            GroqApiKeyStatus {
                configured: false,
                masked_preview: None,
            }
        );
    }

    #[test]
    fn clearing_key_removes_it_and_missing_clear_succeeds() {
        let manager = test_manager();

        manager
            .save_groq_api_key("gsk_12345678abcd".to_string())
            .expect("key should save");
        let cleared = manager
            .clear_groq_api_key()
            .expect("key should clear cleanly");

        assert!(!cleared.configured);
        assert_eq!(
            manager.get_groq_api_key_status().unwrap(),
            GroqApiKeyStatus {
                configured: false,
                masked_preview: None,
            }
        );
        assert!(manager.clear_groq_api_key().is_ok());
    }

    #[test]
    fn app_settings_default_hotkey_is_push_to_talk_shortcut() {
        let manager = test_manager();

        let settings = manager
            .get_app_settings()
            .expect("default settings should load");

        assert_eq!(
            settings.hotkey,
            HotkeySettings {
                accelerator: "Ctrl+Space".to_string(),
                label: "Ctrl+Space".to_string(),
            }
        );
    }

    #[test]
    fn app_settings_validation_rejects_invalid_hotkeys() {
        let manager = test_manager();

        for hotkey in [
            HotkeySettings {
                accelerator: "".to_string(),
                label: "Ctrl+Space".to_string(),
            },
            HotkeySettings {
                accelerator: "Ctrl+Space".to_string(),
                label: "   ".to_string(),
            },
            HotkeySettings {
                accelerator: "Ctrl\nSpace".to_string(),
                label: "Ctrl+Space".to_string(),
            },
            HotkeySettings {
                accelerator: "Ctrl+Space".to_string(),
                label: "Ctrl\nSpace".to_string(),
            },
        ] {
            let error = manager
                .save_app_settings(AppSettings { hotkey })
                .expect_err("invalid settings should fail");

            assert_eq!(error.code, SettingsErrorCode::InvalidAppSettings);
        }

        let too_long = "x".repeat(81);
        let error = manager
            .save_app_settings(AppSettings {
                hotkey: HotkeySettings {
                    accelerator: "Ctrl+Space".to_string(),
                    label: too_long,
                },
            })
            .expect_err("too-long settings should fail");

        assert_eq!(error.code, SettingsErrorCode::InvalidAppSettings);
    }

    #[test]
    fn app_settings_save_trims_valid_hotkey() {
        let manager = test_manager();

        let saved = manager
            .save_app_settings(AppSettings {
                hotkey: HotkeySettings {
                    accelerator: "  Ctrl+Space  ".to_string(),
                    label: "  Ctrl+Space  ".to_string(),
                },
            })
            .expect("valid settings should save");

        assert_eq!(saved.hotkey.accelerator, "Ctrl+Space");
        assert_eq!(saved.hotkey.label, "Ctrl+Space");
        assert_eq!(manager.get_app_settings().unwrap(), saved);
    }

    fn test_manager() -> SettingsManager {
        SettingsManager::with_secret_store(
            Box::<MemorySecretStore>::default(),
            unique_settings_path(),
        )
    }

    fn unique_settings_path() -> std::path::PathBuf {
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!("floe-settings-test-{}.json", unique_id))
    }

    fn secret_store_unavailable_error() -> SettingsError {
        SettingsError {
            code: SettingsErrorCode::SecretStoreUnavailable,
            message: "unavailable".to_string(),
        }
    }
}
