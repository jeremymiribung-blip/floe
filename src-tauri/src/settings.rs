use std::{fs, path::PathBuf, sync::Mutex};

use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "com.floe.app";
const GROQ_API_KEY_USER: &str = "groq-api-key";
const LEGACY_CEREBRAS_API_KEY_USER: &str = "cerebras-api-key";
const MAX_GROQ_API_KEY_LEN: usize = 256;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    pub configured: bool,
    pub masked_preview: Option<String>,
}

pub type GroqApiKeyStatus = ApiKeyStatus;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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

impl Default for HotkeySettings {
    fn default() -> Self {
        crate::system::hotkey::default_hotkey_settings()
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

pub struct KeyringSecretStore {
    user: &'static str,
}

impl KeyringSecretStore {
    fn new(user: &'static str) -> Self {
        Self { user }
    }

    fn entry(&self) -> Result<Entry, SettingsError> {
        Entry::new(KEYRING_SERVICE, self.user).map_err(map_keyring_error)
    }
}

impl SecretStore for KeyringSecretStore {
    fn save(&self, secret: &str) -> Result<(), SettingsError> {
        self.entry()?
            .set_password(secret)
            .map_err(map_keyring_error)
    }

    fn get(&self) -> Result<Option<String>, SettingsError> {
        match self.entry()?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(error)),
        }
    }

