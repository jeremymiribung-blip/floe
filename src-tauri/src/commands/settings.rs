use tauri::{AppHandle, Runtime, State};

use crate::{
    settings::{
        AppSettings, CerebrasApiKeyStatus, CleanupMode, GroqApiKeyStatus, SettingsError,
        SettingsManager,
    },
    system::autostart::{
        get_start_at_login_status_with, set_start_at_login_enabled_with, StartAtLoginError,
        StartAtLoginStatus, TauriAutostartIntegration,
    },
};

#[tauri::command]
pub fn save_groq_api_key(
    manager: State<'_, SettingsManager>,
    api_key: String,
) -> Result<GroqApiKeyStatus, SettingsError> {
    manager.save_groq_api_key(api_key)
}

#[tauri::command]
pub fn clear_groq_api_key(
    manager: State<'_, SettingsManager>,
) -> Result<GroqApiKeyStatus, SettingsError> {
    manager.clear_groq_api_key()
}

#[tauri::command]
pub fn get_groq_api_key_status(
    manager: State<'_, SettingsManager>,
) -> Result<GroqApiKeyStatus, SettingsError> {
    manager.get_groq_api_key_status()
}

#[tauri::command]
pub fn save_cerebras_api_key(
    manager: State<'_, SettingsManager>,
    api_key: String,
) -> Result<CerebrasApiKeyStatus, SettingsError> {
    manager.save_cerebras_api_key(api_key)
}

#[tauri::command]
pub fn clear_cerebras_api_key(
    manager: State<'_, SettingsManager>,
) -> Result<CerebrasApiKeyStatus, SettingsError> {
    manager.clear_cerebras_api_key()
}

#[tauri::command]
pub fn get_cerebras_api_key_status(
    manager: State<'_, SettingsManager>,
) -> Result<CerebrasApiKeyStatus, SettingsError> {
    manager.get_cerebras_api_key_status()
}

#[tauri::command]
pub fn get_app_settings(manager: State<'_, SettingsManager>) -> Result<AppSettings, SettingsError> {
    manager.get_app_settings()
}

#[tauri::command]
pub fn save_app_settings(
    manager: State<'_, SettingsManager>,
    settings: AppSettings,
) -> Result<AppSettings, SettingsError> {
    manager.save_app_settings(settings)
}

#[tauri::command]
pub fn get_cleanup_mode(manager: State<'_, SettingsManager>) -> Result<CleanupMode, SettingsError> {
    manager.get_cleanup_mode()
}

#[tauri::command]
pub fn set_cleanup_mode(
    manager: State<'_, SettingsManager>,
    cleanup_mode: CleanupMode,
) -> Result<CleanupMode, SettingsError> {
    manager.set_cleanup_mode(cleanup_mode)
}

#[tauri::command]
pub fn get_start_at_login_status<R: Runtime>(
    app: AppHandle<R>,
) -> Result<StartAtLoginStatus, StartAtLoginError> {
    let integration = TauriAutostartIntegration::new(&app);

    get_start_at_login_status_with(&integration)
}

#[tauri::command]
pub fn set_start_at_login_enabled<R: Runtime>(
    app: AppHandle<R>,
    enabled: bool,
) -> Result<StartAtLoginStatus, StartAtLoginError> {
    let integration = TauriAutostartIntegration::new(&app);

    set_start_at_login_enabled_with(&integration, enabled)
}
