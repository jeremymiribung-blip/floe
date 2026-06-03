use tauri::{AppHandle, Runtime, State};

use crate::{
    settings::SettingsManager,
    system::hotkey::{HotkeyError, HotkeyManager, HotkeyStatus, TauriHotkeyRegistrar},
};

#[tauri::command]
pub fn get_hotkey_settings(
    settings_manager: State<'_, SettingsManager>,
    hotkey_manager: State<'_, HotkeyManager>,
) -> Result<HotkeyStatus, HotkeyError> {
    hotkey_manager.get_hotkey_settings(&settings_manager)
}

#[tauri::command]
pub fn set_hotkey<R: Runtime>(
    app: AppHandle<R>,
    settings_manager: State<'_, SettingsManager>,
    hotkey_manager: State<'_, HotkeyManager>,
    accelerator: String,
) -> Result<HotkeyStatus, HotkeyError> {
    let mut registrar = TauriHotkeyRegistrar::new(&app);
    hotkey_manager.set_hotkey(&settings_manager, &mut registrar, accelerator)
}

#[tauri::command]
pub fn reset_hotkey_to_default<R: Runtime>(
    app: AppHandle<R>,
    settings_manager: State<'_, SettingsManager>,
    hotkey_manager: State<'_, HotkeyManager>,
) -> Result<HotkeyStatus, HotkeyError> {
    let mut registrar = TauriHotkeyRegistrar::new(&app);
    hotkey_manager.reset_hotkey_to_default(&settings_manager, &mut registrar)
}

#[tauri::command]
pub fn register_global_hotkey<R: Runtime>(
    app: AppHandle<R>,
    settings_manager: State<'_, SettingsManager>,
    hotkey_manager: State<'_, HotkeyManager>,
) -> Result<HotkeyStatus, HotkeyError> {
    let configured = settings_manager
        .get_app_settings()
        .map_err(HotkeyError::from_settings)?
        .hotkey;
    let mut registrar = TauriHotkeyRegistrar::new(&app);

    hotkey_manager.register_hotkey(&mut registrar, configured)?;
    hotkey_manager.get_hotkey_settings(&settings_manager)
}

#[tauri::command]
pub fn unregister_global_hotkey<R: Runtime>(
    app: AppHandle<R>,
    settings_manager: State<'_, SettingsManager>,
    hotkey_manager: State<'_, HotkeyManager>,
) -> Result<HotkeyStatus, HotkeyError> {
    let mut registrar = TauriHotkeyRegistrar::new(&app);

    hotkey_manager.unregister_hotkey(&mut registrar)?;
    hotkey_manager.get_hotkey_settings(&settings_manager)
}