    fn clear(&self) -> Result<(), SettingsError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

pub struct SettingsManager {
    groq_secret_store: Box<dyn SecretStore>,
    app_settings_store: AppSettingsStore,
}

impl SettingsManager {
    pub fn new(config_dir: PathBuf) -> Self {
        Self::migrate_legacy_cerebras_keyring_entry();

        Self {
            groq_secret_store: Box::new(KeyringSecretStore::new(GROQ_API_KEY_USER)),
            app_settings_store: AppSettingsStore::new(config_dir.join("settings.json")),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_secret_store(
        groq_secret_store: Box<dyn SecretStore>,
        settings_path: PathBuf,
    ) -> Self {
        Self {
            groq_secret_store,
            app_settings_store: AppSettingsStore::new(settings_path),
        }
    }

    pub fn save_groq_api_key(&self, api_key: String) -> Result<GroqApiKeyStatus, SettingsError> {
        let api_key = validate_api_key(
            &api_key,
            MAX_GROQ_API_KEY_LEN,
            SettingsErrorCode::InvalidGroqApiKey,
            "Enter a valid Groq API key.",
        )?;
        self.groq_secret_store.save(&api_key)?;

        Ok(status_from_secret(Some(api_key)))
    }

    pub fn clear_groq_api_key(&self) -> Result<GroqApiKeyStatus, SettingsError> {
        self.groq_secret_store.clear()?;

        Ok(status_from_secret(None))
    }

    pub fn get_groq_api_key_status(&self) -> Result<GroqApiKeyStatus, SettingsError> {
        secret_status(&*self.groq_secret_store)
    }

    pub fn get_groq_api_key_secret(&self) -> Result<Option<String>, SettingsError> {
        self.groq_secret_store.get()
    }

    pub fn get_app_settings(&self) -> Result<AppSettings, SettingsError> {
        self.app_settings_store.load()
    }

    pub fn save_app_settings(&self, settings: AppSettings) -> Result<AppSettings, SettingsError> {
        let settings = validate_app_settings(settings)?;
        self.app_settings_store.save(&settings)?;

        Ok(settings)
    }

    fn migrate_legacy_cerebras_keyring_entry() {
        let Ok(entry) = Entry::new(KEYRING_SERVICE, LEGACY_CEREBRAS_API_KEY_USER) else {
            return;
        };
        if let Err(error) = entry.delete_credential() {
            if !matches!(error, KeyringError::NoEntry) {
                eprintln!("[floe:settings] legacy Cerebras keyring entry could not be removed");
            }
        }
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
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|_| app_settings_error())?;

        let hotkey = match value.get("hotkey") {
            Some(hotkey_value) => serde_json::from_value::<HotkeySettings>(hotkey_value.clone())
                .map_err(|_| {
                    settings_error(
                        SettingsErrorCode::InvalidAppSettings,
                        "App settings contain an unsupported hotkey value.",
                    )
                })?,
            None => HotkeySettings::default(),
        };

        Ok(AppSettings { hotkey })
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

fn secret_status(secret_store: &dyn SecretStore) -> Result<ApiKeyStatus, SettingsError> {
    match secret_store.get() {
        Ok(secret) => Ok(status_from_secret(secret)),
        Err(error) if error.code == SettingsErrorCode::SecretStoreUnavailable => {
            Ok(status_from_secret(None))
        }
        Err(error) => Err(error),
    }
}

fn validate_api_key(
    api_key: &str,
    max_len: usize,
    code: SettingsErrorCode,
    message: &'static str,
) -> Result<String, SettingsError> {
    let trimmed = api_key.trim();

    if trimmed.is_empty() || trimmed.len() > max_len || trimmed.chars().any(char::is_control) {
        return Err(settings_error(code, message));
    }

    Ok(trimmed.to_string())
}

fn validate_app_settings(settings: AppSettings) -> Result<AppSettings, SettingsError> {
    crate::system::hotkey::validate_app_hotkey_settings(settings)
}

fn status_from_secret(secret: Option<String>) -> ApiKeyStatus {
    ApiKeyStatus {
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
    use std::{
        fs,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
    };

    use super::{
        mask_api_key, ApiKeyStatus, AppSettings, HotkeySettings, SecretStore, SettingsError,
        SettingsErrorCode, SettingsManager, MAX_GROQ_API_KEY_LEN,
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
    fn missing_keys_report_unconfigured_status() {
        let manager = test_manager();

        assert_eq!(
            manager.get_groq_api_key_status().unwrap(),
            ApiKeyStatus {
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

        assert_eq!(
            manager.get_groq_api_key_status().unwrap(),
            ApiKeyStatus {
                configured: false,
                masked_preview: None,
            }
        );
    }

    #[test]
    fn groq_key_round_trips_through_settings_manager() {
        let manager = test_manager();

        manager
            .save_groq_api_key("gsk_12345678abcd".to_string())
            .unwrap();

        assert_eq!(
            manager.get_groq_api_key_secret().unwrap(),
            Some("gsk_12345678abcd".to_string())
        );

        manager.clear_groq_api_key().unwrap();
        assert!(manager.get_groq_api_key_secret().unwrap().is_none());
    }

    #[test]
    fn saving_keys_trims_secret_and_returns_only_masked_status() {
        let manager = test_manager();
        let groq_key = "gsk_12345678abcd";

        let groq_status = manager
            .save_groq_api_key(format!("  {groq_key}  "))
            .expect("Groq key should save");
        let serialized = serde_json::to_string(&groq_status.clone()).unwrap();

        assert_eq!(
            groq_status,
            ApiKeyStatus {
                configured: true,
                masked_preview: Some("gsk_...abcd".to_string()),
            }
        );
        assert!(!serialized.contains(groq_key));
    }

    #[test]
    fn invalid_keys_are_rejected_without_storing_secret() {
        let manager = test_manager();

        for api_key in [
            String::new(),
            "   ".to_string(),
            "gsk_valid_prefix\nwith_control".to_string(),
            "x".repeat(MAX_GROQ_API_KEY_LEN + 1),
        ] {
            let error = manager
                .save_groq_api_key(api_key)
                .expect_err("invalid Groq key should fail");

            assert_eq!(error.code, SettingsErrorCode::InvalidGroqApiKey);
        }

        assert_eq!(manager.get_groq_api_key_secret().unwrap(), None);
    }

    #[test]
    fn app_settings_default_hotkey_only() {
        let manager = test_manager();

        let settings = manager.get_app_settings().unwrap();

        assert_eq!(settings.hotkey, HotkeySettings::default());
    }

    #[test]
    fn app_settings_loads_defaults_from_legacy_empty_file() {
        let path = unique_settings_path();
        fs::write(&path, "{}").expect("legacy settings should write");
        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let settings = manager.get_app_settings().unwrap();

        assert_eq!(settings.hotkey, HotkeySettings::default());
    }

    #[test]
    fn app_settings_ignores_legacy_cleanup_mode_field() {
        let path = unique_settings_path();
        fs::write(
            &path,
            r#"{"hotkey":{"accelerator":"Ctrl+Shift+KeyA","label":"Ctrl+Shift+A"},"cleanupMode":"fast"}"#,
        )
        .expect("legacy settings should write");
        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let settings = manager.get_app_settings().unwrap();

        assert_eq!(settings.hotkey.label, "Ctrl+Shift+A");
    }

    #[test]
    fn app_settings_default_file_is_created_without_cleanup_mode() {
        let path = unique_settings_path();
        let manager = SettingsManager::with_secret_store(
            Box::new(MemorySecretStore::default()),
            path.clone(),
        );

        manager
            .save_app_settings(AppSettings {
                hotkey: HotkeySettings::default(),
            })
            .unwrap();

        let saved_raw = fs::read_to_string(&path).expect("settings should be written");
        assert!(!saved_raw.contains("cleanupMode"));
        assert!(!saved_raw.contains("cleanup_mode"));
    }

    #[test]
    fn corrupt_app_settings_file_returns_settings_error() {
        let path = unique_settings_path();
        fs::write(&path, "{not valid json").expect("corrupt settings should write");
        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let error = manager
            .get_app_settings()
            .expect_err("corrupt settings should fail");

        assert_eq!(error.code, SettingsErrorCode::AppSettingsUnavailable);
    }

    #[test]
    fn malformed_hotkey_returns_invalid_settings_error() {
        let path = unique_settings_path();
        fs::write(
            &path,
            r#"{"hotkey":{"accelerator":42,"label":"Ctrl+Shift+Space"}}"#,
        )
        .expect("settings should write");
        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let error = manager
            .get_app_settings()
            .expect_err("malformed hotkey should fail");

        assert_eq!(error.code, SettingsErrorCode::InvalidAppSettings);
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
                accelerator: "Control\nShift+Space".to_string(),
                label: "Control+Shift+Space".to_string(),
            },
            HotkeySettings {
                accelerator: "Control+Space".to_string(),
                label: "Control+Space".to_string(),
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
                    accelerator: too_long,
                    label: "Control+Shift+Space".to_string(),
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
                    accelerator: "  Control+Shift+A  ".to_string(),
                    label: "  Control+Shift+A  ".to_string(),
                },
            })
            .expect("valid settings should save");

        assert_eq!(saved.hotkey.accelerator, "Control+Shift+KeyA");
        assert_eq!(saved.hotkey.label, "Control+Shift+A");
        assert_eq!(manager.get_app_settings().unwrap(), saved);
    }

    fn test_manager() -> SettingsManager {
        SettingsManager::with_secret_store(
            Box::<MemorySecretStore>::default(),
            unique_settings_path(),
        )
    }

    fn unique_settings_path() -> std::path::PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);

        std::env::temp_dir().join(format!("floe-settings-test-{}-{}.json", unique_id, counter))
    }

    fn secret_store_unavailable_error() -> SettingsError {
        SettingsError {
            code: SettingsErrorCode::SecretStoreUnavailable,
            message: "unavailable".to_string(),
        }
    }
}
