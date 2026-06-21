// ─────────────────────────────────────────────────────────────────────────────
// Tauri IPC bridge
//
// Typed invocation layer for backend commands used by the frontend.
// No browser fallback needed — the app only runs inside Tauri.
// ─────────────────────────────────────────────────────────────────────────────

import { invoke } from "@tauri-apps/api/core";
import type {
  ApiKeyStatus,
  AppSettings,
  HotkeyStatus,
  RecordingInfo,
  RecordingStatus,
  StartAtLoginStatus,
  SttResult,
  TranscriptCleanupResult,
  UpdateInfo,
  UpdateError,
  AudioDevice,
} from "../types/app";

import {
  CMD_LOG_FRONTEND_EVENT,
  CMD_SAVE_API_KEY,
  CMD_VALIDATE_API_KEY,
  CMD_CLEAR_API_KEY,
  CMD_GET_API_KEY_STATUS,
  CMD_GET_APP_SETTINGS,
  CMD_GET_AUDIO_DEVICES,
  CMD_SAVE_APP_SETTINGS,
  CMD_GET_HOTKEY_SETTINGS,
  CMD_SET_HOTKEY,
  CMD_RESET_HOTKEY_TO_DEFAULT,
  CMD_GET_START_AT_LOGIN_STATUS,
  CMD_SET_START_AT_LOGIN_ENABLED,
  CMD_GET_RECORDING_STATUS,
  CMD_START_RECORDING,
  CMD_STOP_RECORDING,
  CMD_FORCE_STOP_RECORDING,
  CMD_GET_LATEST_RECORDING_INFO,
  CMD_TRANSCRIBE_LATEST_RECORDING,
  CMD_CLEANUP_TRANSCRIPT,
  CMD_COPY_TEXT_TO_CLIPBOARD,
  CMD_PASTE_CLIPBOARD,
  CMD_BUBBLE_SHOW,
  CMD_BUBBLE_HIDE,
  CMD_BUBBLE_CANCEL_RECORDING,
  CMD_DIAG_LOG_STR,
  CMD_GET_DIAGNOSTICS_REPORT,
  CMD_UPDATE_SESSION_HOTKEY_LATENCY,
  CMD_GET_UPDATE_INFO,
  CMD_CHECK_FOR_UPDATE,
  CMD_DOWNLOAD_UPDATE,
  CMD_INSTALL_UPDATE,
  CMD_RESET_UPDATE_STATE,
} from "./contract";

// ── Runtime check ──

export function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

// ── API key commands ──

export function saveApiKey(apiKey: string): Promise<ApiKeyStatus> {
  return invoke(CMD_SAVE_API_KEY, { apiKey });
}

export function validateApiKey(apiKey: string): Promise<boolean> {
  return invoke(CMD_VALIDATE_API_KEY, { apiKey });
}

export function clearApiKey(): Promise<ApiKeyStatus> {
  return invoke(CMD_CLEAR_API_KEY);
}

export function getApiKeyStatus(): Promise<ApiKeyStatus> {
  return invoke(CMD_GET_API_KEY_STATUS);
}

// ── App settings ──

export function getAppSettings(): Promise<AppSettings> {
  return invoke(CMD_GET_APP_SETTINGS);
}

export function getAudioDevices(): Promise<AudioDevice[]> {
  return invoke(CMD_GET_AUDIO_DEVICES);
}

export function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke(CMD_SAVE_APP_SETTINGS, { settings });
}

// ── Hotkey commands ──

export function getHotkeySettings(): Promise<HotkeyStatus> {
  return invoke(CMD_GET_HOTKEY_SETTINGS);
}

export function setHotkey(accelerator: string): Promise<HotkeyStatus> {
  return invoke(CMD_SET_HOTKEY, { accelerator });
}

export function resetHotkeyToDefault(): Promise<HotkeyStatus> {
  return invoke(CMD_RESET_HOTKEY_TO_DEFAULT);
}

// ── Start-at-login commands ──

export function getStartAtLoginStatus(): Promise<StartAtLoginStatus> {
  return invoke(CMD_GET_START_AT_LOGIN_STATUS);
}

export function setStartAtLoginEnabled(
  enabled: boolean,
): Promise<StartAtLoginStatus> {
  return invoke(CMD_SET_START_AT_LOGIN_ENABLED, { enabled });
}

// ── Recording commands ──

export function startRecording(): Promise<RecordingStatus> {
  return invoke(CMD_START_RECORDING);
}

export function stopRecording(): Promise<RecordingInfo> {
  return invoke(CMD_STOP_RECORDING);
}

export function forceStopRecording(): Promise<void> {
  return invoke(CMD_FORCE_STOP_RECORDING);
}

export function getRecordingStatus(): Promise<RecordingStatus> {
  return invoke(CMD_GET_RECORDING_STATUS);
}

export function getLatestRecordingInfo(): Promise<RecordingInfo | null> {
  return invoke(CMD_GET_LATEST_RECORDING_INFO);
}

// ── Transcription commands ──

