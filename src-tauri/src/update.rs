//! Tauri v2 Updater Plugin integration for Floe.
//!
//! Provides a thin wrapper around the official tauri-plugin-updater with
//! clean error types for the frontend.

use serde::Serialize;

/// Application-level error returned to the frontend.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateError {
    pub domain: &'static str,
    pub code: UpdateErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum UpdateErrorCode {
    NetworkError,
    UpdateNotFound,
    DownloadFailed,
    InstallFailed,
    AlreadyUpToDate,
    Internal,
}

/// Serializable snapshot of the current update state exposed to the frontend.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub status: UpdateStatusLabel,
    pub download_progress: f64,
    pub last_check_result: Option<String>,
    pub error_message: Option<String>,
}

/// Serialization-friendly status label matching the frontend union type.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum UpdateStatusLabel {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "checking")]
    Checking,
    #[serde(rename = "available")]
    Available,
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "downloaded")]
    Downloaded,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "no_update")]
    NoUpdate,
    #[serde(rename = "error")]
    Error,
}

impl UpdateError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            domain: "update",
            code: UpdateErrorCode::Internal,
            message: message.into(),
        }
    }
}

impl From<tauri_plugin_updater::Error> for UpdateError {
    fn from(err: tauri_plugin_updater::Error) -> Self {
        match err {
            tauri_plugin_updater::Error::Network(e) => UpdateError {
                domain: "update",
                code: UpdateErrorCode::NetworkError,
                message: format!("Network error: {}", e),
            },
            tauri_plugin_updater::Error::ReleaseNotFound => UpdateError {
                domain: "update",
                code: UpdateErrorCode::UpdateNotFound,
                message: "No release found".into(),
            },
            _ => UpdateError {
                domain: "update",
                code: UpdateErrorCode::Internal,
                message: format!("Updater error: {}", err),
            },
        }
    }
}
