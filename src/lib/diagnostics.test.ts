import { describe, expect, it } from "vitest";
import {
  assertDiagnosticsSafe,
  bottleneckFor,
  CLEANUP_MODEL,
  createPipelineDiagnostics,
  diagnosticsToJson,
  STT_MODEL,
} from "./diagnostics";
import type { RecordingInfo } from "../types/app";

const recordingInfo: RecordingInfo = {
  sampleRate: 16_000,
  inputChannels: 2,
  outputChannels: 1,
  wavFormat: "wav",
  wavSampleRate: 16_000,
  wavChannels: 1,
  durationMs: 3_200,
  sampleCount: 51_200,
  wavByteCount: 102_444,
  wavBitsPerSample: 16,
  recordingStopToEncodeStartMs: 1,
  audioEncodeMs: 18,
  startedAtMs: 1_000,
  endedAtMs: 4_200,
  maxDurationReached: false,
  endedReason: "manual",
};

function fullInput() {
  return {
    createdAt: new Date("2026-01-01T12:00:00.000Z"),
    platform: "windows" as const,
    totalMs: 1_420,
    hotkeyToRecordingStartMs: 12,
    recordingInfo,
    sttDurationMs: 710,
    stt: {
      text: "private transcript",
      model: "whisper-large-v3-turbo",
      retryCount: 0,
      rateLimit: {
        remainingRequests: "9",
        remainingTokens: "1000",
        resetRequests: "2s",
        resetTokens: "5s",
        retryAfterSeconds: 1,
      },
    },
    sttError: undefined,
    cleanupDurationMs: 190,
    cleanup: {
      text: "private cleaned text",
      model: "qwen/qwen3-32b",
      retryCount: 1,
      validationMs: 2,
      fallbackUsed: false,
      rateLimit: {
        remainingRequests: "8",
        remainingTokens: "900",
        resetRequests: "3s",
        retryAfterSeconds: 2,
      },
    },
    cleanupFallbackUsed: false,
    cleanupValidationMs: 2,
    clipboardWriteMs: 8,
    pasteAttemptMs: 35,
    clipboardSuccess: true,
    pasteSuccess: true,
    copiedOnly: false,
    errorStage: null as null,
    sanitizedErrorCode: null as null,
  };
}

