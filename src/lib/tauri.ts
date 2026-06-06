import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AppStatus,
  ClipboardError,
  HotkeyError,
  HotkeyStatus,
  GroqTranscription,
  GroqTranscriptionError,
  GroqApiKeyStatus,
  ManualTestResult,
  RecordingError,
  RecordingInfo,
  RecordingStatus,
  StartAtLoginStatus,
  TranscriptCleanupResult,
} from "../types/app";
import { CLEANUP_MODEL, STT_MODEL } from "./models";
import { isMacLikePlatform } from "./hotkeyCapture";

const browserStatus: AppStatus = {
  appName: "Floe",
  status: "setup_only",
  message:
    "Push-to-talk recording, transcription, clipboard copy, and paste checks are ready.",
};

function browserDefaultHotkey() {
  return isMacLikePlatform()
    ? { accelerator: "Alt+Space", label: "Option + Space" }
    : { accelerator: "Control+Space", label: "Ctrl + Space" };
}

let browserGroqApiKeyStatus: GroqApiKeyStatus = {
  configured: false,
  maskedPreview: null,
};
let browserAppSettings: AppSettings = {
  hotkey: browserDefaultHotkey(),
};
let browserHotkeyRegistered = true;
let browserHotkeyRegistrationError: string | null = null;
let browserStartAtLoginEnabled = false;
let browserClipboardText = "";

const browserSampleRate = 48_000;
const browserMaxDurationSeconds = 120;
let browserRecordingStartedAtMs: number | null = null;
let browserLatestRecording: RecordingInfo | null = null;
let browserLastError: RecordingError | null = null;

export function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

function recordingError(
  code: RecordingError["code"],
  message: string,
): RecordingError {
  return { code, message };
}

function transcriptionError(
  code: GroqTranscriptionError["code"],
  message: string,
): GroqTranscriptionError {
  return { code, message };
}

function clipboardError(
  code: ClipboardError["code"],
  message: string,
): ClipboardError {
  return { code, message };
}

function hotkeyError(code: HotkeyError["code"], message: string): HotkeyError {
  return { code, message };
}

function browserHotkeyStatus(): HotkeyStatus {
  const hotkey = browserAppSettings.hotkey;
  const defaultHotkey = browserDefaultHotkey();

  return {
    accelerator: hotkey.accelerator,
    label: hotkey.label,
    isDefault: hotkey.accelerator === defaultHotkey.accelerator,
    isRegistered: browserHotkeyRegistered,
    error: browserHotkeyRegistered ? null : browserHotkeyRegistrationError,
  };
}

function normalizeBrowserHotkey(accelerator: string): AppSettings["hotkey"] {
  const trimmed = accelerator.trim();

  if (!trimmed) {
    throw hotkeyError("invalidHotkey", "Enter a valid shortcut.");
  }

  const parts = trimmed
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean);
  const key = parts[parts.length - 1] ?? "";

  if (parts.length < 2 || !key) {
    throw hotkeyError("unsupportedHotkey", "This shortcut is not supported.");
  }

  const modifiers = parts.slice(0, -1).map((part) => part.toUpperCase());
  const modifierSet = new Set(modifiers);
  const hasPrimaryModifier =
    modifierSet.has("CONTROL") ||
    modifierSet.has("CTRL") ||
    modifierSet.has("ALT") ||
    modifierSet.has("OPTION") ||
    modifierSet.has("SUPER") ||
    modifierSet.has("COMMAND") ||
    modifierSet.has("CMD");

  if (!hasPrimaryModifier) {
    throw hotkeyError("unsupportedHotkey", "This shortcut is not supported.");
  }

  if (
    /^Key[A-Z]$/.test(key) === false &&
    /^Digit[0-9]$/.test(key) === false &&
    /^F([1-9]|1[0-9]|2[0-4])$/.test(key) === false &&
    [
      "Backquote",
      "Backslash",
      "BracketLeft",
      "BracketRight",
      "Comma",
      "Delete",
      "End",
      "Enter",
      "Equal",
      "Home",
      "Insert",
      "Minus",
      "PageDown",
      "PageUp",
      "Period",
      "Quote",
      "Semicolon",
      "Slash",
      "Space",
      "Tab",
    ].includes(key) === false
  ) {
    throw hotkeyError("unsupportedHotkey", "This shortcut is not supported.");
  }

  const canonicalModifiers = modifiers.map((modifier) => {
    if (modifier === "CTRL") return "Control";
    if (modifier === "OPTION") return "Alt";
    if (modifier === "CMD") return "Super";
    return modifier[0] + modifier.slice(1).toLowerCase();
  });

  const canonicalKey =
    /^Key[A-Z]$/.test(key) || /^Digit[0-9]$/.test(key) ? key : key;

  return {
    accelerator: [...canonicalModifiers, canonicalKey].join("+"),
    label: [
      ...canonicalModifiers.map((modifier) =>
        browserModifierLabel(modifier, isMacLikePlatform()),
      ),
      key.replace(/^Key/, "").replace(/^Digit/, ""),
    ].join(" + "),
  };
}

