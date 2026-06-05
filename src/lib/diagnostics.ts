import type {
  GroqTranscription,
  GroqTranscriptionError,
  RecordingInfo,
  TranscriptCleanupResult,
} from "../types/app";

export const STT_MODEL = "whisper-large-v3-turbo";
export const CLEANUP_MODEL = "qwen/qwen3-32b";

const APP_VERSION = "0.1.0";
const TRACE_VERSION = 1;

export type PipelineStage =
  | "hotkey"
  | "recording"
  | "audio_encode"
  | "stt"
  | "cleanup"
  | "cleanup_validation"
  | "clipboard"
  | "paste";

export interface PipelineDiagnostics {
  app: "Floe";
  trace_version: 1;
  created_at: string;
  platform: string;
  app_version: string;
  pipeline: {
    total_ms: number;
    hotkey_to_recording_start_ms: number;
    recording_duration_ms: number;
    recording_stop_to_encode_start_ms: number;
    audio_encode_ms: number;
    stt_ms: number;
    cleanup_ms: number;
    cleanup_validation_ms: number;
    clipboard_ms: number;
    paste_ms: number;
  };
  models: {
    stt: string;
    cleanup: string;
  };
  audio: {
    format: "wav";
    sample_rate: number;
    channels: number;
    bytes: number;
  };
  rate_limit?: {
    stt?: {
      remaining_requests?: string;
      remaining_tokens?: string;
      reset_requests?: string;
      reset_tokens?: string;
      retry_after_seconds?: number;
    };
    cleanup?: {
      remaining_requests?: string;
      remaining_tokens?: string;
      reset_requests?: string;
      reset_tokens?: string;
      retry_after_seconds?: number;
    };
  };
  retries: {
    stt: number;
    cleanup: number;
  };
  result: {
    stt_success: boolean;
    cleanup_success: boolean;
    cleanup_fallback_used: boolean;
    clipboard_success: boolean;
    paste_success: boolean;
    copied_only: boolean;
    error_stage: PipelineStage | null;
    sanitized_error_code: string | null;
  };
  bottleneck: {
    stage: string;
    duration_ms: number;
  };
}

export interface PipelineDiagnosticsInput {
  createdAt: Date;
  platform: string;
  totalMs: number;
  hotkeyToRecordingStartMs: number;
  recordingInfo?: RecordingInfo | null;
  sttDurationMs: number;
  stt?: GroqTranscription | null;
  sttError?: Partial<GroqTranscriptionError> | null;
  cleanupDurationMs: number;
  cleanup?: TranscriptCleanupResult | null;
  cleanupFallbackUsed: boolean;
  cleanupErrorCode?: string | null;
  cleanupValidationMs: number;
  clipboardWriteMs: number;
  pasteAttemptMs: number;
  clipboardSuccess: boolean;
  pasteSuccess: boolean;
  copiedOnly: boolean;
  errorStage: PipelineStage | null;
  sanitizedErrorCode: string | null;
}

export function createPipelineDiagnostics(
  input: PipelineDiagnosticsInput,
): PipelineDiagnostics {
  const recordingInfo = input.recordingInfo;
  const pipeline = {
    total_ms: normalizeDuration(input.totalMs),
    hotkey_to_recording_start_ms: normalizeDuration(
      input.hotkeyToRecordingStartMs,
    ),
    recording_duration_ms: normalizeDuration(recordingInfo?.durationMs ?? 0),
    recording_stop_to_encode_start_ms: normalizeDuration(
      recordingInfo?.recordingStopToEncodeStartMs ?? 0,
    ),
    audio_encode_ms: normalizeDuration(recordingInfo?.audioEncodeMs ?? 0),
    stt_ms: normalizeDuration(input.sttDurationMs),
    cleanup_ms: normalizeDuration(input.cleanupDurationMs),
    cleanup_validation_ms: normalizeDuration(input.cleanupValidationMs),
    clipboard_ms: normalizeDuration(input.clipboardWriteMs),
    paste_ms: normalizeDuration(input.pasteAttemptMs),
  };

  const diagnostics: PipelineDiagnostics = {
    app: "Floe",
    trace_version: TRACE_VERSION,
    created_at: input.createdAt.toISOString(),
    platform: input.platform,
    app_version: APP_VERSION,
    pipeline,
    models: {
      stt: input.stt?.model ?? input.sttError?.model ?? STT_MODEL,
      cleanup: input.cleanup?.model ?? CLEANUP_MODEL,
    },
    audio: {
      format: recordingInfo?.wavFormat ?? "wav",
      sample_rate:
        recordingInfo?.wavSampleRate ?? recordingInfo?.sampleRate ?? 0,
      channels:
        recordingInfo?.wavChannels ?? recordingInfo?.outputChannels ?? 1,
      bytes: recordingInfo?.wavByteCount ?? 0,
    },
    rate_limit: rateLimitDiagnostics(input),
    retries: {
      stt: input.stt?.retryCount ?? input.sttError?.retryCount ?? 0,
      cleanup: input.cleanup?.retryCount ?? 0,
    },
    result: {
      stt_success: input.stt !== null && input.stt !== undefined,
      cleanup_success:
        input.cleanup !== null &&
        input.cleanup !== undefined &&
        input.cleanupFallbackUsed === false,
      cleanup_fallback_used: input.cleanupFallbackUsed,
      clipboard_success: input.clipboardSuccess,
      paste_success: input.pasteSuccess,
      copied_only: input.copiedOnly,
      error_stage: input.errorStage,
      sanitized_error_code: input.sanitizedErrorCode,
    },
    bottleneck: bottleneckFor({
      audio_encode: pipeline.audio_encode_ms,
      stt: pipeline.stt_ms,
      cleanup: pipeline.cleanup_ms,
      cleanup_validation: pipeline.cleanup_validation_ms,
      clipboard: pipeline.clipboard_ms,
      paste: pipeline.paste_ms,
    }),
  };
  assertDiagnosticsSafe(diagnostics);
  return diagnostics;
}

