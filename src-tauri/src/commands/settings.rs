use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait};
use tauri::{AppHandle, Runtime, State};

use crate::{
    settings::{ApiKeyStatus, AppSettings, SettingsError, SettingsManager},
    system::autostart::{
        get_start_at_login_status_with, set_start_at_login_enabled_with, StartAtLoginError,
        StartAtLoginStatus, TauriAutostartIntegration,
    },
};

#[tauri::command]
pub fn save_api_key(
    manager: State<'_, SettingsManager>,
    api_key: String,
) -> Result<ApiKeyStatus, SettingsError> {
    manager.save_api_key(api_key)
}

#[tauri::command]
pub fn clear_api_key(manager: State<'_, SettingsManager>) -> Result<ApiKeyStatus, SettingsError> {
    manager.clear_api_key()
}

#[tauri::command]
pub fn get_api_key_status(
    manager: State<'_, SettingsManager>,
) -> Result<ApiKeyStatus, SettingsError> {
    manager.get_api_key_status()
}

#[tauri::command]
pub fn get_app_settings(manager: State<'_, SettingsManager>) -> Result<AppSettings, SettingsError> {
    manager.get_app_settings()
}

#[derive(serde::Serialize)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub fn get_audio_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    let host = cpal::default_host();

    // Gracefully handle enumeration failure — return empty vec with a warning
    // instead of propagating the error to the frontend.
    let devices = match host.input_devices() {
        Ok(devices) => devices,
        Err(e) => {
            log::warn!("Failed to enumerate input devices: {}", e);
            return Ok(Vec::new());
        }
    };

    let mut device_list = Vec::new();
    for device in devices {
        // Try to get the human-readable device name; skip on failure.
        let name = match device.description() {
            Ok(desc) => desc.name().to_string(),
            Err(e) => {
                log::warn!("Failed to get device description, skipping: {}", e);
                continue;
            }
        };

        // Try to get the stable device ID; skip on failure.
        let id = match device.id() {
            Ok(id) => id.to_string(),
            Err(e) => {
                log::warn!("Failed to get device ID, skipping: {}", e);
                continue;
            }
        };

        device_list.push(AudioDeviceInfo { id, name });
    }

    Ok(device_list)
}

#[tauri::command]
pub async fn validate_api_key(api_key: String) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let response = client
        .get("https://api.groq.com/openai/v1/models")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    match response.status() {
        reqwest::StatusCode::OK => Ok(true),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => Ok(false),
        status => Err(format!("Unexpected API response: {status}")),
    }
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