export function transcribeLatestRecording(): Promise<SttResult> {
  return invoke(CMD_TRANSCRIBE_LATEST_RECORDING);
}

export function cleanupTranscript(
  transcript: string,
  skipCleanup: boolean = false,
): Promise<TranscriptCleanupResult> {
  return invoke(CMD_CLEANUP_TRANSCRIPT, { transcript, skip_cleanup: skipCleanup });
}

// ── Clipboard commands ──

export function copyTextToClipboard(text: string): Promise<void> {
  return invoke(CMD_COPY_TEXT_TO_CLIPBOARD, { text });
}

export function pasteClipboard(): Promise<void> {
  return invoke(CMD_PASTE_CLIPBOARD);
}

// ── Bubble (overlay) commands ──

export function bubbleShow(): Promise<void> {
  return invoke<void>(CMD_BUBBLE_SHOW).catch((err: unknown) => {
    diagLog(`[tauri] bubbleShow failed: ${err}`);
  });
}

export function bubbleHide(): Promise<void> {
  return invoke<void>(CMD_BUBBLE_HIDE).catch((err: unknown) => {
    diagLog(`[tauri] bubbleHide failed: ${err}`);
  });
}

export function bubbleCancelRecording(): Promise<void> {
  return invoke(CMD_BUBBLE_CANCEL_RECORDING);
}

// ── Diagnostics ──

export function diagLog(line: string): void {
  invoke(CMD_DIAG_LOG_STR, { line }).catch((err) =>
    console.error("diagLog failed:", err),
  );
}

export interface DiagnosticsReport {
  schema_version: number;
  app: string;
  app_version: string;
  generated_at: string;
  environment: string;
  platform: {
    os: string;
    arch: string;
    family: string;
    tauri_version: string | null;
    os_version: string | null;
    cpu_model: string | null;
    cpu_logical_cores: number | null;
    memory_total_mb: number | null;
    memory_available_mb: number | null;
    process_memory_mb: number | null;
    uptime_secs: number | null;
  };
  hotkey: {
    accelerator: string;
    label: string;
    is_default: boolean;
    is_registered: boolean;
    error: string | null;
  };
  settings: {
    api_key_configured: boolean;
    api_key_masked_preview: string | null;
    start_at_login_enabled: boolean | null;
    start_at_login_available: boolean | null;
    keyring_migrated: boolean;
    feature_flags: Record<string, boolean>;
  };
  provider_state: {
    configured: boolean;
    available: boolean;
  };
  last_session: {
    has_session: boolean;
    trace_id: string | null;
    completed: boolean;
    stage_summary: Record<string, unknown>;
    stages: Record<string, Record<string, unknown>>;
    audio: Record<string, unknown> | null;
    stt_provider: Record<string, unknown> | null;
    recovery_actions: Array<Record<string, unknown>>;
    rate_limit: Record<string, unknown> | null;
    retries: Record<string, number>;
    pipeline_total_ms: number | null;
    recording_started_at_unix_ms: number | null;
    recording_ended_at_unix_ms: number | null;
    detailed_timeline: Array<Record<string, unknown>>;
    cleanup_chars: number | null;
    cleanup_words: number | null;
  };
  last_error: Record<string, unknown> | null;
  state_flags: Record<string, boolean>;
  event_timeline: Array<Record<string, unknown>>;
}

export function getDiagnosticsReport(): Promise<DiagnosticsReport> {
  return invoke(CMD_GET_DIAGNOSTICS_REPORT);
}

export function updateSessionHotkeyLatency(
  traceId: string,
  hotkeyToRecordingStartMs: number,
): Promise<void> {
  return invoke(CMD_UPDATE_SESSION_HOTKEY_LATENCY, {
    trace_id: traceId,
    hotkey_to_recording_start_ms: hotkeyToRecordingStartMs,
  });
}

// ── Update commands ──

export function getUpdateInfo(): Promise<UpdateInfo> {
  return invoke(CMD_GET_UPDATE_INFO);
}

export function checkForUpdate(): Promise<UpdateInfo> {
  return invoke(CMD_CHECK_FOR_UPDATE);
}

export function downloadUpdate(): Promise<UpdateInfo> {
  return invoke(CMD_DOWNLOAD_UPDATE);
}

export function installUpdate(): Promise<void> {
  return invoke(CMD_INSTALL_UPDATE);
}

export function resetUpdateState(): Promise<void> {
  return invoke(CMD_RESET_UPDATE_STATE);
}

export interface FrontendEvent {
  traceId: string;
  stage: string;
  eventType: string;
  durationMs: number;
  errorCode?: string | null;
  retryCount?: number | null;
  pipelineTotalMs?: number | null;
}

/**
 * Push a structured lifecycle event from the frontend pipeline into
 * the backend's detailed_timeline so the diagnostics report can
 * reconstruct frontend-only timing and transitions.
 */
export async function logFrontendEvent(event: FrontendEvent): Promise<void> {
  await invoke(CMD_LOG_FRONTEND_EVENT, { event });
}
