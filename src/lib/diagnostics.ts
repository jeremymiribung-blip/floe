import type {
  SttResult,
  SttError,
  RecordingInfo,
  TranscriptCleanupResult,
} from "../types/app";
import { assertNoForbiddenKeys, assertNoForbiddenPatterns } from "./privacy";

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
  stt_provider: {
    provider_name: string;
    audio_duration_ms: number;
    transcription_ms: number;
    realtime_factor: number;
    fallback_used: boolean;
    error_code: string | null;
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
  appVersion: string;
  totalMs: number;
  hotkeyToRecordingStartMs: number;
  recordingInfo?: RecordingInfo | null;
  sttDurationMs: number;
  stt?: SttResult | null;
  sttError?: Partial<SttError> | null;
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
    app_version: input.appVersion,
    pipeline,
    models: {
      stt: input.stt?.model ?? input.sttError?.model ?? "",
      cleanup: input.cleanup?.model ?? "",
    },
    stt_provider: sttProviderDiagnostics(input),
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

function sttProviderDiagnostics(
  input: PipelineDiagnosticsInput,
): PipelineDiagnostics["stt_provider"] {
  const provider = input.stt?.sttProvider ?? input.sttError?.sttProvider;
  const audioDurationMs =
    provider?.audioDurationMs ?? input.recordingInfo?.durationMs ?? 0;
  const transcriptionMs = provider?.transcriptionMs ?? input.sttDurationMs;

  return {
    provider_name: provider?.providerName ?? "",
    audio_duration_ms: normalizeDuration(audioDurationMs),
    transcription_ms: normalizeDuration(transcriptionMs),
    realtime_factor: sanitizeRealtimeFactor(
      provider?.realtimeFactor ??
        realtimeFactor(transcriptionMs, audioDurationMs),
    ),
    fallback_used: provider?.fallbackUsed === true,
    error_code: sanitizeDiagnosticCode(provider?.errorCode ?? null),
  };
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
    | NonNullable<SttResult["rateLimit"]>
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

export function assertDiagnosticsSafe(diagnostics: PipelineDiagnostics): void {
  assertNoForbiddenKeys(diagnostics, "");
  const json = diagnosticsToJson(diagnostics);
  assertNoForbiddenPatterns(json);
}

function normalizeDuration(value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return 0;
  }

  return Math.round(value);
}

function realtimeFactor(
  transcriptionMs: number,
  audioDurationMs: number,
): number {
  if (
    !Number.isFinite(transcriptionMs) ||
    !Number.isFinite(audioDurationMs) ||
    audioDurationMs <= 0
  ) {
    return 0;
  }

  return sanitizeRealtimeFactor(transcriptionMs / audioDurationMs);
}

function sanitizeRealtimeFactor(value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return 0;
  }

  return Math.round(value * 1000) / 1000;
}

export function sanitizeDiagnosticCode(
  value: string | null | undefined,
): string | null {
  if (!value) {
    return null;
  }

  const sanitized = value
    .trim()
    .split("")
    .map((ch) => (/[a-zA-Z0-9_-]/.test(ch) ? ch.toLowerCase() : "_"))
    .join("");

  if (
    sanitized.length === 0 ||
    sanitized.length > 64 ||
    sanitized.includes("bearer") ||
    sanitized.includes("authorization") ||
    sanitized.includes("api_key") ||
    sanitized.includes("api-key") ||
    sanitized.includes("gsk_") ||
    sanitized.includes("sk_")
  ) {
    return "internal";
  }

  return sanitized;
}
