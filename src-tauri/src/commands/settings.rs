use tauri::State;

use crate::settings::{AppSettings, GroqApiKeyStatus, SettingsError, SettingsManager};

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
