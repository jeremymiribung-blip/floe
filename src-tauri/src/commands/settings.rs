use tauri::{AppHandle, Runtime, State};

use crate::{
    settings::{AppSettings, GroqApiKeyStatus, SettingsError, SettingsManager},
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
