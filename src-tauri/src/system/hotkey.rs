use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};

use crate::{
    lifecycle::{log_lifecycle, LifecycleLevel},
    settings::{AppSettings, HotkeySettings, SettingsError, SettingsManager},
};

pub const HOTKEY_EVENT: &str = "floe-global-hotkey-state";
const HOTKEY_UNAVAILABLE: &str = "Hotkey unavailable";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub accelerator: String,
    pub label: String,
    pub is_default: bool,
    pub is_registered: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyError {
    pub code: HotkeyErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyErrorCode {
    InvalidHotkey,
    UnsupportedHotkey,
    AlreadyInUse,
    RegistrationFailed,
    UnregisterFailed,
    SettingsUnavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HotkeyEventPayload {
    state: HotkeyEventState,
}

#[derive(Debug, Clone, Copy, Serialize)]
enum HotkeyEventState {
    Pressed,
    Released,
}

#[derive(Default)]
pub struct HotkeyManager {
    state: Mutex<HotkeyRuntimeState>,
}

#[derive(Default)]
struct HotkeyRuntimeState {
    registered: Option<HotkeySettings>,
    registration_error: Option<String>,
}

pub trait HotkeyRegistrar {
    fn register(&mut self, hotkey: &HotkeySettings) -> Result<(), HotkeyError>;
    fn unregister(&mut self, hotkey: &HotkeySettings) -> Result<(), HotkeyError>;
}

pub struct TauriHotkeyRegistrar<'a, R: Runtime> {
    app: &'a AppHandle<R>,
}

impl<'a, R: Runtime> TauriHotkeyRegistrar<'a, R> {
    pub fn new(app: &'a AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> HotkeyRegistrar for TauriHotkeyRegistrar<'_, R> {
    fn register(&mut self, hotkey: &HotkeySettings) -> Result<(), HotkeyError> {
        let accelerator = hotkey.accelerator.clone();

        self.app
            .global_shortcut()
            .on_shortcut(accelerator.as_str(), move |app, _shortcut, event| {
                emit_hotkey_event(app, event);
            })
            .map_err(map_registration_error)
    }

    fn unregister(&mut self, hotkey: &HotkeySettings) -> Result<(), HotkeyError> {
        self.app
            .global_shortcut()
            .unregister(hotkey.accelerator.as_str())
            .map_err(map_unregister_error)
    }
}

impl HotkeyManager {
    pub fn get_hotkey_settings(
        &self,
        settings_manager: &SettingsManager,
    ) -> Result<HotkeyStatus, HotkeyError> {
        let configured = settings_manager
            .get_app_settings()
            .map_err(HotkeyError::from_settings)?
            .hotkey;

        Ok(self.status(configured))
    }

    pub fn set_hotkey(
        &self,
        settings_manager: &SettingsManager,
        registrar: &mut impl HotkeyRegistrar,
        accelerator: String,
    ) -> Result<HotkeyStatus, HotkeyError> {
        let hotkey = normalize_hotkey_settings(HotkeySettings {
            accelerator,
            label: String::new(),
        })?;

        self.save_and_register(settings_manager, registrar, hotkey)
    }

    pub fn reset_hotkey_to_default(
        &self,
        settings_manager: &SettingsManager,
        registrar: &mut impl HotkeyRegistrar,
    ) -> Result<HotkeyStatus, HotkeyError> {
        self.save_and_register(settings_manager, registrar, default_hotkey_settings())
    }

    pub fn register_hotkey(
        &self,
        registrar: &mut impl HotkeyRegistrar,
        hotkey: HotkeySettings,
    ) -> Result<(), HotkeyError> {
        let hotkey = normalize_hotkey_settings(hotkey)?;
        let previous = self.current_registered();

        if previous.as_ref() == Some(&hotkey) {
            return Ok(());
        }

        if let Some(previous_hotkey) = previous.as_ref() {
            registrar.unregister(previous_hotkey)?;
        }

        match registrar.register(&hotkey) {
            Ok(()) => {
                self.set_registered(Some(hotkey), None);
                Ok(())
            }
            Err(error) => {
                let restored = previous
                    .as_ref()
                    .map(|previous_hotkey| registrar.register(previous_hotkey).is_ok())
                    .unwrap_or(false);
                let registered = if restored { previous } else { None };
                self.set_registered(registered, Some(error.message.clone()));

                Err(error)
            }
        }
    }

    pub fn unregister_hotkey(
        &self,
        registrar: &mut impl HotkeyRegistrar,
    ) -> Result<(), HotkeyError> {
        let Some(registered) = self.current_registered() else {
            self.set_registered(None, None);
            return Ok(());
        };

        registrar.unregister(&registered)?;
        self.set_registered(None, None);

        Ok(())
    }

    pub fn register_or_fallback(
        &self,
        registrar: &mut impl HotkeyRegistrar,
        configured: HotkeySettings,
    ) -> HotkeyStatus {
        let configured = normalize_hotkey_settings(configured).unwrap_or_else(|error| {
            self.set_registered(None, Some(error.message));
            default_hotkey_settings()
        });

        match self.register_hotkey(registrar, configured.clone()) {
            Ok(()) => self.status(configured),
            Err(_) => self.status(configured),
        }
    }

    fn save_and_register(
        &self,
        settings_manager: &SettingsManager,
        registrar: &mut impl HotkeyRegistrar,
        hotkey: HotkeySettings,
    ) -> Result<HotkeyStatus, HotkeyError> {
        let previous_settings = settings_manager
            .get_app_settings()
            .map_err(HotkeyError::from_settings)?;
        let previous_registered = self.current_registered();

        self.register_hotkey(registrar, hotkey.clone())?;

        let mut next_settings = previous_settings.clone();
        next_settings.hotkey = hotkey;

        match settings_manager.save_app_settings(next_settings) {
            Ok(saved) => Ok(self.status(saved.hotkey)),
            Err(error) => {
                let restore_hotkey = previous_registered.unwrap_or(previous_settings.hotkey);
                let _ = self.register_hotkey(registrar, restore_hotkey);

                Err(HotkeyError::from_settings(error))
            }
        }
    }

    fn status(&self, configured: HotkeySettings) -> HotkeyStatus {
        let state = self.state.lock().unwrap();
        let is_registered = state
            .registered
            .as_ref()
            .is_some_and(|registered| registered.accelerator == configured.accelerator);
        let error = if is_registered {
            None
        } else {
            state
                .registration_error
                .as_ref()
                .map(|_| HOTKEY_UNAVAILABLE.to_string())
        };

        HotkeyStatus {
            is_default: configured.accelerator == default_hotkey_settings().accelerator,
            accelerator: configured.accelerator,
            label: configured.label,
            is_registered,
            error,
        }
    }

    fn current_registered(&self) -> Option<HotkeySettings> {
        self.state.lock().unwrap().registered.clone()
    }

    fn set_registered(&self, registered: Option<HotkeySettings>, error: Option<String>) {
        let mut state = self.state.lock().unwrap();
        state.registered = registered;
        state.registration_error = error;
    }
}

pub fn register_startup_hotkey<R: Runtime>(app: &AppHandle<R>) {
    let configured = app
        .try_state::<SettingsManager>()
        .and_then(|manager| manager.get_app_settings().ok())
        .map(|settings| settings.hotkey)
        .unwrap_or_else(default_hotkey_settings);
    let Some(manager) = app.try_state::<HotkeyManager>() else {
        log_lifecycle(LifecycleLevel::Warn, "startup_hotkey_manager_missing");
        return;
    };
    let mut registrar = TauriHotkeyRegistrar::new(app);
    let status = manager.register_or_fallback(&mut registrar, configured);

    if status.is_registered {
        log_lifecycle(LifecycleLevel::Info, "startup_hotkey_registered");
    } else {
        log_lifecycle(LifecycleLevel::Warn, "startup_hotkey_registration_failed");
    }
}

pub fn unregister_shutdown_hotkey<R: Runtime>(app: &AppHandle<R>) {
    let Some(manager) = app.try_state::<HotkeyManager>() else {
        log_lifecycle(LifecycleLevel::Warn, "shutdown_hotkey_manager_missing");
        return;
    };
    let mut registrar = TauriHotkeyRegistrar::new(app);

    match manager.unregister_hotkey(&mut registrar) {
        Ok(()) => log_lifecycle(LifecycleLevel::Info, "shutdown_hotkey_unregistered"),
        Err(_) => log_lifecycle(LifecycleLevel::Warn, "shutdown_hotkey_unregister_failed"),
    }
}

pub fn default_hotkey_settings() -> HotkeySettings {
    default_hotkey_for_os(std::env::consts::OS)
}

pub fn default_hotkey_for_os(os: &str) -> HotkeySettings {
    if os == "macos" {
        HotkeySettings {
            accelerator: "Alt+Space".to_string(),
            label: "Option + Space".to_string(),
        }
    } else {
        HotkeySettings {
            accelerator: "Control+Space".to_string(),
            label: "Ctrl + Space".to_string(),
        }
    }
}

pub fn normalize_hotkey_settings(hotkey: HotkeySettings) -> Result<HotkeySettings, HotkeyError> {
    normalize_hotkey_settings_for_os(hotkey, std::env::consts::OS)
}

pub fn normalize_hotkey_settings_for_os(
    hotkey: HotkeySettings,
    os: &str,
) -> Result<HotkeySettings, HotkeyError> {
    let accelerator = hotkey.accelerator.trim();
    let _label = hotkey.label.trim();

    if accelerator.is_empty() || accelerator.chars().any(char::is_control) {
        return Err(invalid_hotkey_error());
    }

    let default_hotkey = default_hotkey_for_os(os);
    if accelerator.eq_ignore_ascii_case(&default_hotkey.accelerator) {
        return Ok(default_hotkey);
    }

    let shortcut = parse_shortcut(accelerator)?;
    validate_shortcut(&shortcut)?;

    let accelerator = canonical_accelerator(&shortcut);
    let label = label_from_shortcut_for_os(&shortcut, os);

    Ok(HotkeySettings { accelerator, label })
}

fn emit_hotkey_event<R: Runtime>(app: &AppHandle<R>, event: ShortcutEvent) {
    let state = match event.state {
        ShortcutState::Pressed => HotkeyEventState::Pressed,
        ShortcutState::Released => HotkeyEventState::Released,
    };

    let _ = app.emit(HOTKEY_EVENT, HotkeyEventPayload { state });
}

fn parse_shortcut(accelerator: &str) -> Result<Shortcut, HotkeyError> {
    accelerator
        .parse::<Shortcut>()
        .map_err(|_| unsupported_hotkey_error())
}

fn validate_shortcut(shortcut: &Shortcut) -> Result<(), HotkeyError> {
    let base_mods = Modifiers::SHIFT | Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER;
    let mods = shortcut.mods & base_mods;
    let has_primary_modifier =
        mods.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER);

    if !has_primary_modifier {
        return Err(unsupported_hotkey_error());
    }

    if is_floe_conflict(shortcut) || is_unsafe_system_shortcut(shortcut) {
        return Err(unsupported_hotkey_error());
    }

    Ok(())
}

fn is_floe_conflict(shortcut: &Shortcut) -> bool {
    shortcut.matches(Modifiers::CONTROL, Code::KeyV)
        || shortcut.matches(Modifiers::SUPER, Code::KeyV)
        || shortcut.matches(Modifiers::CONTROL | Modifiers::SHIFT, Code::KeyV)
        || shortcut.matches(Modifiers::SUPER | Modifiers::SHIFT, Code::KeyV)
}

fn is_unsafe_system_shortcut(shortcut: &Shortcut) -> bool {
    shortcut.matches(Modifiers::ALT, Code::F4)
        || shortcut.matches(Modifiers::SUPER, Code::KeyQ)
        || shortcut.matches(Modifiers::SUPER, Code::Space)
        || shortcut.matches(Modifiers::CONTROL | Modifiers::ALT, Code::Delete)
}

fn canonical_accelerator(shortcut: &Shortcut) -> String {
    let mut parts = modifier_parts(shortcut, false);
    parts.push(shortcut.key.to_string());

    parts.join("+")
}

fn label_from_shortcut_for_os(shortcut: &Shortcut, os: &str) -> String {
    let mut parts = modifier_parts_for_os(shortcut, true, os);
    parts.push(key_label(shortcut.key));

    parts.join(" + ")
}

fn modifier_parts(shortcut: &Shortcut, label: bool) -> Vec<String> {
    modifier_parts_for_os(shortcut, label, std::env::consts::OS)
}

fn modifier_parts_for_os(shortcut: &Shortcut, label: bool, os: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let is_macos = os == "macos";

    if shortcut.mods.contains(Modifiers::CONTROL) {
        parts.push(if label && !is_macos {
            "Ctrl".to_string()
        } else {
            "Control".to_string()
        });
    }
    if shortcut.mods.contains(Modifiers::ALT) {
        parts.push(if label && is_macos {
            "Option".to_string()
        } else {
            "Alt".to_string()
        });
    }
    if shortcut.mods.contains(Modifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
    if shortcut.mods.contains(Modifiers::SUPER) {
        parts.push(if label && is_macos {
            "Command".to_string()
        } else {
            "Super".to_string()
        });
    }

    parts
}

fn key_label(key: Code) -> String {
    let raw = key.to_string();

    raw.strip_prefix("Key")
        .or_else(|| raw.strip_prefix("Digit"))
        .unwrap_or(&raw)
        .to_string()
}

fn map_registration_error(error: tauri_plugin_global_shortcut::Error) -> HotkeyError {
    let details = error.to_string();
    let lower = details.to_lowercase();

    if lower.contains("already") || lower.contains("taken") || lower.contains("register hotkey") {
        return HotkeyError {
            code: HotkeyErrorCode::AlreadyInUse,
            message: "This shortcut is already in use.".to_string(),
        };
    }

    HotkeyError {
        code: HotkeyErrorCode::RegistrationFailed,
        message: "Hotkey could not be registered.".to_string(),
    }
}

fn map_unregister_error(_error: tauri_plugin_global_shortcut::Error) -> HotkeyError {
    HotkeyError {
        code: HotkeyErrorCode::UnregisterFailed,
        message: "Hotkey could not be unregistered.".to_string(),
    }
}

fn invalid_hotkey_error() -> HotkeyError {
    HotkeyError {
        code: HotkeyErrorCode::InvalidHotkey,
        message: "Enter a valid shortcut.".to_string(),
    }
}

fn unsupported_hotkey_error() -> HotkeyError {
    HotkeyError {
        code: HotkeyErrorCode::UnsupportedHotkey,
        message: "This shortcut is not supported.".to_string(),
    }
}

impl HotkeyError {
    pub fn from_settings(_error: SettingsError) -> Self {
        Self {
            code: HotkeyErrorCode::SettingsUnavailable,
            message: "App settings could not be loaded or saved.".to_string(),
        }
    }
}

impl From<HotkeyError> for SettingsError {
    fn from(_error: HotkeyError) -> Self {
        SettingsError {
            code: crate::settings::SettingsErrorCode::InvalidAppSettings,
            message: "Enter a valid hotkey.".to_string(),
        }
    }
}

pub fn validate_app_hotkey_settings(settings: AppSettings) -> Result<AppSettings, SettingsError> {
    let hotkey = normalize_hotkey_settings(settings.hotkey).map_err(SettingsError::from)?;

    Ok(AppSettings { hotkey })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        default_hotkey_for_os, normalize_hotkey_settings_for_os, HotkeyError, HotkeyErrorCode,
        HotkeyManager, HotkeyRegistrar, HotkeySettings,
    };
    use crate::settings::{AppSettings, SecretStore, SettingsError, SettingsManager};

    #[derive(Default)]
    struct FakeRegistrar {
        registered: Vec<String>,
        failed: HashSet<String>,
    }

    impl HotkeyRegistrar for FakeRegistrar {
        fn register(&mut self, hotkey: &HotkeySettings) -> Result<(), HotkeyError> {
            if self.failed.contains(&hotkey.accelerator) {
                return Err(HotkeyError {
                    code: HotkeyErrorCode::AlreadyInUse,
                    message: "This shortcut is already in use.".to_string(),
                });
            }

            if self.registered.contains(&hotkey.accelerator) {
                return Err(HotkeyError {
                    code: HotkeyErrorCode::AlreadyInUse,
                    message: "This shortcut is already in use.".to_string(),
                });
            }

            self.registered.push(hotkey.accelerator.clone());
            Ok(())
        }

        fn unregister(&mut self, hotkey: &HotkeySettings) -> Result<(), HotkeyError> {
            self.registered
                .retain(|registered| registered != &hotkey.accelerator);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemorySecretStore;

    impl SecretStore for MemorySecretStore {
        fn save(&self, _secret: &str) -> Result<(), SettingsError> {
            Ok(())
        }

        fn get(&self) -> Result<Option<String>, SettingsError> {
            Ok(None)
        }

        fn clear(&self) -> Result<(), SettingsError> {
            Ok(())
        }
    }

    fn normalize_for_test(accelerator: &str) -> Result<HotkeySettings, HotkeyError> {
        normalize_hotkey_settings_for_os(
            HotkeySettings {
                accelerator: accelerator.to_string(),
                label: String::new(),
            },
            "linux",
        )
    }

    fn normalize_for_macos(accelerator: &str) -> Result<HotkeySettings, HotkeyError> {
        normalize_hotkey_settings_for_os(
            HotkeySettings {
                accelerator: accelerator.to_string(),
                label: String::new(),
            },
            "macos",
        )
    }

    fn current_default() -> HotkeySettings {
        default_hotkey_for_os(std::env::consts::OS)
    }

    #[test]
    fn default_hotkey_selection_matches_platforms() {
        assert_eq!(default_hotkey_for_os("macos").accelerator, "Alt+Space");
        assert_eq!(default_hotkey_for_os("macos").label, "Option + Space");
        assert_eq!(
            default_hotkey_for_os("windows").accelerator,
            "Control+Space"
        );
        assert_eq!(default_hotkey_for_os("windows").label, "Ctrl + Space");
        assert_eq!(default_hotkey_for_os("linux").accelerator, "Control+Space");
        assert_eq!(default_hotkey_for_os("linux").label, "Ctrl + Space");
    }

    #[test]
    fn hotkey_parsing_normalizes_label_and_accelerator() {
        let hotkey = normalize_for_test("  Ctrl + Shift + A  ").unwrap();

        assert_eq!(hotkey.accelerator, "Control+Shift+KeyA");
        assert_eq!(hotkey.label, "Ctrl + Shift + A");
    }

    #[test]
    fn macos_label_uses_option_for_alt() {
        let hotkey = normalize_for_macos("Alt+Space").unwrap();

        assert_eq!(hotkey.accelerator, "Alt+Space");
        assert_eq!(hotkey.label, "Option + Space");
    }

    #[test]
    fn single_modifier_shortcuts_are_valid() {
        let windows = normalize_for_test("Control+Space").unwrap();
        assert_eq!(windows.accelerator, "Control+Space");
        assert_eq!(windows.label, "Ctrl + Space");

        let macos = normalize_for_macos("Alt+Space").unwrap();
        assert_eq!(macos.accelerator, "Alt+Space");
        assert_eq!(macos.label, "Option + Space");

        let combo = normalize_for_test("Control+Alt+KeyK").unwrap();
        assert_eq!(combo.accelerator, "Control+Alt+KeyK");
        assert_eq!(combo.label, "Ctrl + Alt + K");

        let shifted = normalize_for_test("Control+Shift+KeyB").unwrap();
        assert_eq!(shifted.accelerator, "Control+Shift+KeyB");
        assert_eq!(shifted.label, "Ctrl + Shift + B");
    }

    #[test]
    fn modifier_only_and_plain_shortcuts_are_invalid() {
        for accelerator in [
            "",
            "  ",
            "Control",
            "Shift",
            "Alt",
            "Super",
            "Control+Shift",
            "A",
            "Space",
            "Control+",
            "+++",
        ] {
            let error = normalize_for_test(accelerator).expect_err("hotkey should fail validation");

            assert!(
                matches!(
                    error.code,
                    HotkeyErrorCode::InvalidHotkey | HotkeyErrorCode::UnsupportedHotkey
                ),
                "unexpected error for {accelerator:?}: {error:?}"
            );
        }
    }

    #[test]
    fn control_space_is_not_treated_as_unsafe() {
        let hotkey = normalize_for_test("Control+Space").unwrap();
        assert_eq!(hotkey.accelerator, "Control+Space");
        assert_eq!(hotkey.label, "Ctrl + Space");

        let manager = HotkeyManager::default();
        let mut registrar = FakeRegistrar::default();
        manager
            .register_hotkey(&mut registrar, hotkey)
            .expect("Control+Space should register");
        assert!(registrar.registered.iter().any(|a| a == "Control+Space"));
    }

    #[test]
    fn status_reports_configured_default_and_registration_state() {
        let manager = HotkeyManager::default();
        let settings = test_settings_manager();
        let mut registrar = FakeRegistrar::default();

        manager
            .register_hotkey(&mut registrar, current_default())
            .unwrap();
        let status = manager.get_hotkey_settings(&settings).unwrap();

        assert_eq!(status.accelerator, current_default().accelerator);
        assert_eq!(status.label, current_default().label);
        assert!(status.is_default);
        assert!(status.is_registered);
        assert_eq!(status.error, None);
    }

    #[test]
    fn changing_hotkey_persists_and_registers_new_value() {
        let manager = HotkeyManager::default();
        let settings = test_settings_manager();
        let mut registrar = FakeRegistrar::default();

        manager
            .register_hotkey(&mut registrar, current_default())
            .unwrap();
        let status = manager
            .set_hotkey(&settings, &mut registrar, "Control+Alt+KeyA".to_string())
            .unwrap();

        assert_eq!(status.accelerator, "Control+Alt+KeyA");
        assert_eq!(status.label, "Ctrl + Alt + A");
        assert!(!status.is_default);
        assert!(status.is_registered);
        assert_eq!(
            settings.get_app_settings().unwrap().hotkey.label,
            "Ctrl + Alt + A"
        );
    }

    #[test]
    fn reset_hotkey_persists_and_registers_default() {
        let manager = HotkeyManager::default();
        let settings = test_settings_manager();
        let mut registrar = FakeRegistrar::default();

        manager
            .set_hotkey(&settings, &mut registrar, "Control+Alt+KeyA".to_string())
            .unwrap();
        let status = manager
            .reset_hotkey_to_default(&settings, &mut registrar)
            .unwrap();

        assert_eq!(status.accelerator, current_default().accelerator);
        assert_eq!(status.label, current_default().label);
        assert!(status.is_default);
        assert!(status.is_registered);
        assert_eq!(
            settings.get_app_settings().unwrap().hotkey,
            current_default()
        );
    }

    #[test]
    fn startup_reports_unavailable_when_saved_registration_fails() {
        let manager = HotkeyManager::default();
        let configured = normalize_for_test("Control+Alt+KeyA").unwrap();
        let mut registrar = FakeRegistrar::default();
        registrar.failed.insert(configured.accelerator.clone());

        let status = manager.register_or_fallback(&mut registrar, configured.clone());

        assert_eq!(status.accelerator, configured.accelerator);
        assert_eq!(status.label, configured.label);
        assert!(!status.is_default);
        assert!(!status.is_registered);
        assert_eq!(status.error.as_deref(), Some("Hotkey unavailable"));
    }

    #[test]
    fn previous_hotkey_is_restored_when_new_registration_fails() {
        let manager = HotkeyManager::default();
        let settings = test_settings_manager();
        let mut registrar = FakeRegistrar::default();
        let default_hotkey = current_default();
        let failing_hotkey = normalize_for_test("Control+Alt+KeyA").unwrap();
        registrar.failed.insert(failing_hotkey.accelerator.clone());

        manager
            .register_hotkey(&mut registrar, default_hotkey.clone())
            .unwrap();
        let error = manager
            .set_hotkey(
                &settings,
                &mut registrar,
                failing_hotkey.accelerator.clone(),
            )
            .expect_err("registration should fail");
        let status = manager.get_hotkey_settings(&settings).unwrap();

        assert_eq!(error.code, HotkeyErrorCode::AlreadyInUse);
        assert_eq!(status.accelerator, default_hotkey.accelerator);
        assert_eq!(status.label, default_hotkey.label);
        assert!(status.is_registered);
        assert_eq!(
            settings.get_app_settings().unwrap().hotkey,
            current_default()
        );
    }

    fn test_settings_manager() -> SettingsManager {
        SettingsManager::with_secret_store(
            Box::<MemorySecretStore>::default(),
            unique_settings_path(),
        )
    }

    fn unique_settings_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "floe-hotkey-test-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn settings_persistence_keeps_normalized_hotkey() {
        let settings = test_settings_manager();

        settings
            .save_app_settings(AppSettings {
                hotkey: HotkeySettings {
                    accelerator: "Control+Shift+B".to_string(),
                    label: String::new(),
                },
            })
            .unwrap();

        let saved = settings.get_app_settings().unwrap();

        assert_eq!(saved.hotkey.accelerator, "Control+Shift+KeyB");
        assert_eq!(saved.hotkey.label, "Ctrl + Shift + B");
    }
}
