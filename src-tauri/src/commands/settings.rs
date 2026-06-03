use tauri::State;

use crate::settings::{
    AppSettings, CerebrasApiKeyStatus, CleanupMode, GroqApiKeyStatus, SettingsError,
    SettingsManager,
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
