import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AppStatus,
  ClipboardError,
  GroqTranscription,
  GroqTranscriptionError,
  GroqApiKeyStatus,
  ManualTestResult,
  RecordingError,
  RecordingInfo,
  RecordingStatus,
} from "../types/app";

const browserStatus: AppStatus = {
  appName: "Floe",
  status: "setup_only",
  message:
    "Manual recording, transcription, clipboard copy, and paste checks are ready.",
};

let browserGroqApiKeyStatus: GroqApiKeyStatus = {
  configured: false,
  maskedPreview: null,
};
let browserAppSettings: AppSettings = {
  hotkeyLabel: "Not configured",
};
let browserClipboardText = "";

const browserSampleRate = 48_000;
const browserMaxDurationSeconds = 120;
let browserRecordingStartedAtMs: number | null = null;
let browserLatestRecording: RecordingInfo | null = null;
let browserLastError: RecordingError | null = null;

function isTauriRuntime(): boolean {
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
      hotkeyLabel: settings.hotkeyLabel.trim(),
    };
    return Promise.resolve(browserAppSettings);
  }

  return invoke("save_app_settings", { settings });
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

export function getBrowserClipboardTextForTest(): string {
  if (isTauriRuntime()) {
    throw clipboardError(
      "clipboardUnavailable",
      "Browser clipboard test state is unavailable in Tauri.",
    );
  }

  return browserClipboardText;
}
