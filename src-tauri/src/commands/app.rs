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
pub struct ManualTestResult {
    action: String,
    message: String,
}

#[tauri::command]
pub fn get_app_status() -> AppStatus {
    AppStatus {
        app_name: "Floe",
        status: "setup_only",
        message: "Manual recording, transcription, clipboard copy, and paste checks are ready.",
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
    use super::{get_app_status, run_manual_test_stub};

    #[test]
    fn status_is_setup_only() {
        let status = get_app_status();

        assert_eq!(status.app_name, "Floe");
        assert_eq!(status.status, "setup_only");
    }

    #[test]
    fn manual_test_reports_stubbed_action() {
        let result = run_manual_test_stub("recording".to_string());

        assert_eq!(result.action, "recording");
        assert!(result.message.contains("placeholder"));
    }
}
