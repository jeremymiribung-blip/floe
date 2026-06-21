// ─────────────────────────────────────────────────────────────────────────────
// FLOE Frontend-Backend Contract
//
// This file is the TypeScript mirror of src-tauri/src/contract.rs.
// Every constant here MUST match its Rust counterpart exactly.
// ─────────────────────────────────────────────────────────────────────────────

// ── Event names ─────────────────────────────────────────────────────────────

// ── Event payload types ──────────────────────────────────────────────────────

/// Payload for EVENT_RECORDING_LEVEL.
export interface RecordingLevelPayload {
  level: number;
}

/// Payload for EVENT_BUBBLE_STATE.
export interface BubbleStatePayload {
  bubbleState: string;
}

// ── Event names ─────────────────────────────────────────────────────────────

/// Emitted on every audio level sample (~33ms interval) during recording.
export const EVENT_RECORDING_LEVEL = "recording-level";

/// Emitted when recording state transitions (idle/starting/recording/stopping).
export const EVENT_RECORDING_STATE_CHANGED = "recording-state-changed";

/// Emitted by the global shortcut plugin on press/release of the hotkey.
export const EVENT_HOTKEY_STATE = "floe-global-hotkey-state";

/// Emitted to the overlay (bubble) window when recording state toggles.
export const EVENT_BUBBLE_STATE = "recording-bubble-state";

/// Emitted to the main window when the user cancels recording from the bubble overlay.
export const EVENT_BUBBLE_CANCEL = "recording-bubble-cancelled";

/// Emitted from tray "Settings" menu item to switch frontend to settings view.
export const EVENT_SHOW_SETTINGS = "floe-show-settings";

/// Emitted when the app begins its shutdown sequence.
export const EVENT_SHUTTING_DOWN = "floe-app-shutting-down";

/// Emitted when an update has been installed and the app will restart.
export const EVENT_UPDATE_INSTALLED = "floe-update-installed";

// ── Overlay (bubble) window ─────────────────────────────────────────────────

/// Label of the Tauri webview window used as the recording bubble overlay.
export const BUBBLE_WINDOW_LABEL = "recording-bubble";

// ── Recording constants ─────────────────────────────────────────────────────

export const MAX_RECORDING_DURATION_SECS = 120;
export const WATCHDOG_GRACE_SECS = 5;
export const LEVEL_EMIT_INTERVAL_MS = 33;

// ── Audio constants ─────────────────────────────────────────────────────────

export const TARGET_WAV_SAMPLE_RATE = 16_000;
export const OUTPUT_CHANNELS = 1;
export const WAV_BITS_PER_SAMPLE = 16;

// ── Tauri command names ─────────────────────────────────────────────────────

export const CMD_SAVE_API_KEY = "save_api_key";
export const CMD_VALIDATE_API_KEY = "validate_api_key";
export const CMD_CLEAR_API_KEY = "clear_api_key";
export const CMD_GET_API_KEY_STATUS = "get_api_key_status";
export const CMD_GET_APP_SETTINGS = "get_app_settings";
export const CMD_GET_AUDIO_DEVICES = "get_audio_devices";
export const CMD_SAVE_APP_SETTINGS = "save_app_settings";
export const CMD_GET_START_AT_LOGIN_STATUS = "get_start_at_login_status";
export const CMD_SET_START_AT_LOGIN_ENABLED = "set_start_at_login_enabled";
export const CMD_GET_HOTKEY_SETTINGS = "get_hotkey_settings";
export const CMD_SET_HOTKEY = "set_hotkey";
export const CMD_RESET_HOTKEY_TO_DEFAULT = "reset_hotkey_to_default";
export const CMD_REGISTER_GLOBAL_HOTKEY = "register_global_hotkey";
export const CMD_UNREGISTER_GLOBAL_HOTKEY = "unregister_global_hotkey";
export const CMD_START_RECORDING = "start_recording";
export const CMD_STOP_RECORDING = "stop_recording";
export const CMD_FORCE_STOP_RECORDING = "force_stop_recording";
export const CMD_GET_RECORDING_STATUS = "get_recording_status";
export const CMD_GET_LATEST_RECORDING_INFO = "get_latest_recording_info";
export const CMD_TRANSCRIBE_LATEST_RECORDING = "transcribe_latest_recording";
export const CMD_CLEANUP_TRANSCRIPT = "cleanup_transcript";
export const CMD_COPY_TEXT_TO_CLIPBOARD = "copy_text_to_clipboard";
export const CMD_PASTE_TEXT = "paste_text";
export const CMD_PASTE_CLIPBOARD = "paste_clipboard";
export const CMD_BUBBLE_HIDE = "bubble_hide";
export const CMD_BUBBLE_CANCEL_RECORDING = "bubble_cancel_recording";
export const CMD_BUBBLE_SET_STATE = "bubble_set_state";
export const CMD_BUBBLE_SHOW = "bubble_show";
export const CMD_DIAG_LOG = "diag_log";
export const CMD_DIAG_LOG_STR = "diag_log_str";
export const CMD_LOG_FRONTEND_EVENT = "log_frontend_event";
export const CMD_GET_DIAGNOSTICS_REPORT = "get_diagnostics_report";
export const CMD_GET_RECENT_TRACES = "get_recent_traces";
export const CMD_GET_CURRENT_TRACE = "get_current_trace";
export const CMD_UPDATE_SESSION_HOTKEY_LATENCY =
  "update_session_hotkey_latency";
export const CMD_GET_UPDATE_INFO = "get_update_info";
export const CMD_CHECK_FOR_UPDATE = "check_for_update";
export const CMD_DOWNLOAD_UPDATE = "download_update";
export const CMD_INSTALL_UPDATE = "install_update";
export const CMD_RESET_UPDATE_STATE = "reset_update_state";
export const CMD_CHECK_AND_INSTALL_UPDATE = "check_and_install_update";
