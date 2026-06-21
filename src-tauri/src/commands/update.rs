//! Tauri commands for the updater plugin integration.

use tauri::AppHandle;

use crate::update::{UpdateError, UpdateInfo, UpdateStatusLabel};
use tauri_plugin_updater::UpdaterExt;

/// Check for updates, download with progress, and install.
/// On success, the app automatically restarts.
#[tauri::command]
pub async fn check_and_install_update(app: AppHandle) -> Result<UpdateInfo, UpdateError> {
    let updater = app.updater().map_err(|e| UpdateError::internal(e.to_string()))?;

    // Check for update
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => {
            return Ok(UpdateInfo {
                current_version: env!("CARGO_PKG_VERSION").to_string(),
                latest_version: None,
                status: UpdateStatusLabel::NoUpdate,
                download_progress: 0.0,
                last_check_result: Some("You're up to date".into()),
                error_message: None,
            });
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    let version = update.version.clone();

    // Download and install with progress callback
    match update
        .download_and_install(
            |chunk, total| {
                let _ = chunk;
                let _ = total;
                // Progress tracking available here if needed for frontend events
            },
            || {
                log::info!("update_download_complete version={}", version);
            },
        )
        .await
    {
        Ok(()) => {
            log::info!("update_installed version={}", version);
            Ok(UpdateInfo {
                current_version: env!("CARGO_PKG_VERSION").to_string(),
                latest_version: Some(version),
                status: UpdateStatusLabel::Ready,
                download_progress: 100.0,
                last_check_result: Some("Update installed, restarting...".into()),
                error_message: None,
            })
        }
        Err(e) => {
            log::error!("update_install_failed error={:?}", e);
            Err(e.into())
        }
    }
}
