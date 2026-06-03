export type AppState =
  | "idle"
  | "recording"
  | "transcribing"
  | "cleaning"
  | "pasting"
  | "pasted"
  | "error";

export interface AppStatus {
  appName: "Floe";
  status: "setup_only";
  message: string;
}

export interface GroqApiKeyStatus {
  configured: boolean;
  maskedPreview: string | null;
}

export interface CerebrasApiKeyStatus {
  configured: boolean;
  maskedPreview: string | null;
}

export type CleanupMode = "raw" | "fast" | "clean";

export interface AppSettings {
  hotkey: HotkeySettings;
  cleanupMode: CleanupMode;
}

export interface HotkeySettings {
  accelerator: string;
  label: string;
}

export interface SettingsError {
  code:
    | "invalidGroqApiKey"
    | "invalidCerebrasApiKey"
    | "missingCerebrasApiKey"
    | "invalidAppSettings"
    | "secretStoreUnavailable"
    | "appSettingsUnavailable";
  message: string;
}

export interface ManualTestResult {
  action: string;
  message: string;
}

export type RecordingEndReason =
  | "manual"
  | "maxDuration"
  | "deviceDisconnected"
  | "shutdown";

export type RecordingErrorCode =
  | "noInputDevice"
  | "permissionDenied"
  | "alreadyRecording"
  | "notRecording"
  | "emptyRecording"
  | "unsupportedSampleFormat"
  | "deviceDisconnected"
  | "streamBuildFailed"
  | "streamPlayFailed"
  | "wavEncodingFailed"
  | "internal";

export interface RecordingError {
  code: RecordingErrorCode;
  message: string;
}

export interface RecordingInfo {
  sampleRate: number;
  inputChannels: number;
  outputChannels: 1;
  durationMs: number;
  sampleCount: number;
  wavByteCount: number;
  wavBitsPerSample: 16;
  startedAtMs: number;
  endedAtMs: number;
  maxDurationReached: boolean;
  endedReason: RecordingEndReason;
}

export interface RecordingStatus {
  isRecording: boolean;
  sampleRate: number | null;
  inputChannels: number | null;
  outputChannels: 1;
  durationMs: number;
  sampleCount: number;
  startedAtMs: number | null;
  maxDurationSeconds: number;
  latestRecording: RecordingInfo | null;
  lastError: RecordingError | null;
}

export type GroqTranscriptionErrorCode =
  | "missingApiKey"
  | "invalidApiKey"
  | "rateLimit"
  | "timeout"
  | "apiUnreachable"
  | "malformedResponse"
  | "unsupportedAudio"
  | "invalidRequest"
  | "emptyAudio"
  | "serverError";

export interface GroqTranscription {
  text: string;
}

export interface GroqTranscriptionError {
  code: GroqTranscriptionErrorCode;
  message: string;
}

export interface TranscriptCleanupResult {
  text: string;
  mode: CleanupMode;
  warning: string | null;
}

export type ClipboardErrorCode = "clipboardUnavailable" | "pasteUnavailable";

export interface ClipboardError {
  code: ClipboardErrorCode;
  message: string;
}
