mod commands;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_status,
            commands::app::get_settings_stub,
            commands::app::run_manual_test_stub,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Floe");
}
