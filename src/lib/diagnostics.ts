import type {
  GroqTranscription,
  GroqTranscriptionError,
  RecordingInfo,
  TranscriptCleanupResult,
} from "../types/app";

export const STT_MODEL = "whisper-large-v3-turbo";
export const CLEANUP_MODEL = "llama-3.1-8b-instant";

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

  return {
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
      format: "wav",
      sample_rate: recordingInfo?.sampleRate ?? 0,
      channels: recordingInfo?.outputChannels ?? 1,
      bytes: recordingInfo?.wavByteCount ?? 0,
    },
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

function normalizeDuration(value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    return 0;
  }

  return Math.round(value);
}
