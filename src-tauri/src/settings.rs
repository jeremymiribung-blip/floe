use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};

const KEYRING_SERVICE: &str = "dev.floe.desktop";
const LEGACY_KEYRING_SERVICES: &[&str] = &["com.floe.app"];
const API_KEY_USER: &str = "groq-api-key";
const MAX_API_KEY_LEN: usize = 256;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    pub configured: bool,
    pub masked_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub hotkey: HotkeySettings,
    #[serde(default)]
    pub keyring_migrated: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: HotkeySettings::default(),
            keyring_migrated: false,
        }
    }
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
    pub domain: &'static str,
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

pub fn migrate_legacy_keyring_entries(settings: &mut AppSettings) {
    if settings.keyring_migrated {
        return;
    }

    for legacy_service in LEGACY_KEYRING_SERVICES {
        if *legacy_service == KEYRING_SERVICE {
            continue;
        }

        let Ok(legacy_entry) = Entry::new(legacy_service, API_KEY_USER) else {
            continue;
        };

        let Ok(target_entry) = Entry::new(KEYRING_SERVICE, API_KEY_USER) else {
            continue;
        };

        let _ = migrate_secret_from_to(
            &KeyringEntryStore(target_entry),
            &KeyringEntryStore(legacy_entry),
        );
    }

    settings.keyring_migrated = true;
}

fn migrate_secret_from_to(target: &dyn SecretStore, source: &dyn SecretStore) -> bool {
    let Ok(Some(secret)) = source.get() else {
        return false;
    };

    if target.save(&secret).is_err() {
        return false;
    }

    let _ = source.clear();
    true
}

struct KeyringEntryStore(Entry);

impl SecretStore for KeyringEntryStore {
    fn save(&self, secret: &str) -> Result<(), SettingsError> {
        self.0.set_password(secret).map_err(map_keyring_error)
    }

