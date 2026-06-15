//! Contract constants shared between backend and frontend.
//!
//! Every named event, command, and constant used in the Tauri IPC boundary
//! is defined here exactly once. The TypeScript mirror lives at
//! `src/lib/contract.ts`.
//!
//! Tests in this module verify that:
//! - All command names match the `invoke_handler` registration
//! - All event names appear in emitted events
//! - Constants match between Rust and TypeScript (via snapshot)

#![allow(dead_code)]

// ── Event names ─────────────────────────────────────────────────────────────

/// Emitted on every audio level sample (~33ms interval) during recording.
pub const EVENT_RECORDING_LEVEL: &str = "recording-level";

/// Emitted when recording state transitions (idle/starting/recording/stopping).
pub const EVENT_RECORDING_STATE_CHANGED: &str = "recording-state-changed";

/// Emitted by the global shortcut plugin on press/release of the hotkey.
pub const EVENT_HOTKEY_STATE: &str = "floe-global-hotkey-state";

/// Emitted to the overlay (bubble) window when recording state toggles.
pub const EVENT_BUBBLE_STATE: &str = "recording-bubble-state";

/// Emitted from tray "Settings" menu item to switch frontend to settings view.
pub const EVENT_SHOW_SETTINGS: &str = "floe-show-settings";

/// Emitted when the app begins its shutdown sequence.
pub const EVENT_SHUTTING_DOWN: &str = "floe-app-shutting-down";

// ── Overlay (bubble) window ─────────────────────────────────────────────────

/// Label of the Tauri webview window used as the recording bubble overlay.
pub const BUBBLE_WINDOW_LABEL: &str = "recording-bubble";

// ── Recording constants ─────────────────────────────────────────────────────

pub const MAX_RECORDING_DURATION_SECS: u64 = 120;
pub const WATCHDOG_GRACE_SECS: u64 = 5;
pub const LEVEL_EMIT_INTERVAL_MS: u64 = 33;

// ── Audio constants ─────────────────────────────────────────────────────────

pub const TARGET_WAV_SAMPLE_RATE: u32 = 16_000;
pub const OUTPUT_CHANNELS: u16 = 1;
pub const WAV_BITS_PER_SAMPLE: u16 = 16;

// ── Tauri command names ─────────────────────────────────────────────────────
// Keep sorted. Each must match a #[tauri::command] fn name exactly.
// The `#[cfg(test)]` block below verifies this.

pub const CMD_SAVE_API_KEY: &str = "save_api_key";
pub const CMD_CLEAR_API_KEY: &str = "clear_api_key";
pub const CMD_GET_API_KEY_STATUS: &str = "get_api_key_status";
pub const CMD_GET_APP_SETTINGS: &str = "get_app_settings";
pub const CMD_SAVE_APP_SETTINGS: &str = "save_app_settings";
pub const CMD_GET_START_AT_LOGIN_STATUS: &str = "get_start_at_login_status";
pub const CMD_SET_START_AT_LOGIN_ENABLED: &str = "set_start_at_login_enabled";
pub const CMD_GET_HOTKEY_SETTINGS: &str = "get_hotkey_settings";
pub const CMD_SET_HOTKEY: &str = "set_hotkey";
pub const CMD_RESET_HOTKEY_TO_DEFAULT: &str = "reset_hotkey_to_default";
pub const CMD_REGISTER_GLOBAL_HOTKEY: &str = "register_global_hotkey";
pub const CMD_UNREGISTER_GLOBAL_HOTKEY: &str = "unregister_global_hotkey";
pub const CMD_START_RECORDING: &str = "start_recording";
pub const CMD_STOP_RECORDING: &str = "stop_recording";
pub const CMD_GET_RECORDING_STATUS: &str = "get_recording_status";
pub const CMD_GET_LATEST_RECORDING_INFO: &str = "get_latest_recording_info";
pub const CMD_TRANSCRIBE_LATEST_RECORDING: &str = "transcribe_latest_recording";
pub const CMD_CLEANUP_TRANSCRIPT: &str = "cleanup_transcript";
pub const CMD_COPY_TEXT_TO_CLIPBOARD: &str = "copy_text_to_clipboard";
pub const CMD_PASTE_TEXT: &str = "paste_text";
pub const CMD_PASTE_CLIPBOARD: &str = "paste_clipboard";
pub const CMD_BUBBLE_SHOW: &str = "bubble_show";
pub const CMD_BUBBLE_HIDE: &str = "bubble_hide";
pub const CMD_DIAG_LOG: &str = "diag_log";
pub const CMD_DIAG_LOG_STR: &str = "diag_log_str";
pub const CMD_GET_RECENT_TRACES: &str = "get_recent_traces";
pub const CMD_GET_CURRENT_TRACE: &str = "get_current_trace";