function rateLimitDiagnostics(
  input: PipelineDiagnosticsInput,
): PipelineDiagnostics["rate_limit"] {
  const stt = sanitizeRateLimit(
    input.stt?.rateLimit ?? input.sttError?.rateLimit,
  );
  const cleanup = sanitizeRateLimit(input.cleanup?.rateLimit);

  if (!stt && !cleanup) {
    return undefined;
  }

  return {
    ...(stt ? { stt } : {}),
    ...(cleanup ? { cleanup } : {}),
  };
}

function sanitizeRateLimit(
  metadata:
    | NonNullable<GroqTranscription["rateLimit"]>
    | NonNullable<TranscriptCleanupResult["rateLimit"]>
    | undefined,
):
  | NonNullable<NonNullable<PipelineDiagnostics["rate_limit"]>["stt"]>
  | undefined {
  if (!metadata) {
    return undefined;
  }

  const sanitized = {
    remaining_requests: metadata.remainingRequests,
    remaining_tokens: metadata.remainingTokens,
    reset_requests: metadata.resetRequests,
    reset_tokens: metadata.resetTokens,
    retry_after_seconds: metadata.retryAfterSeconds,
  };
  const hasValue = Object.values(sanitized).some(
    (value) => value !== undefined && value !== null && value !== "",
  );

  return hasValue ? sanitized : undefined;
}

export function diagnosticsToJson(diagnostics: PipelineDiagnostics): string {
  return JSON.stringify(diagnostics, null, 2);
}

export function bottleneckFor(
  durations: Record<string, number>,
): PipelineDiagnostics["bottleneck"] {
  let stage = "audio_encode";
  let durationMs = -1;

  for (const [nextStage, duration] of Object.entries(durations)) {
    const normalized = normalizeDuration(duration);
    if (normalized > durationMs) {
      stage = nextStage;
      durationMs = normalized;
    }
  }

  return {
    stage,
    duration_ms: Math.max(0, durationMs),
  };
}

const FORBIDDEN_KEYS: ReadonlySet<string> = new Set([
  "transcript",
  "transcripts",
  "cleaned",
  "cleaned_text",
  "text",
  "api_key",
  "apikey",
  "api-key",
  "key",
  "bearer",
  "authorization",
  "auth",
  "samples",
  "raw_audio",
  "rawaudio",
  "audio_data",
  "audiodata",
  "audio_bytes",
  "audiobytes",
  "wav",
  "wav_bytes",
  "wavbytes",
  "pcm",
  "pcm_samples",
  "pcmsamples",
  "clipboard",
  "clipboard_text",
  "clipboardtext",
  "response",
  "response_body",
  "responsebody",
  "body",
  "payload",
  "headers",
  "request",
  "url",
  "endpoint",
]);

const FORBIDDEN_SUBSTRINGS: ReadonlyArray<{
  pattern: RegExp;
  name: string;
}> = [
  { pattern: /\bBearer\s+[A-Za-z0-9._\-+/=]{8,}/i, name: "Bearer token" },
  { pattern: /gsk_[A-Za-z0-9]{8,}/, name: "Groq API key prefix" },
  {
    pattern: /Authorization\s*[:=]/i,
    name: "Authorization header",
  },
  {
    pattern: /x-api-key\s*[:=]/i,
    name: "x-api-key header",
  },
];

export function assertDiagnosticsSafe(diagnostics: PipelineDiagnostics): void {
  assertNoForbiddenKeys(diagnostics, "");
  const json = diagnosticsToJson(diagnostics);
  for (const { pattern, name } of FORBIDDEN_SUBSTRINGS) {
    if (pattern.test(json)) {
      throw new Error(`Diagnostics contain forbidden pattern: ${name}`);
    }
  }
}

function assertNoForbiddenKeys(value: unknown, path: string): void {
  if (value === null || value === undefined) {
    return;
  }
  if (typeof value !== "object") {
    return;
  }
  if (Array.isArray(value)) {
    for (let i = 0; i < value.length; i += 1) {
      assertNoForbiddenKeys(value[i], `${path}[${i}]`);
    }
    return;
  }
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (FORBIDDEN_KEYS.has(key.toLowerCase())) {
      throw new Error(
        `Diagnostics contain forbidden key: ${path ? `${path}.` : ""}${key}`,
      );
    }
    assertNoForbiddenKeys(record[key], path ? `${path}.${key}` : key);
  }
}

function normalizeDuration(value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return 0;
  }

  return Math.round(value);
}