function browserModifierLabel(modifier: string, mac: boolean): string {
  if (modifier === "Control") return mac ? "Control" : "Ctrl";
  if (modifier === "Alt") return mac ? "Option" : "Alt";
  if (modifier === "Shift") return "Shift";
  if (modifier === "Super") return mac ? "Command" : "Super";
  return modifier;
}

function maskBrowserApiKey(apiKey: string): string {
  if (apiKey.length < 12) {
    return "Configured key";
  }

  return `${apiKey.slice(0, 4)}...${apiKey.slice(-4)}`;
}

function currentBrowserRecordingStatus(): RecordingStatus {
  const now = Date.now();
  const durationMs =
    browserRecordingStartedAtMs === null
      ? 0
      : Math.min(
          now - browserRecordingStartedAtMs,
          browserMaxDurationSeconds * 1000,
        );

  return {
    isRecording: browserRecordingStartedAtMs !== null,
    sampleRate: browserRecordingStartedAtMs === null ? null : browserSampleRate,
    inputChannels: browserRecordingStartedAtMs === null ? null : 1,
    outputChannels: 1,
    durationMs,
    sampleCount: Math.floor((durationMs / 1000) * browserSampleRate),
    startedAtMs: browserRecordingStartedAtMs,
    maxDurationSeconds: browserMaxDurationSeconds,
    latestRecording: browserLatestRecording,
    lastError: browserLastError,
  };
}

export function getAppStatus(): Promise<AppStatus> {
  if (!isTauriRuntime()) {
    return Promise.resolve(browserStatus);
  }

  return invoke("get_app_status");
}

export function saveGroqApiKey(apiKey: string): Promise<GroqApiKeyStatus> {
  if (!isTauriRuntime()) {
    const trimmed = apiKey.trim();
    browserGroqApiKeyStatus = {
      configured: true,
      maskedPreview: maskBrowserApiKey(trimmed),
    };
    return Promise.resolve(browserGroqApiKeyStatus);
  }

  return invoke("save_groq_api_key", { apiKey });
}

export function clearGroqApiKey(): Promise<GroqApiKeyStatus> {
  if (!isTauriRuntime()) {
    browserGroqApiKeyStatus = {
      configured: false,
      maskedPreview: null,
    };
    return Promise.resolve(browserGroqApiKeyStatus);
  }

  return invoke("clear_groq_api_key");
}

export function getGroqApiKeyStatus(): Promise<GroqApiKeyStatus> {
  if (!isTauriRuntime()) {
    return Promise.resolve(browserGroqApiKeyStatus);
  }

  return invoke("get_groq_api_key_status");
}