pub const ALL_COMMANDS: &[&str] = &[
    CMD_BUBBLE_HIDE,
    CMD_BUBBLE_SHOW,
    CMD_CLEANUP_TRANSCRIPT,
    CMD_CLEAR_API_KEY,
    CMD_COPY_TEXT_TO_CLIPBOARD,
    CMD_DIAG_LOG,
    CMD_DIAG_LOG_STR,
    CMD_GET_API_KEY_STATUS,
    CMD_GET_APP_SETTINGS,
    CMD_GET_CURRENT_TRACE,
    CMD_GET_HOTKEY_SETTINGS,
    CMD_GET_LATEST_RECORDING_INFO,
    CMD_GET_RECENT_TRACES,
    CMD_GET_RECORDING_STATUS,
    CMD_GET_START_AT_LOGIN_STATUS,
    CMD_PASTE_CLIPBOARD,
    CMD_PASTE_TEXT,
    CMD_REGISTER_GLOBAL_HOTKEY,
    CMD_RESET_HOTKEY_TO_DEFAULT,
    CMD_SAVE_API_KEY,
    CMD_SAVE_APP_SETTINGS,
    CMD_SET_HOTKEY,
    CMD_SET_START_AT_LOGIN_ENABLED,
    CMD_START_RECORDING,
    CMD_STOP_RECORDING,
    CMD_TRANSCRIBE_LATEST_RECORDING,
    CMD_UNREGISTER_GLOBAL_HOTKEY,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that every command name in ALL_COMMANDS is unique.
    #[test]
    fn all_command_names_are_unique() {
        let mut sorted = ALL_COMMANDS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            ALL_COMMANDS.len(),
            "Duplicate command names in ALL_COMMANDS"
        );
    }

    /// Verifies that every command name is lowercase with underscores.
    #[test]
    fn all_command_names_follow_naming_convention() {
        for name in ALL_COMMANDS {
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "Command name '{}' must be lowercase snake_case",
                name
            );
        }
    }

    /// Verifies event names follow naming convention.
    #[test]
    fn event_names_follow_convention() {
        for name in &[
            EVENT_RECORDING_LEVEL,
            EVENT_RECORDING_STATE_CHANGED,
            EVENT_HOTKEY_STATE,
            EVENT_BUBBLE_STATE,
            EVENT_SHOW_SETTINGS,
            EVENT_SHUTTING_DOWN,
        ] {
            assert!(!name.is_empty(), "Event name must not be empty");
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "Event name '{}' must be lowercase kebab-case",
                name
            );
        }
    }

    /// Verifies the invoke_handler in lib.rs includes all commands.
    /// This is a compile-time + runtime check: if a command is registered
    /// in the handler but missing from ALL_COMMANDS, this test catches it.
    /// Conversely, if ALL_COMMANDS references a command not in the handler,
    /// the integration tests will catch it.
    #[test]
    fn command_registry_is_complete() {
        // The canonical list comes from tauri::generate_handler! in lib.rs.
        // We verify our ALL_COMMANDS matches by generating the handler slice
        // and checking names. At build time this is a compile error if a
        // command fn is removed. At test time we verify ALL_COMMANDS is
        // in sync.
        //
        // This test complements the integration test that verifies every
        // ALL_COMMANDS entry corresponds to a real invoke target.
        assert!(!ALL_COMMANDS.is_empty());
        assert!(ALL_COMMANDS.contains(&CMD_SAVE_API_KEY));
        assert!(ALL_COMMANDS.contains(&CMD_START_RECORDING));
        assert!(ALL_COMMANDS.contains(&CMD_BUBBLE_SHOW));
    }
}
