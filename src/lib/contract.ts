// ─────────────────────────────────────────────────────────────────────────────
// FLOE Frontend-Backend Contract
//
// This file is the TypeScript mirror of src-tauri/src/contract.rs.
// Every constant here MUST match its Rust counterpart exactly.
//
// When changing either side, update the other AND run the contract tests:
//   cargo test -p floe contract  (Rust side)
//   npx vitest run src/lib/contract.test.ts  (TypeScript side)
// ─────────────────────────────────────────────────────────────────────────────

// ── Event names ─────────────────────────────────────────────────────────────

/// Emitted on every audio level sample (~33ms interval) during recording.
export const EVENT_RECORDING_LEVEL = "recording-level";
/// Emitted when recording state transitions (idle/starting/recording/stopping).
export const EVENT_RECORDING_STATE_CHANGED = "recording-state-changed";
/// Emitted by the global shortcut plugin on press/release of the hotkey.
export const EVENT_HOTKEY_STATE = "floe-global-hotkey-state";
/// Emitted to the overlay (bubble) window when recording state toggles.
export const EVENT_BUBBLE_STATE = "recording-bubble-state";
/// Emitted from tray "Settings" menu item to switch frontend to settings view.
export const EVENT_SHOW_SETTINGS = "floe-show-settings";
/// Emitted when the app begins its shutdown sequence.
export const EVENT_SHUTTING_DOWN = "floe-app-shutting-down";

// ── Recording constants ─────────────────────────────────────────────────────

export const MAX_RECORDING_DURATION_SECS = 120;
export const WATCHDOG_GRACE_SECS = 5;
export const LEVEL_EMIT_INTERVAL_MS = 33;

// ── Audio constants ─────────────────────────────────────────────────────────

export const TARGET_WAV_SAMPLE_RATE = 16_000;
export const OUTPUT_CHANNELS = 1;
export const WAV_BITS_PER_SAMPLE = 16;

// ── Tauri command names ─────────────────────────────────────────────────────
// Every string must match the #[tauri::command] fn name in the Rust backend.
// The `contract.test.ts` file verifies these match the Rust side.

export const CMD_SAVE_API_KEY = "save_api_key";
export const CMD_CLEAR_API_KEY = "clear_api_key";
export const CMD_GET_API_KEY_STATUS = "get_api_key_status";
export const CMD_GET_APP_SETTINGS = "get_app_settings";
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
export const CMD_GET_RECORDING_STATUS = "get_recording_status";
export const CMD_GET_LATEST_RECORDING_INFO = "get_latest_recording_info";
export const CMD_TRANSCRIBE_LATEST_RECORDING = "transcribe_latest_recording";
export const CMD_CLEANUP_TRANSCRIPT = "cleanup_transcript";
export const CMD_COPY_TEXT_TO_CLIPBOARD = "copy_text_to_clipboard";
export const CMD_PASTE_TEXT = "paste_text";
export const CMD_PASTE_CLIPBOARD = "paste_clipboard";
export const CMD_BUBBLE_SHOW = "bubble_show";
export const CMD_BUBBLE_HIDE = "bubble_hide";
export const CMD_DIAG_LOG = "diag_log";
export const CMD_DIAG_LOG_STR = "diag_log_str";
export const CMD_GET_RECENT_TRACES = "get_recent_traces";
export const CMD_GET_CURRENT_TRACE = "get_current_trace";

/// All registered commands — used in tests.
export const ALL_COMMANDS: readonly string[] = [
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
] as const;

// ── Typed event payloads ────────────────────────────────────────────────────

/** Payload for `EVENT_RECORDING_LEVEL`. */
export interface RecordingLevelPayload {
  level: number;
}

/** Payload for `EVENT_RECORDING_STATE_CHANGED`. */
export interface RecordingStateChangedPayload {
  state: "idle" | "starting" | "recording" | "stopping";
  isRecording: boolean;
}

/** Payload for `EVENT_HOTKEY_STATE`. */
export interface HotkeyStatePayload {
  state: "Pressed" | "Released";
}

/** Payload for `EVENT_BUBBLE_STATE` (emitted to the overlay window). */
export interface BubbleStatePayload {
  recording: boolean;
}

// ── Type-safe event listener helpers ────────────────────────────────────────

import { listen, UnlistenFn } from "@tauri-apps/api/event";

/**
 * Type-safe shorthand for listening to a contract-defined event.
 *
 * ```ts
 * const unlisten = await listenRecordingLevel((payload) => {
 *   console.log(payload.level);
 * });
 * ```
 */
export async function listenHotkeyState(
  handler: (payload: HotkeyStatePayload) => void,
): Promise<UnlistenFn> {
  return listen<HotkeyStatePayload>(EVENT_HOTKEY_STATE, (event) =>
    handler(event.payload),
  );
}

export async function listenRecordingStateChanged(
  handler: (payload: RecordingStateChangedPayload) => void,
): Promise<UnlistenFn> {
  return listen<RecordingStateChangedPayload>(
    EVENT_RECORDING_STATE_CHANGED,
    (event) => handler(event.payload),
  );
}

export async function listenRecordingLevel(
  handler: (payload: RecordingLevelPayload) => void,
): Promise<UnlistenFn> {
  return listen<RecordingLevelPayload>(EVENT_RECORDING_LEVEL, (event) =>
    handler(event.payload),
  );
}

export async function listenShowSettings(
  handler: () => void,
): Promise<UnlistenFn> {
  return listen(EVENT_SHOW_SETTINGS, handler);
}

export async function listenShuttingDown(
  handler: () => void,
): Promise<UnlistenFn> {
  return listen(EVENT_SHUTTING_DOWN, handler);
}