export function getAppSettings(): Promise<AppSettings> {
  if (!isTauriRuntime()) {
    return Promise.resolve(browserAppSettings);
  }

  return invoke("get_app_settings");
}

export function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  if (!isTauriRuntime()) {
    browserAppSettings = {
      hotkey: normalizeBrowserHotkey(settings.hotkey.accelerator),
    };
    return Promise.resolve(browserAppSettings);
  }

  return invoke("save_app_settings", { settings });
}

export function getHotkeySettings(): Promise<HotkeyStatus> {
  if (!isTauriRuntime()) {
    return Promise.resolve(browserHotkeyStatus());
  }

  return invoke("get_hotkey_settings");
}

export function setHotkey(accelerator: string): Promise<HotkeyStatus> {
  if (!isTauriRuntime()) {
    browserAppSettings = {
      ...browserAppSettings,
      hotkey: normalizeBrowserHotkey(accelerator),
    };
    browserHotkeyRegistered = true;
    browserHotkeyRegistrationError = null;

    return Promise.resolve(browserHotkeyStatus());
  }

  return invoke("set_hotkey", { accelerator });
}

export function resetHotkeyToDefault(): Promise<HotkeyStatus> {
  if (!isTauriRuntime()) {
    browserAppSettings = {
      ...browserAppSettings,
      hotkey: browserDefaultHotkey(),
    };
    browserHotkeyRegistered = true;
    browserHotkeyRegistrationError = null;

    return Promise.resolve(browserHotkeyStatus());
  }

  return invoke("reset_hotkey_to_default");
}

export function registerGlobalHotkey(): Promise<HotkeyStatus> {
  if (!isTauriRuntime()) {
    browserHotkeyRegistered = true;
    browserHotkeyRegistrationError = null;

    return Promise.resolve(browserHotkeyStatus());
  }

  return invoke("register_global_hotkey");
}

export function unregisterGlobalHotkey(): Promise<HotkeyStatus> {
  if (!isTauriRuntime()) {
    browserHotkeyRegistered = false;
    browserHotkeyRegistrationError = null;

    return Promise.resolve(browserHotkeyStatus());
  }

  return invoke("unregister_global_hotkey");
}

export function getStartAtLoginStatus(): Promise<StartAtLoginStatus> {
  if (!isTauriRuntime()) {
    return Promise.resolve({
      enabled: browserStartAtLoginEnabled,
      available: true,
    });
  }

  return invoke("get_start_at_login_status");
}

export function setStartAtLoginEnabled(
  enabled: boolean,
): Promise<StartAtLoginStatus> {
  if (!isTauriRuntime()) {
    browserStartAtLoginEnabled = enabled;

    return Promise.resolve({
      enabled,
      available: true,
    });
  }

  return invoke("set_start_at_login_enabled", { enabled });
}

export function runManualTestStub(action: string): Promise<ManualTestResult> {
  if (!isTauriRuntime()) {
    return Promise.resolve({
      action,
      message: `${action} is a placeholder for a future implementation task.`,
    });
  }

  return invoke("run_manual_test_stub", { action });
}

export function startRecording(): Promise<RecordingStatus> {
  if (!isTauriRuntime()) {
    if (browserRecordingStartedAtMs !== null) {
      browserLastError = recordingError(
        "alreadyRecording",
        "A recording is already in progress.",
      );
      return Promise.reject(browserLastError);
    }

    browserRecordingStartedAtMs = Date.now();
    browserLastError = null;
    return Promise.resolve(currentBrowserRecordingStatus());
  }

  return invoke("start_recording");
}

