// ─────────────────────────────────────────────────────────────────────────────
// Push-to-talk test fixtures
//
// Typed factories for controller dependencies, callbacks, and domain objects
// used across pushToTalk.test.ts and usePushToTalk.test.tsx. Centralizing these
// keeps individual tests focused on behavior and prevents drift in default
// shapes between suites.
// ─────────────────────────────────────────────────────────────────────────────

import { vi } from "vitest";
import type {
  FloeError,
  RecordingInfo,
  RecordingStatus,
  RecordingState,
  SttResult,
  TranscriptCleanupResult,
} from "../types/app";
import {
  PushToTalkController,
  type PushToTalkCallbacks,
  type PushToTalkDependencies,
} from "../lib/pushToTalk";

// ── Recording domain objects ─────────────────────────────────────────────────

let traceCounter = 1;
export function makeTraceId(prefix = "trace"): string {
  traceCounter += 1;
  return `${prefix}-${traceCounter}`;
}

export function makeRecordingStatus(
  overrides: Partial<RecordingStatus> = {},
): RecordingStatus {
  const traceId = overrides.traceId ?? makeTraceId("rec");
  return {
    isRecording: true,
    sampleRate: 16_000,
    inputChannels: 1,
    outputChannels: 1,
    durationMs: 0,
    sampleCount: 0,
    startedAtMs: 1_000,
    maxDurationSeconds: 120,
    latestRecording: null,
    lastError: null,
    traceId,
    ...overrides,
  };
}

export function makeIdleRecordingStatus(
  overrides: Partial<RecordingStatus> = {},
): RecordingStatus {
  return makeRecordingStatus({
    isRecording: false,
    durationMs: 0,
    sampleCount: 0,
    startedAtMs: null,
    traceId: undefined,
    ...overrides,
  });
}

export function makeRecordingInfo(
  overrides: Partial<RecordingInfo> = {},
): RecordingInfo {
  return {
    sampleRate: 48_000,
    inputChannels: 1,
    outputChannels: 1,
    wavFormat: "wav",
    wavSampleRate: 16_000,
    wavChannels: 1,
    durationMs: 1_500,
    sampleCount: 72_000,
    wavByteCount: 32_044,
    wavBitsPerSample: 16,
    recordingStopToEncodeStartMs: 5,
    audioEncodeMs: 7,
    startedAtMs: 1_000,
    endedAtMs: 2_500,
    maxDurationReached: false,
    endedReason: "manual",
    ...overrides,
  };
}

// ── STT / cleanup ───────────────────────────────────────────────────────────

export function makeSttResult(overrides: Partial<SttResult> = {}): SttResult {
  return {
    text: "Hello world",
    model: "whisper-large-v3-turbo",
    retryCount: 0,
    ...overrides,
  };
}

export function makeCleanupResult(
  overrides: Partial<TranscriptCleanupResult> = {},
): TranscriptCleanupResult {
  return {
    text: "Hello world.",
    model: "qwen/qwen3.6-27b",
    retryCount: 0,
    validationMs: 12,
    fallbackUsed: false,
    ...overrides,
  };
}

// ── Domain error factories ──────────────────────────────────────────────────

export type RecordingErrorCodeLike = Parameters<typeof recordingDomainError>[0];

export function recordingDomainError(
  code:
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
    | "internal",
  message = "Recording failed",
): FloeError {
  return { domain: "recording", code, message };
}

export function sttDomainError(
  code:
    | "missingApiKey"
    | "invalidApiKey"
    | "rateLimit"
    | "timeout"
    | "apiUnreachable"
    | "malformedResponse"
    | "unsupportedAudio"
    | "invalidRequest"
    | "emptyAudio"
    | "serverError",
  message = "Transcription failed",
): FloeError {
  return { domain: "stt", code, message };
}

export function clipboardDomainError(
  code: "clipboardUnavailable" | "pasteUnavailable",
  message = "Clipboard unavailable",
): FloeError {
  return { domain: "clipboard", code, message };
}

// ── PushToTalkDependencies factory ──────────────────────────────────────────

export type FakeDeps = {
  [K in keyof PushToTalkDependencies]: ReturnType<typeof vi.fn>;
};

export interface CreateFakeDepsOptions {
  startRecording?: (...args: unknown[]) => Promise<RecordingStatus>;
  stopRecording?: (...args: unknown[]) => Promise<RecordingInfo>;
  forceStopRecording?: (...args: unknown[]) => Promise<void>;
  getRecordingStatus?: (...args: unknown[]) => Promise<RecordingStatus>;
  transcribeLatestRecording?: (...args: unknown[]) => Promise<SttResult>;
  cleanupTranscript?: (...args: unknown[]) => Promise<TranscriptCleanupResult>;
  copyTextToClipboard?: (...args: unknown[]) => Promise<void>;
  pasteClipboard?: (...args: unknown[]) => Promise<void>;
}

