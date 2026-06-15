export type RecordingState = "idle" | "starting" | "recording" | "stopping";

export type AppState =
  | "idle"
  | "ready"
  | "starting"
  | "recording"
  | "stopping"
  | "transcribing"
  | "cleaning"
  | "pasting"
  | "pasted"
  | "copied"
  | "error";

export interface RecordingStatePayload {
  state: RecordingState;
  isRecording: boolean;
}

export interface AppStatus {
  appName: "Floe";
  status: "setup_only";
  message: string;
}

export interface ApiKeyStatus {
  configured: boolean;
  maskedPreview: string | null;
}

export interface AppSettings {
  hotkey: HotkeySettings;
}

export interface HotkeySettings {
  accelerator: string;
  label: string;
}

export interface HotkeyStatus {
  accelerator: string;
  label: string;
  isDefault: boolean;
  isRegistered: boolean;
  error: string | null;
}

export interface HotkeyError {
  domain: "hotkey";
  code:
    | "invalidHotkey"
    | "unsupportedHotkey"
    | "alreadyInUse"
    | "registrationFailed"
    | "unregisterFailed"
    | "settingsUnavailable";
  message: string;
}

export interface StartAtLoginStatus {
  enabled: boolean;
  available: boolean;
}

export interface StartAtLoginError {
  domain: "startAtLogin";
  code: "enableFailed" | "disableFailed" | "unavailable";
  message: string;
}

export interface GlobalHotkeyEvent {
  state: "Pressed" | "Released";
}

export interface SettingsError {
  domain: "settings";
  code:
    | "invalidGroqApiKey"
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
  | "shutdown"
  | "watchdogTimeout";

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
  | "stopFailed"
  | "watchdogTimeout"
  | "appShuttingDown"
  | "internal";

export interface RecordingError {
  domain: "recording";
  code: RecordingErrorCode;
  message: string;
}

export interface RecordingInfo {
  sampleRate: number;
  inputChannels: number;
  outputChannels: 1;
  wavFormat: "wav";
  wavSampleRate: number;
  wavChannels: 1;
  durationMs: number;
  sampleCount: number;
  wavByteCount: number;
  wavBitsPerSample: 16;
  recordingStopToEncodeStartMs: number;
  audioEncodeMs: number;
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
  traceId?: string;
}

export type SttErrorCode =
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

export interface SttResult {
  text: string;
  model: string;
  retryCount: number;
  rateLimit?: RateLimitMetadata;
}

export interface SttError {
  domain: "stt";
  code: SttErrorCode;
  message: string;
  model?: string;
  retryCount?: number;
  rateLimit?: RateLimitMetadata;
}

export interface TranscriptCleanupResult {
  text: string;
  warning?: string;
  model?: string;
  retryCount?: number;
  validationMs?: number;
  fallbackUsed?: boolean;
  rateLimit?: RateLimitMetadata;
  errorCode?: string;
}

export interface RateLimitMetadata {
  remainingRequests?: string;
  remainingTokens?: string;
  resetRequests?: string;
  resetTokens?: string;
  retryAfterSeconds?: number;
}

export type ClipboardErrorCode = "clipboardUnavailable" | "pasteUnavailable";

export type FloeError =
  | SettingsError
  | HotkeyError
  | RecordingError
  | SttError
  | ClipboardError
  | StartAtLoginError;

export interface ClipboardError {
  domain: "clipboard";
  code: ClipboardErrorCode;
  message: string;
}