export function stopRecording(): Promise<RecordingInfo> {
  if (!isTauriRuntime()) {
    if (browserRecordingStartedAtMs === null) {
      browserLastError = recordingError(
        "notRecording",
        "No recording is currently in progress.",
      );
      return Promise.reject(browserLastError);
    }

    const endedAtMs = Date.now();
    const durationMs = Math.min(
      endedAtMs - browserRecordingStartedAtMs,
      browserMaxDurationSeconds * 1000,
    );

    if (durationMs <= 0) {
      browserRecordingStartedAtMs = null;
      browserLastError = recordingError(
        "emptyRecording",
        "The recording did not capture any audio samples.",
      );
      return Promise.reject(browserLastError);
    }

    browserLatestRecording = {
      sampleRate: browserSampleRate,
      inputChannels: 1,
      outputChannels: 1,
      wavFormat: "wav",
      wavSampleRate: 16_000,
      wavChannels: 1,
      durationMs,
      sampleCount: Math.floor((durationMs / 1000) * browserSampleRate),
      wavByteCount: 44 + Math.floor((durationMs / 1000) * 16_000) * 2,
      wavBitsPerSample: 16,
      recordingStopToEncodeStartMs: 0,
      audioEncodeMs: 0,
      startedAtMs: browserRecordingStartedAtMs,
      endedAtMs: browserRecordingStartedAtMs + durationMs,
      maxDurationReached: durationMs >= browserMaxDurationSeconds * 1000,
      endedReason:
        durationMs >= browserMaxDurationSeconds * 1000
          ? "maxDuration"
          : "manual",
    };
    browserRecordingStartedAtMs = null;
    browserLastError = null;

    return Promise.resolve(browserLatestRecording);
  }

  return invoke("stop_recording");
}

export function getRecordingStatus(): Promise<RecordingStatus> {
  if (!isTauriRuntime()) {
    return Promise.resolve(currentBrowserRecordingStatus());
  }

  return invoke("get_recording_status");
}

export function getLatestRecordingInfo(): Promise<RecordingInfo | null> {
  if (!isTauriRuntime()) {
    return Promise.resolve(browserLatestRecording);
  }

  return invoke("get_latest_recording_info");
}

export function getLatestRecordingWavBytes(): Promise<number[] | null> {
  if (!isTauriRuntime()) {
    return Promise.resolve(null);
  }

  return invoke("get_latest_recording_wav_bytes");
}

export function transcribeLatestRecording(): Promise<GroqTranscription> {
  if (!isTauriRuntime()) {
    if (browserLatestRecording === null) {
      return Promise.reject(
        transcriptionError(
          "emptyAudio",
          "Record audio before requesting a transcription.",
        ),
      );
    }

    return Promise.resolve({
      text: "Mock transcript from the latest manual recording.",
      model: STT_MODEL,
      retryCount: 0,
    });
  }

  return invoke("transcribe_latest_recording");
}

export function cleanupTranscript(
  transcript: string,
): Promise<TranscriptCleanupResult> {
  if (!isTauriRuntime()) {
    return Promise.resolve({
      text: transcript,
      model: CLEANUP_MODEL,
      retryCount: 0,
      validationMs: 0,
      fallbackUsed: false,
    });
  }

  return invoke("cleanup_transcript", { transcript });
}

export function copyTextToClipboard(text: string): Promise<void> {
  if (!isTauriRuntime()) {
    browserClipboardText = text;
    return Promise.resolve();
  }

  return invoke("copy_text_to_clipboard", { text });
}

export function pasteText(text: string): Promise<void> {
  if (!isTauriRuntime()) {
    browserClipboardText = text;
    return Promise.resolve();
  }

  return invoke("paste_text", { text });
}

export function pasteClipboard(): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.resolve();
  }

  return invoke("paste_clipboard");
}

export interface RecordingLevelPayload {
  level: number;
}

export function bubbleShow(): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.resolve();
  }

  return invoke("bubble_show");
}

export function bubbleHide(): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.resolve();
  }

  return invoke("bubble_hide");
}

export function getBrowserClipboardTextForTest(): string {
  if (isTauriRuntime()) {
    throw clipboardError(
      "clipboardUnavailable",
      "Browser clipboard test state is unavailable in Tauri.",
    );
  }

  return browserClipboardText;
}
