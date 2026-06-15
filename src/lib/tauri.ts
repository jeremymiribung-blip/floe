// ─────────────────────────────────────────────────────────────────────────────
// Tauri IPC bridge
//
// This module provides the typed invocation layer for all backend commands.
// Browser-mode fallback is intentionally MINIMAL — only enough for UI layout
// testing. All business logic (hotkey normalization, recording state tracking,
// API key masking) lives in the Rust backend.
//
// When adding a new command:
//   1. Define the command name in contract.ts + contract.rs
//   2. Add the invoke wrapper here
//   3. Add the result type to types/app.ts
//   4. Add serialization shape tests in contract_tests.rs
// ─────────────────────────────────────────────────────────────────────────────

import { invoke } from "@tauri-apps/api/core";
import type {
  ApiKeyStatus,
  HotkeyStatus,
  RecordingInfo,
  RecordingStatus,
  SttResult,
  StartAtLoginStatus,
  TranscriptCleanupResult,
} from "../types/app";
import { parseFloeError } from "./errors";
import {
  CMD_SAVE_API_KEY,
  CMD_CLEAR_API_KEY,
  CMD_GET_API_KEY_STATUS,
  CMD_GET_HOTKEY_SETTINGS,
  CMD_SET_HOTKEY,
  CMD_RESET_HOTKEY_TO_DEFAULT,
  CMD_GET_START_AT_LOGIN_STATUS,
  CMD_SET_START_AT_LOGIN_ENABLED,
  CMD_START_RECORDING,
  CMD_STOP_RECORDING,
  CMD_GET_RECORDING_STATUS,
  CMD_TRANSCRIBE_LATEST_RECORDING,
  CMD_CLEANUP_TRANSCRIPT,
  CMD_COPY_TEXT_TO_CLIPBOARD,
  CMD_PASTE_CLIPBOARD,
  CMD_BUBBLE_SHOW,
  CMD_BUBBLE_HIDE,
  CMD_DIAG_LOG_STR,
} from "./contract";

/**
 * Returns `true` when running inside Tauri (desktop app).
 * Returns `false` when running in a plain browser (dev/test).
 */
export function isTauriRuntime(): boolean {
  return (
    typeof (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ !==
    "undefined"
  );
}

/**
 * Invoke a Tauri command with typed result. On error, parses the
 * caught value into a typed `FloeError` and throws it.
 */
async function invokeTyped<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (caught) {
    throw parseFloeError(caught);
  }
}

// ── API key commands ────────────────────────────────────────────────────────

export function saveApiKey(apiKey: string): Promise<ApiKeyStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("saveApiKey is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_SAVE_API_KEY, { apiKey });
}

export function clearApiKey(): Promise<ApiKeyStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("clearApiKey is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_CLEAR_API_KEY);
}

export function getApiKeyStatus(): Promise<ApiKeyStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("getApiKeyStatus is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_GET_API_KEY_STATUS);
}

// ── Hotkey commands ─────────────────────────────────────────────────────────

export function getHotkeySettings(): Promise<HotkeyStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("getHotkeySettings is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_GET_HOTKEY_SETTINGS);
}

export function setHotkey(accelerator: string): Promise<HotkeyStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("setHotkey is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_SET_HOTKEY, { accelerator });
}

export function resetHotkeyToDefault(): Promise<HotkeyStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("resetHotkeyToDefault is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_RESET_HOTKEY_TO_DEFAULT);
}

// ── Start-at-login commands ─────────────────────────────────────────────────

export function getStartAtLoginStatus(): Promise<StartAtLoginStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("getStartAtLoginStatus is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_GET_START_AT_LOGIN_STATUS);
}

export function setStartAtLoginEnabled(
  enabled: boolean,
): Promise<StartAtLoginStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error(
        "setStartAtLoginEnabled is only available in the Tauri runtime",
      ),
    );
  }
  return invokeTyped(CMD_SET_START_AT_LOGIN_ENABLED, { enabled });
}

// ── Recording commands ──────────────────────────────────────────────────────

export function startRecording(): Promise<RecordingStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("startRecording is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_START_RECORDING);
}

export function stopRecording(): Promise<RecordingInfo> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("stopRecording is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_STOP_RECORDING);
}

export function getRecordingStatus(): Promise<RecordingStatus> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("getRecordingStatus is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_GET_RECORDING_STATUS);
}

// ── Transcription commands ──────────────────────────────────────────────────

export function transcribeLatestRecording(): Promise<SttResult> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error(
        "transcribeLatestRecording is only available in the Tauri runtime",
      ),
    );
  }
  return invokeTyped(CMD_TRANSCRIBE_LATEST_RECORDING);
}

export function cleanupTranscript(
  transcript: string,
): Promise<TranscriptCleanupResult> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("cleanupTranscript is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_CLEANUP_TRANSCRIPT, { transcript });
}

// ── Clipboard commands ──────────────────────────────────────────────────────

export function copyTextToClipboard(text: string): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("copyTextToClipboard is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_COPY_TEXT_TO_CLIPBOARD, { text });
}

export function pasteClipboard(): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.reject(
      new Error("pasteClipboard is only available in the Tauri runtime"),
    );
  }
  return invokeTyped(CMD_PASTE_CLIPBOARD);
}

// ── Overlay/bubble commands ─────────────────────────────────────────────────

export function bubbleShow(): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.resolve();
  }
  return invokeTyped(CMD_BUBBLE_SHOW);
}

export function bubbleHide(): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.resolve();
  }
  return invokeTyped(CMD_BUBBLE_HIDE);
}

// ── Diagnostics commands ────────────────────────────────────────────────────

export function diagLog(line: string): void {
  if (!isTauriRuntime()) {
    return;
  }
  void invokeTyped(CMD_DIAG_LOG_STR, { line });
}
