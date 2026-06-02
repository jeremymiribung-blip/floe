import { invoke } from "@tauri-apps/api/core";
import type {
  AppStatus,
  ManualTestResult,
  RecordingError,
  RecordingInfo,
  RecordingStatus,
  SettingsStub,
} from "../types/app";

const browserStatus: AppStatus = {
  appName: "Floe",
  status: "setup_only",
  message:
    "Initial scaffold is ready. Runtime transcription features are not implemented yet.",
};

const browserSettings: SettingsStub = {
  hasGroqApiKey: false,
  hotkeyLabel: "Not configured",
  storageLabel: "Keychain storage not wired yet",
};

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

export function getSettingsStub(): Promise<SettingsStub> {
  if (!isTauriRuntime()) {
    return Promise.resolve(browserSettings);
  }

  return invoke("get_settings_stub");
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
