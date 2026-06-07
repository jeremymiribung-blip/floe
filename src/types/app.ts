export type AppState =
  | "idle"
  | "ready"
  | "recording"
  | "transcribing"
  | "cleaning"
  | "pasting"
  | "pasted"
  | "copied"
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
  code: "enableFailed" | "disableFailed" | "unavailable";
  message: string;
}

export interface GlobalHotkeyEvent {
  state: "Pressed" | "Released";
}

export interface SettingsError {
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
  | "internal";

export interface RecordingError {
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
  model: string;
  retryCount: number;
  rateLimit?: GroqRateLimitMetadata;
  localAsr?: LocalAsrDiagnostics;
}

export interface GroqTranscriptionError {
  code: GroqTranscriptionErrorCode;
  message: string;
  model?: string;
  retryCount?: number;
  rateLimit?: GroqRateLimitMetadata;
  localAsr?: LocalAsrDiagnostics;
}

export type PipelineMode = "groq_cloud" | "experimental_nemotron_streaming";

export interface LocalAsrDiagnostics {
  pipelineMode: PipelineMode;
  localAsrEnabled: boolean;
  localAsrAvailable: boolean;
  sidecarConnected: boolean;
  sidecarStartMs: number;
  localAsrSessionMs: number;
  localAsrFinalWaitMs: number;
  localAsrErrorCode: string | null;
  fallbackToGroqUsed: boolean;
  fallbackReason: string | null;
}

export interface TranscriptCleanupResult {
  text: string;
  warning?: string;
  model?: string;
  retryCount?: number;
  validationMs?: number;
  fallbackUsed?: boolean;
  rateLimit?: GroqRateLimitMetadata;
  errorCode?:
    | "missingApiKey"
    | "invalidApiKey"
    | "rateLimit"
    | "timeout"
    | "apiUnreachable"
    | "malformedResponse"
    | "invalidRequest"
    | "emptyTranscript"
    | "validationFailed"
    | "serverError";
}

export interface GroqRateLimitMetadata {
  remainingRequests?: string;
  remainingTokens?: string;
  resetRequests?: string;
  resetTokens?: string;
  retryAfterSeconds?: number;
}

export type ClipboardErrorCode = "clipboardUnavailable" | "pasteUnavailable";

export interface ClipboardError {
  code: ClipboardErrorCode;
  message: string;
}