export function createFakeDeps(
  overrides: CreateFakeDepsOptions = {},
): FakeDeps {
  return {
    startRecording: vi.fn(
      overrides.startRecording ??
        (() => Promise.resolve(makeRecordingStatus())),
    ),
    stopRecording: vi.fn(
      overrides.stopRecording ?? (() => Promise.resolve(makeRecordingInfo())),
    ),
    forceStopRecording: vi.fn(
      overrides.forceStopRecording ?? (() => Promise.resolve()),
    ),
    getRecordingStatus: vi.fn(
      overrides.getRecordingStatus ??
        (() => Promise.resolve(makeRecordingStatus({ isRecording: true }))),
    ),
    transcribeLatestRecording: vi.fn(
      overrides.transcribeLatestRecording ??
        (() => Promise.resolve(makeSttResult())),
    ),
    cleanupTranscript: vi.fn(
      overrides.cleanupTranscript ??
        (() => Promise.resolve(makeCleanupResult())),
    ),
    copyTextToClipboard: vi.fn(
      overrides.copyTextToClipboard ?? (() => Promise.resolve()),
    ),
    pasteClipboard: vi.fn(
      overrides.pasteClipboard ?? (() => Promise.resolve()),
    ),
  };
}

// ── PushToTalkCallbacks recorder ────────────────────────────────────────────

export type RecordedState = Parameters<PushToTalkCallbacks["onStateChange"]>[0];

export interface CallbackRecorder {
  onStateChange: PushToTalkCallbacks["onStateChange"];
  onErrorChange: PushToTalkCallbacks["onErrorChange"];
  onRecordingStatusChange: PushToTalkCallbacks["onRecordingStatusChange"];
  onLatestRecordingChange: PushToTalkCallbacks["onLatestRecordingChange"];
  onTranscriptChange: PushToTalkCallbacks["onTranscriptChange"];
  onDiagnosticsChange: PushToTalkCallbacks["onDiagnosticsChange"];
  errorMessage: PushToTalkCallbacks["errorMessage"];
  showToast: PushToTalkCallbacks["showToast"];
  stateChanges: RecordedState[];
  errors: (string | null)[];
  recordingStatuses: RecordingStatus[];
  latestRecordings: RecordingInfo[];
  transcripts: (string | null)[];
  diagnostics: (string | null)[];
  toasts: string[];
}

export interface CreateCallbacksOptions {
  errorMessage?: PushToTalkCallbacks["errorMessage"];
  showToast?: (message: string) => void;
}

export function createCallbacks(
  options: CreateCallbacksOptions = {},
): CallbackRecorder {
  const recorder = {
    stateChanges: [] as RecordedState[],
    errors: [] as (string | null)[],
    recordingStatuses: [] as RecordingStatus[],
    latestRecordings: [] as RecordingInfo[],
    transcripts: [] as (string | null)[],
    diagnostics: [] as (string | null)[],
    toasts: [] as string[],
  };

  const showToastFn: (message: string) => void = options.showToast
    ? (message: string) => {
        recorder.toasts.push(message);
        options.showToast?.(message);
      }
    : (message: string) => {
        recorder.toasts.push(message);
      };

  return {
    onStateChange: vi.fn((state: RecordedState) => {
      recorder.stateChanges.push(state);
    }),
    onErrorChange: vi.fn((message: string | null) => {
      recorder.errors.push(message);
    }),
    onRecordingStatusChange: vi.fn((status: RecordingStatus) => {
      recorder.recordingStatuses.push(status);
    }),
    onLatestRecordingChange: vi.fn((recording: RecordingInfo) => {
      recorder.latestRecordings.push(recording);
    }),
    onTranscriptChange: vi.fn((transcript: string | null) => {
      recorder.transcripts.push(transcript);
    }),
    onDiagnosticsChange: vi.fn((json: string | null) => {
      recorder.diagnostics.push(json);
    }),
    errorMessage:
      options.errorMessage ??
      ((error: FloeError) => `friendly:${error.code}:${error.message}`),
    showToast: vi.fn(showToastFn),
    stateChanges: recorder.stateChanges,
    errors: recorder.errors,
    recordingStatuses: recorder.recordingStatuses,
    latestRecordings: recorder.latestRecordings,
    transcripts: recorder.transcripts,
    diagnostics: recorder.diagnostics,
    toasts: recorder.toasts,
  };
}

// ── Controller factory ──────────────────────────────────────────────────────

export interface CreateControllerOptions {
  deps?: CreateFakeDepsOptions;
  callbacks?: CreateCallbacksOptions;
  now?: () => number;
  createdAt?: () => Date;
  platform?: string;
  appVersion?: string;
}

export interface ControllerHandle {
  controller: PushToTalkController;
  deps: FakeDeps;
  callbacks: CallbackRecorder;
  advanceTime: (ms: number) => void;
  setNowMs: (now: number) => void;
}

export function createController(
  options: CreateControllerOptions = {},
): ControllerHandle {
  const deps = createFakeDeps(options.deps ?? {});
  const callbacks = createCallbacks(options.callbacks);

  let nowValue = 0;
  const nowFn = options.now ?? ((): number => nowValue);
  let createdAtValue = new Date("2026-06-18T00:00:00.000Z");
  const createdAt =
    options.createdAt ?? ((): Date => new Date(createdAtValue.getTime()));

  const controller = new PushToTalkController(
    deps as unknown as PushToTalkDependencies,
    callbacks,
    nowFn,
    createdAt,
    options.platform ?? "test",
    options.appVersion ?? "1.0.0",
  );

  return {
    controller,
    deps,
    callbacks,
    advanceTime: (ms: number) => {
      nowValue += ms;
    },
    setNowMs: (now: number) => {
      nowValue = now;
    },
  };
}

export type { PushToTalkController };
export type { RecordingState };