describe("diagnostics", () => {
  it("creates the expected trace shape and total duration", () => {
    const diagnostics = createPipelineDiagnostics({
      createdAt: new Date("2026-01-01T12:00:00.000Z"),
      platform: "windows",
      totalMs: 1_420,
      hotkeyToRecordingStartMs: 12,
      recordingInfo,
      sttDurationMs: 710,
      stt: {
        text: "private transcript",
        model: "whisper-large-v3-turbo",
        retryCount: 0,
        rateLimit: {
          remainingRequests: "9",
          remainingTokens: "1000",
          resetRequests: "2s",
        },
      },
      cleanupDurationMs: 190,
      cleanup: {
        text: "private cleaned text",
        model: "qwen/qwen3-32b",
        retryCount: 1,
        validationMs: 2,
        fallbackUsed: false,
      },
      cleanupFallbackUsed: false,
      cleanupValidationMs: 2,
      clipboardWriteMs: 8,
      pasteAttemptMs: 35,
      clipboardSuccess: true,
      pasteSuccess: true,
      copiedOnly: false,
      errorStage: null,
      sanitizedErrorCode: null,
    });

    expect(diagnostics.pipeline.total_ms).toBe(1_420);
    expect(diagnostics.pipeline.recording_duration_ms).toBe(3_200);
    expect(diagnostics.audio.bytes).toBe(102_444);
    expect(diagnostics.audio.sample_rate).toBe(16_000);
    expect(diagnostics.rate_limit?.stt?.remaining_requests).toBe("9");
    expect(diagnostics.retries.cleanup).toBe(1);
    expect(diagnostics.bottleneck).toEqual({
      stage: "stt",
      duration_ms: 710,
    });
  });

  it("calculates the largest bottleneck stage", () => {
    expect(
      bottleneckFor({
        audio_encode: 18,
        stt: 710,
        cleanup: 190,
        cleanup_validation: 2,
        clipboard: 8,
        paste: 35,
      }),
    ).toEqual({
      stage: "stt",
      duration_ms: 710,
    });
  });

  it("does not serialize transcript, cleaned text, API keys, auth headers, or raw audio", () => {
    const json = diagnosticsToJson(
      createPipelineDiagnostics({
        createdAt: new Date("2026-01-01T12:00:00.000Z"),
        platform: "windows",
        totalMs: 10,
        hotkeyToRecordingStartMs: 1,
        recordingInfo,
        sttDurationMs: 2,
        stt: {
          text: "secret transcript gsk_12345678abcd",
          model: "whisper-large-v3-turbo",
          retryCount: 0,
        },
        cleanupDurationMs: 3,
        cleanup: {
          text: "cleaned secret authorization bearer",
          model: "qwen/qwen3-32b",
          retryCount: 0,
          validationMs: 1,
          fallbackUsed: false,
        },
        cleanupFallbackUsed: false,
        cleanupValidationMs: 1,
        clipboardWriteMs: 1,
        pasteAttemptMs: 1,
        clipboardSuccess: true,
        pasteSuccess: true,
        copiedOnly: false,
        errorStage: null,
        sanitizedErrorCode: null,
      }),
    );

    expect(json).not.toContain("secret transcript");
    expect(json).not.toContain("cleaned secret");
    expect(json).not.toContain("gsk_12345678abcd");
    expect(json.toLowerCase()).not.toContain("authorization bearer");
    expect(json).not.toContain("raw_audio");
  });

  it("does not include clipboard text in diagnostics for a paste fallback", () => {
    const clipboardText =
      "clipboard-only-sentinel authorization bearer gsk_clipboard_secret";
    const json = diagnosticsToJson(
      createPipelineDiagnostics({
        createdAt: new Date("2026-01-01T12:00:00.000Z"),
        platform: "windows",
        totalMs: 10,
        hotkeyToRecordingStartMs: 1,
        recordingInfo,
        sttDurationMs: 2,
        stt: {
          text: "transcript sentinel",
          model: "whisper-large-v3-turbo",
          retryCount: 0,
        },
        cleanupDurationMs: 3,
        cleanup: {
          text: "cleaned sentinel",
          model: "qwen/qwen3-32b",
          retryCount: 0,
          validationMs: 1,
          fallbackUsed: false,
        },
        cleanupFallbackUsed: false,
        cleanupValidationMs: 1,
        clipboardWriteMs: 7,
        pasteAttemptMs: 12,
        clipboardSuccess: true,
        pasteSuccess: false,
        copiedOnly: true,
        errorStage: "paste",
        sanitizedErrorCode: "internal",
      }),
    );

    expect(json).not.toContain(clipboardText);
    expect(json).not.toContain("transcript sentinel");
    expect(json).not.toContain("cleaned sentinel");
    expect(json.toLowerCase()).not.toContain("authorization bearer");
    expect(json).not.toContain("gsk_clipboard_secret");
    const parsed = JSON.parse(json);
    expect(parsed.result.clipboard_success).toBe(true);
    expect(parsed.result.paste_success).toBe(false);
    expect(parsed.result.copied_only).toBe(true);
    expect(parsed.result.error_stage).toBe("paste");
  });

  it("uses qwen/qwen3-32b for cleanup and whisper-large-v3-turbo for stt", () => {
    expect(CLEANUP_MODEL).toBe("qwen/qwen3-32b");
    expect(STT_MODEL).toBe("whisper-large-v3-turbo");
    expect(CLEANUP_MODEL).not.toContain("gpt-oss");
    expect(CLEANUP_MODEL).not.toContain("openai/");
  });

  it("falls back to qwen/qwen3-32b when cleanup data is missing", () => {
    const diagnostics = createPipelineDiagnostics({
      createdAt: new Date("2026-01-01T12:00:00.000Z"),
      platform: "windows",
      totalMs: 1,
      hotkeyToRecordingStartMs: 0,
      recordingInfo,
      sttDurationMs: 1,
      cleanupDurationMs: 0,
      cleanupFallbackUsed: false,
      cleanupValidationMs: 0,
      clipboardWriteMs: 0,
      pasteAttemptMs: 0,
      clipboardSuccess: false,
      pasteSuccess: false,
      copiedOnly: false,
      errorStage: null,
      sanitizedErrorCode: null,
    });

    expect(diagnostics.models.cleanup).toBe("qwen/qwen3-32b");
    expect(diagnostics.models.stt).toBe("whisper-large-v3-turbo");
    expect(diagnostics.models.cleanup).not.toContain("gpt-oss");
  });

  it("uses only an allowlist of safe top-level keys", () => {
    const diagnostics = createPipelineDiagnostics(fullInput());
    const keys = Object.keys(diagnostics).sort();
    expect(keys).toEqual(
      [
        "app",
        "app_version",
        "audio",
        "bottleneck",
        "created_at",
        "models",
        "pipeline",
        "platform",
        "rate_limit",
        "result",
        "retries",
        "trace_version",
      ].sort(),
    );
  });

  it("never includes a forbidden key at any level of the diagnostics tree", () => {
    const diagnostics = createPipelineDiagnostics(fullInput());
    const forbidden = new Set([
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

    function walk(value: unknown, path: string): void {
      if (value === null || value === undefined) return;
      if (typeof value !== "object") return;
      if (Array.isArray(value)) {
        for (let i = 0; i < value.length; i += 1) {
          walk(value[i], `${path}[${i}]`);
        }
        return;
      }
      for (const k of Object.keys(value as Record<string, unknown>)) {
        const here = path ? `${path}.${k}` : k;
        expect(
          forbidden.has(k.toLowerCase()),
          `forbidden key "${k}" found at ${here}`,
        ).toBe(false);
        walk((value as Record<string, unknown>)[k], here);
      }
    }

    walk(diagnostics, "");
  });

  it("never serializes Bearer tokens, Groq API keys, or auth header patterns", () => {
    const diagnostics = createPipelineDiagnostics(fullInput());
    const json = diagnosticsToJson(diagnostics);
    expect(json).not.toMatch(/\bBearer\s+[A-Za-z0-9._\-+/=]{8,}/i);
    expect(json).not.toMatch(/gsk_[A-Za-z0-9]{8,}/);
    expect(json).not.toMatch(/Authorization\s*[:=]/i);
    expect(json).not.toMatch(/x-api-key\s*[:=]/i);
    expect(json.toLowerCase()).not.toContain("bearer");
    expect(json).not.toContain("gsk_");
    expect(json.toLowerCase()).not.toContain("authorization");
    expect(json.toLowerCase()).not.toContain("api_key");
    expect(json).not.toContain("api-key");
  });

  it("passes assertDiagnosticsSafe for the full diagnostics output", () => {
    const diagnostics = createPipelineDiagnostics(fullInput());
    expect(() => assertDiagnosticsSafe(diagnostics)).not.toThrow();
  });

  it("throws assertDiagnosticsSafe when a forbidden key is introduced", () => {
    const diagnostics = createPipelineDiagnostics(fullInput());
    const tampered = {
      ...diagnostics,
      // Simulate a future contributor accidentally spreading transcript text.
      transcript: "secret transcript",
    } as unknown as Parameters<typeof assertDiagnosticsSafe>[0];
    expect(() => assertDiagnosticsSafe(tampered)).toThrow(/forbidden key/i);
  });

  it("throws assertDiagnosticsSafe when a Bearer token is introduced", () => {
    const diagnostics = createPipelineDiagnostics(fullInput());
    const tampered = {
      ...diagnostics,
      result: {
        ...diagnostics.result,
        // Simulate a contributor logging an Authorization header value.
        note: "Authorization: Bearer abcdefghijklmnop",
      },
    } as unknown as Parameters<typeof assertDiagnosticsSafe>[0];
    expect(() => assertDiagnosticsSafe(tampered)).toThrow(/forbidden pattern/i);
  });

  it("only includes platform values from the known set", () => {
    const diagnostics = createPipelineDiagnostics(fullInput());
    expect(["windows", "macos", "linux", "unknown"]).toContain(
      diagnostics.platform,
    );
  });

  it("only includes sanitized error codes from the known enums", () => {
    const diagnostics = createPipelineDiagnostics({
      ...fullInput(),
      errorStage: "stt",
      sanitizedErrorCode: "timeout",
    });
    const allowedCodes = new Set([
      "missingApiKey",
      "invalidApiKey",
      "rateLimit",
      "timeout",
      "apiUnreachable",
      "malformedResponse",
      "unsupportedAudio",
      "invalidRequest",
      "emptyAudio",
      "serverError",
      "noInputDevice",
      "permissionDenied",
      "alreadyRecording",
      "notRecording",
      "emptyRecording",
      "tooShortRecording",
      "unsupportedSampleFormat",
      "deviceDisconnected",
      "streamBuildFailed",
      "streamPlayFailed",
      "wavEncodingFailed",
      "stopFailed",
      "watchdogTimeout",
      "internal",
      "invalidHotkey",
      "unsupportedHotkey",
      "alreadyInUse",
      "registrationFailed",
      "unregisterFailed",
      "settingsUnavailable",
      "enableFailed",
      "disableFailed",
      "unavailable",
      "invalidGroqApiKey",
      "invalidAppSettings",
      "secretStoreUnavailable",
      "appSettingsUnavailable",
      "clipboardUnavailable",
      "pasteUnavailable",
      "emptyTranscript",
      "validationFailed",
    ]);
    expect(
      allowedCodes.has(diagnostics.result.sanitized_error_code ?? ""),
    ).toBe(true);
  });

  it("keeps the rate_limit object to only safe metadata fields", () => {
    const diagnostics = createPipelineDiagnostics(fullInput());
    const allowed = new Set([
      "stt",
      "cleanup",
      "remaining_requests",
      "remaining_tokens",
      "reset_requests",
      "reset_tokens",
      "retry_after_seconds",
    ]);
    function walk(value: unknown): void {
      if (value === null || value === undefined || typeof value !== "object") {
        return;
      }
      if (Array.isArray(value)) {
        for (const item of value) walk(item);
        return;
      }
      for (const k of Object.keys(value as Record<string, unknown>)) {
        expect(allowed.has(k), `unexpected rate_limit key: ${k}`).toBe(true);
        walk((value as Record<string, unknown>)[k]);
      }
    }
    walk(diagnostics.rate_limit);
  });
});
