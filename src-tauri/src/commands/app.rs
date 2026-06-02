use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    app_name: &'static str,
    status: &'static str,
    message: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsStub {
    has_groq_api_key: bool,
    hotkey_label: &'static str,
    storage_label: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualTestResult {
    action: String,
    message: String,
}

#[tauri::command]
pub fn get_app_status() -> AppStatus {
    AppStatus {
        app_name: "Floe",
        status: "setup_only",
        message:
            "Initial scaffold is ready. Runtime transcription features are not implemented yet.",
    }
}

#[tauri::command]
pub fn get_settings_stub() -> SettingsStub {
    SettingsStub {
        has_groq_api_key: false,
        hotkey_label: "Not configured",
        storage_label: "Keychain storage not wired yet",
    }
}

#[tauri::command]
pub fn run_manual_test_stub(action: String) -> ManualTestResult {
    ManualTestResult {
        message: format!("{action} is a placeholder for a future implementation task."),
        action,
    }
}

#[cfg(test)]
mod tests {
    use super::{get_app_status, get_settings_stub, run_manual_test_stub};

    #[test]
    fn status_is_setup_only() {
        let status = get_app_status();

        assert_eq!(status.app_name, "Floe");
        assert_eq!(status.status, "setup_only");
    }

    #[test]
    fn settings_do_not_claim_secret_storage() {
        let settings = get_settings_stub();

        assert!(!settings.has_groq_api_key);
        assert_eq!(settings.hotkey_label, "Not configured");
    }

    #[test]
    fn manual_test_reports_stubbed_action() {
        let result = run_manual_test_stub("recording".to_string());

        assert_eq!(result.action, "recording");
        assert!(result.message.contains("placeholder"));
    }
}