    fn get(&self) -> Result<Option<String>, SettingsError> {
        match self.0.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(error)),
        }
    }

    fn clear(&self) -> Result<(), SettingsError> {
        match self.0.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

pub struct SettingsManager {
    api_key_secret_store: Box<dyn SecretStore>,
    app_settings_store: AppSettingsStore,
}

impl SettingsManager {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            api_key_secret_store: Box::new(KeyringSecretStore::new(API_KEY_USER)),
            app_settings_store: AppSettingsStore::new(config_dir.join("settings.json")),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_secret_store(
        api_key_secret_store: Box<dyn SecretStore>,
        settings_path: PathBuf,
    ) -> Self {
        Self {
            api_key_secret_store,
            app_settings_store: AppSettingsStore::new(settings_path),
        }
    }

    pub fn save_api_key(&self, api_key: String) -> Result<ApiKeyStatus, SettingsError> {
        let api_key = validate_api_key(
            &api_key,
            MAX_API_KEY_LEN,
            SettingsErrorCode::InvalidGroqApiKey,
            "Enter a valid API key.",
        )?;
        self.api_key_secret_store.save(&api_key)?;

        Ok(status_from_secret(Some(api_key)))
    }

    pub fn clear_api_key(&self) -> Result<ApiKeyStatus, SettingsError> {
        self.api_key_secret_store.clear()?;

        Ok(status_from_secret(None))
    }

    pub fn get_api_key_status(&self) -> Result<ApiKeyStatus, SettingsError> {
        secret_status(&*self.api_key_secret_store)
    }

    pub fn get_api_key_secret(&self) -> Result<Option<String>, SettingsError> {
        self.api_key_secret_store.get()
    }

    pub fn get_app_settings(&self) -> Result<AppSettings, SettingsError> {
        self.app_settings_store.load()
    }

    #[cfg(test)]
    #[expect(dead_code)]
    pub async fn get_app_settings_async(&self) -> Result<AppSettings, SettingsError> {
        self.app_settings_store.load_async().await
    }

    #[cfg(test)]
    #[expect(dead_code)]
    pub async fn save_app_settings_async(
        &self,
        settings: AppSettings,
    ) -> Result<AppSettings, SettingsError> {
        let settings = validate_app_settings(settings)?;
        self.app_settings_store.save_async(&settings).await?;
        Ok(settings)
    }

    #[cfg(test)]
    pub fn restore_settings_from_backup(&self) -> Result<(), SettingsError> {
        self.app_settings_store.restore_from_backup()
    }
    pub fn save_app_settings(&self, settings: AppSettings) -> Result<AppSettings, SettingsError> {
        let settings = validate_app_settings(settings)?;
        self.app_settings_store.save(&settings)?;

        Ok(settings)
    }
}

struct AppSettingsStore {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl AppSettingsStore {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Arc::new(Mutex::new(())),
        }
    }

    fn load(&self) -> Result<AppSettings, SettingsError> {
        let _guard = self.settings_lock()?;
        Self::load_from_path(&self.path)
    }

    #[cfg(test)]
    async fn load_async(&self) -> Result<AppSettings, SettingsError> {
        let path = self.path.clone();
        let lock = Arc::clone(&self.lock);
        tokio::task::spawn_blocking(move || {
            let _guard = lock.lock().map_err(log_then_settings_error)?;
            Self::load_from_path(&path)
        })
        .await
        .map_err(log_then_settings_error)?
    }

    fn load_from_path(path: &PathBuf) -> Result<AppSettings, SettingsError> {
        if !path.exists() {
            return Ok(AppSettings::default());
        }

        let raw = fs::read_to_string(path).map_err(log_then_settings_error)?;

        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(value) => {
                // JSON parsed successfully; validate hotkey content
                let hotkey = load_hotkey_settings(value.get("hotkey"))?;
                let settings = AppSettings {
                    hotkey,
                    ..Default::default()
                };
                validate_app_settings(settings)
            }
            Err(_) => {
                // Primary file failed to parse as valid JSON, try backup
                let backup = Self::backup_path(path);
                if backup.exists() {
                    Self::load_settings_from_file(&backup).or_else(|_| Ok(AppSettings::default()))
                } else {
                    Ok(AppSettings::default())
                }
            }
        }
    }

    fn load_settings_from_file(path: &PathBuf) -> Result<AppSettings, SettingsError> {
        let raw = fs::read_to_string(path).map_err(log_then_settings_error)?;
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(log_then_settings_error)?;

        let hotkey = load_hotkey_settings(value.get("hotkey"))?;

        let settings = AppSettings {
            hotkey,
            ..Default::default()
        };
        validate_app_settings(settings)
    }

    fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        let _guard = self.settings_lock()?;
        Self::save_to_path(&self.path, settings)
    }

    #[cfg(test)]
    async fn save_async(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        let path = self.path.clone();
        let lock = Arc::clone(&self.lock);
        let settings = settings.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = lock.lock().map_err(log_then_settings_error)?;
            Self::save_to_path(&path, &settings)
        })
        .await
        .map_err(log_then_settings_error)?
    }

    fn save_to_path(path: &PathBuf, settings: &AppSettings) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(log_then_settings_error)?;
        }

        // Create backup of existing file before overwriting
        if path.exists() {
            let backup = Self::backup_path(path);
            fs::copy(path, &backup).map_err(log_then_settings_error)?;
        }

        let raw = serde_json::to_string_pretty(settings).map_err(log_then_settings_error)?;
        fs::write(path, raw).map_err(log_then_settings_error)
    }

    fn backup_path(path: &Path) -> PathBuf {
        path.with_extension("json.bak")
    }

    #[cfg(test)]
    fn restore_backup_to_path(path: &Path) -> Result<(), SettingsError> {
        let backup = Self::backup_path(path);
        if !backup.exists() {
            return Err(app_settings_error());
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(log_then_settings_error)?;
        }

        fs::copy(&backup, path).map_err(log_then_settings_error)?;
        Ok(())
    }

    #[cfg(test)]
    fn restore_from_backup(&self) -> Result<(), SettingsError> {
        let _guard = self.settings_lock()?;
        Self::restore_backup_to_path(&self.path)
    }

    fn settings_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, SettingsError> {
        self.lock.lock().map_err(log_then_settings_error)
    }
}

fn map_keyring_error(_error: KeyringError) -> SettingsError {
    settings_error(
        SettingsErrorCode::SecretStoreUnavailable,
        "Secure key storage is unavailable. Floe did not store the API key.",
    )
}

