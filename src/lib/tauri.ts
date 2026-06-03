import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AppStatus,
  CerebrasApiKeyStatus,
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

const browserStatus: AppStatus = {
  appName: "Floe",
  status: "setup_only",
  message:
    "Push-to-talk recording, transcription, clipboard copy, and paste checks are ready.",
};

let browserGroqApiKeyStatus: GroqApiKeyStatus = {
  configured: false,
  maskedPreview: null,
};
let browserCerebrasApiKeyStatus: CerebrasApiKeyStatus = {
  configured: false,
  maskedPreview: null,
};
let browserAppSettings: AppSettings = {
  hotkey: {
    accelerator: "Control+Shift+Space",
    label: "Control+Shift+Space",
  },
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
  return {
    configured: browserAppSettings.hotkey,
    registered: browserHotkeyRegistered ? browserAppSettings.hotkey : null,
    isRegistered: browserHotkeyRegistered,
    registrationError: browserHotkeyRegistrationError,
  };
}

function normalizeBrowserHotkey(accelerator: string): AppSettings["hotkey"] {
  const trimmed = accelerator.trim();

  if (!trimmed) {
    throw hotkeyError("invalidHotkey", "Enter a valid shortcut.");
  }

  if (!trimmed.includes("+")) {
    throw hotkeyError("unsupportedHotkey", "This shortcut is not supported.");
  }

  const parts = trimmed
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean);
  const key = parts[parts.length - 1] ?? "";

  if (parts.length < 3 || !key) {
    throw hotkeyError("unsupportedHotkey", "This shortcut is not supported.");
  }

  return {
    accelerator: parts.join("+"),
    label: parts
      .map((part) => part.replace(/^Key/, "").replace(/^Digit/, ""))
      .join("+"),
  };
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

export function saveCerebrasApiKey(
  apiKey: string,
): Promise<CerebrasApiKeyStatus> {
  if (!isTauriRuntime()) {
    const trimmed = apiKey.trim();
    browserCerebrasApiKeyStatus = {
      configured: true,
      maskedPreview: maskBrowserApiKey(trimmed),
    };
    return Promise.resolve(browserCerebrasApiKeyStatus);
  }

  return invoke("save_cerebras_api_key", { apiKey });
}

export function clearCerebrasApiKey(): Promise<CerebrasApiKeyStatus> {
  if (!isTauriRuntime()) {
    browserCerebrasApiKeyStatus = {
      configured: false,
      maskedPreview: null,
    };
    return Promise.resolve(browserCerebrasApiKeyStatus);
  }

  return invoke("clear_cerebras_api_key");
}

export function getCerebrasApiKeyStatus(): Promise<CerebrasApiKeyStatus> {
  if (!isTauriRuntime()) {
    return Promise.resolve(browserCerebrasApiKeyStatus);
  }

  return invoke("get_cerebras_api_key_status");
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
      hotkey: {
        accelerator: "Control+Shift+Space",
        label: "Control+Shift+Space",
      },
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
      durationMs,
      sampleCount: Math.floor((durationMs / 1000) * browserSampleRate),
      wavByteCount:
        44 + Math.floor((durationMs / 1000) * browserSampleRate) * 2,
      wavBitsPerSample: 16,
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
    });
  }

  return invoke("transcribe_latest_recording");
}

export function cleanupTranscript(
  transcript: string,
): Promise<TranscriptCleanupResult> {
  if (!isTauriRuntime()) {
    return Promise.resolve({ text: transcript });
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