/// Log the original error detail, then return the generic app_settings_error.
/// Preserves diagnostic info that was previously discarded.
fn log_then_settings_error<E: std::fmt::Display>(e: E) -> SettingsError {
    log::warn!("settings_error_detail=\"{}\"", e);
    app_settings_error()
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
    let settings = crate::system::hotkey::validate_app_hotkey_settings(settings)?;

    Ok(settings)
}

fn load_hotkey_settings(
    value: Option<&serde_json::Value>,
) -> Result<HotkeySettings, SettingsError> {
    let Some(value) = value else {
        return Ok(HotkeySettings::default());
    };

    if value.is_null() {
        return Ok(HotkeySettings::default());
    }

    if let Some(accelerator) = value.as_str() {
        return Ok(HotkeySettings {
            accelerator: accelerator.to_string(),
            label: String::new(),
        });
    }

    let Some(object) = value.as_object() else {
        return Err(settings_error(
            SettingsErrorCode::InvalidAppSettings,
            "App settings contain an unsupported hotkey value.",
        ));
    };

    let Some(accelerator_value) = object.get("accelerator") else {
        return Ok(HotkeySettings::default());
    };
    let Some(accelerator) = accelerator_value.as_str() else {
        return Err(settings_error(
            SettingsErrorCode::InvalidAppSettings,
            "App settings contain an unsupported hotkey value.",
        ));
    };
    let label = object
        .get("label")
        .and_then(|label| label.as_str())
        .unwrap_or_default();

    Ok(HotkeySettings {
        accelerator: accelerator.to_string(),
        label: label.to_string(),
    })
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
        domain: "settings",
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
        mask_api_key, migrate_secret_from_to, ApiKeyStatus, AppSettings, HotkeySettings,
        SecretStore, SettingsError, SettingsErrorCode, SettingsManager, KEYRING_SERVICE,
        LEGACY_KEYRING_SERVICES, MAX_API_KEY_LEN,
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
    fn keyring_service_matches_documented_bundle_identifier() {
        assert_eq!(KEYRING_SERVICE, "dev.floe.desktop");
    }

    #[test]
    fn legacy_keyring_services_are_recorded_for_migration() {
        assert!(LEGACY_KEYRING_SERVICES.contains(&"com.floe.app"));
        assert!(
            LEGACY_KEYRING_SERVICES
                .iter()
                .all(|service| *service != KEYRING_SERVICE),
            "legacy services must not duplicate the current service",
        );
    }

    #[test]
    fn migrate_secret_from_to_moves_value_and_clears_source() {
        let source = MemorySecretStore::default();
        let target = MemorySecretStore::default();
        source.save("gsk_legacy_value").unwrap();

        let migrated = migrate_secret_from_to(&target, &source);

        assert!(migrated);
        assert_eq!(target.get().unwrap(), Some("gsk_legacy_value".to_string()));
        assert_eq!(source.get().unwrap(), None);
    }

    #[test]
    fn migrate_secret_from_to_returns_false_when_source_is_empty() {
        let source = MemorySecretStore::default();
        let target = MemorySecretStore::default();

        let migrated = migrate_secret_from_to(&target, &source);

        assert!(!migrated);
        assert_eq!(target.get().unwrap(), None);
    }

    #[test]
    fn migrate_secret_from_to_does_not_clear_source_when_target_save_fails() {
        let source = MemorySecretStore::default();
        source.save("gsk_legacy_value").unwrap();
        let target = UnavailableSecretStore;

        let migrated = migrate_secret_from_to(&target, &source);

        assert!(!migrated);
        assert_eq!(source.get().unwrap(), Some("gsk_legacy_value".to_string()));
    }

    #[test]
    fn missing_keys_report_unconfigured_status() {
        let manager = test_manager();

        assert_eq!(
            manager.get_api_key_status().unwrap(),
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
            manager.get_api_key_status().unwrap(),
            ApiKeyStatus {
                configured: false,
                masked_preview: None,
            }
        );
    }

    #[test]
    fn api_key_round_trips_through_settings_manager() {
        let manager = test_manager();

        manager
            .save_api_key("gsk_12345678abcd".to_string())
            .unwrap();

        assert_eq!(
            manager.get_api_key_secret().unwrap(),
            Some("gsk_12345678abcd".to_string())
        );

        manager.clear_api_key().unwrap();
        assert!(manager.get_api_key_secret().unwrap().is_none());
    }

    #[test]
    fn saving_keys_trims_secret_and_returns_only_masked_status() {
        let manager = test_manager();
        let api_key = "gsk_12345678abcd";

        let status = manager
            .save_api_key(format!("  {api_key}  "))
            .expect("API key should save");
        let serialized = serde_json::to_string(&status.clone()).unwrap();

        assert_eq!(
            status,
            ApiKeyStatus {
                configured: true,
                masked_preview: Some("gsk_...abcd".to_string()),
            }
        );
        assert!(!serialized.contains(api_key));
    }

    #[test]
    fn invalid_keys_are_rejected_without_storing_secret() {
        let manager = test_manager();

        for api_key in [
            String::new(),
            "   ".to_string(),
            "gsk_valid_prefix\nwith_control".to_string(),
            "x".repeat(MAX_API_KEY_LEN + 1),
        ] {
            let error = manager
                .save_api_key(api_key)
                .expect_err("invalid API key should fail");

            assert_eq!(error.code, SettingsErrorCode::InvalidGroqApiKey);
        }

        assert_eq!(manager.get_api_key_secret().unwrap(), None);
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
    fn app_settings_loads_legacy_string_hotkey() {
        let path = unique_settings_path();
        fs::write(&path, r#"{"hotkey":"Control+Shift+KeyA"}"#)
            .expect("legacy settings should write");
        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let settings = manager.get_app_settings().unwrap();

        assert_eq!(settings.hotkey.accelerator, "Control+Shift+KeyA");
        assert_eq!(settings.hotkey.label, "Ctrl + Shift + A");
    }

    #[test]
    fn app_settings_loads_partial_hotkey_with_missing_label() {
        let path = unique_settings_path();
        fs::write(&path, r#"{"hotkey":{"accelerator":"Control+Shift+KeyA"}}"#)
            .expect("partial settings should write");
        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let settings = manager.get_app_settings().unwrap();

        assert_eq!(settings.hotkey.accelerator, "Control+Shift+KeyA");
        assert_eq!(settings.hotkey.label, "Ctrl + Shift + A");
    }

    #[test]
    fn app_settings_loads_default_from_partial_hotkey_without_accelerator() {
        let path = unique_settings_path();
        fs::write(&path, r#"{"hotkey":{"label":"Ctrl + Space"}}"#)
            .expect("partial settings should write");
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

        assert_eq!(settings.hotkey.label, "Ctrl + Shift + A");
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
                ..Default::default()
            })
            .unwrap();

        let saved_raw = fs::read_to_string(&path).expect("settings should be written");
        assert!(!saved_raw.contains("cleanupMode"));
        assert!(!saved_raw.contains("cleanup_mode"));
    }

    #[test]
    fn corrupt_app_settings_file_returns_defaults_with_backup_fallback() {
        let path = unique_settings_path();
        fs::write(&path, "{not valid json").expect("corrupt settings should write");
        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let settings = manager
            .get_app_settings()
            .expect("corrupt settings should fall back to defaults");

        assert_eq!(settings, AppSettings::default());
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
                label: "Ctrl + Space".to_string(),
            },
            HotkeySettings {
                accelerator: "Control\nShift+Space".to_string(),
                label: "Control+Shift+Space".to_string(),
            },
        ] {
            let error = manager
                .save_app_settings(AppSettings {
                    hotkey,
                    ..Default::default()
                })
                .expect_err("invalid settings should fail");

            assert_eq!(error.code, SettingsErrorCode::InvalidAppSettings);
        }

        let too_long = "x".repeat(81);
        let error = manager
            .save_app_settings(AppSettings {
                hotkey: HotkeySettings {
                    accelerator: too_long,
                    label: "Ctrl + Shift + Space".to_string(),
                },
                ..Default::default()
            })
            .expect_err("too-long settings should fail");

        assert_eq!(error.code, SettingsErrorCode::InvalidAppSettings);
    }

    #[test]
    fn app_settings_accepts_platform_space_defaults() {
        let manager = test_manager();

        let saved = manager
            .save_app_settings(AppSettings {
                hotkey: HotkeySettings {
                    accelerator: "Control+Space".to_string(),
                    label: "Ctrl + Space".to_string(),
                },
                ..Default::default()
            })
            .expect("Control+Space should save");

        assert_eq!(saved.hotkey.accelerator, "Control+Space");
        assert_eq!(saved.hotkey.label, "Ctrl + Space");
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
                ..Default::default()
            })
            .expect("valid settings should save");

        assert_eq!(saved.hotkey.accelerator, "Control+Shift+KeyA");
        assert_eq!(saved.hotkey.label, "Ctrl + Shift + A");
        assert_eq!(manager.get_app_settings().unwrap(), saved);
    }

    #[test]
    fn save_creates_backup_file() {
        let path = unique_settings_path();
        let backup = path.with_extension("json.bak");
        let manager = SettingsManager::with_secret_store(
            Box::new(MemorySecretStore::default()),
            path.clone(),
        );

        // First save creates the file but no backup (no existing file)
        manager.save_app_settings(AppSettings::default()).unwrap();
        assert!(path.exists());
        assert!(!backup.exists(), "backup should not exist on first save");

        // Second save creates a backup of the first settings
        manager
            .save_app_settings(AppSettings {
                hotkey: HotkeySettings {
                    accelerator: "Control+Shift+KeyA".to_string(),
                    label: "Ctrl + Shift + A".to_string(),
                },
                ..Default::default()
            })
            .unwrap();

        assert!(backup.exists(), "backup should exist after second save");

        // Verify backup contains original settings (defaults)
        let backed_up: AppSettings =
            serde_json::from_str(&fs::read_to_string(&backup).unwrap()).unwrap();
        assert_eq!(backed_up, AppSettings::default());
    }

    #[test]
    fn load_falls_back_to_backup_when_primary_corrupt() {
        let path = unique_settings_path();
        let backup = path.with_extension("json.bak");

        // Write valid settings to backup
        let valid = r#"{"hotkey":{"accelerator":"Control+Shift+KeyA","label":"Ctrl + Shift + A"}}"#;
        fs::write(&backup, valid).unwrap();

        // Write corrupt content to primary
        fs::write(&path, "{not valid json").unwrap();

        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let settings = manager.get_app_settings().unwrap();
        assert_eq!(settings.hotkey.accelerator, "Control+Shift+KeyA");
        assert_eq!(settings.hotkey.label, "Ctrl + Shift + A");
    }

    #[test]
    fn load_returns_defaults_when_both_files_corrupt() {
        let path = unique_settings_path();
        let backup = path.with_extension("json.bak");

        fs::write(&path, "{corrupt primary").unwrap();
        fs::write(&backup, "{corrupt backup").unwrap();

        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let settings = manager.get_app_settings().unwrap();
        assert_eq!(settings, AppSettings::default());
    }

    #[test]
    fn restore_from_backup_works() {
        let path = unique_settings_path();
        let backup = path.with_extension("json.bak");

        // Write primary with one set of settings
        let primary = r#"{"hotkey":{"accelerator":"Control+Space","label":"Ctrl + Space"}}"#;
        fs::write(&path, primary).unwrap();

        // Write backup with different settings
        let backup_content =
            r#"{"hotkey":{"accelerator":"Control+Shift+KeyA","label":"Ctrl + Shift + A"}}"#;
        fs::write(&backup, backup_content).unwrap();

        let manager = SettingsManager::with_secret_store(
            Box::new(MemorySecretStore::default()),
            path.clone(),
        );

        manager.restore_settings_from_backup().unwrap();

        let settings = manager.get_app_settings().unwrap();
        assert_eq!(settings.hotkey.accelerator, "Control+Shift+KeyA");
        assert_eq!(settings.hotkey.label, "Ctrl + Shift + A");
    }

    #[test]
    fn restore_from_backup_fails_when_no_backup() {
        let path = unique_settings_path();
        // No backup file exists

        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let error = manager
            .restore_settings_from_backup()
            .expect_err("restore without backup should fail");

        assert_eq!(error.code, SettingsErrorCode::AppSettingsUnavailable);
    }

    #[test]
    fn load_returns_defaults_when_primary_missing_and_no_backup() {
        let path = unique_settings_path();
        // Neither path nor backup exists

        let manager =
            SettingsManager::with_secret_store(Box::new(MemorySecretStore::default()), path);

        let settings = manager.get_app_settings().unwrap();
        assert_eq!(settings, AppSettings::default());
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
            domain: "settings",
            code: SettingsErrorCode::SecretStoreUnavailable,
            message: "unavailable".to_string(),
        }
    }
}
