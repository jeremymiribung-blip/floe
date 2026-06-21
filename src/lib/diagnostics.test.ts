import { describe, expect, it } from "vitest";
import {
  createPipelineDiagnostics,
  diagnosticsToJson,
  bottleneckFor,
  assertDiagnosticsSafe,
  sanitizeDiagnosticCode,
  type PipelineDiagnosticsInput,
} from "./diagnostics";
import { assertNoForbiddenPatterns } from "./privacy";

function sampleInput(
  overrides?: Partial<PipelineDiagnosticsInput>,
): PipelineDiagnosticsInput {
  return {
    createdAt: new Date("2026-06-18T00:00:00.000Z"),
    platform: "macos",
    appVersion: "1.0.0",
    totalMs: 2_737,
    hotkeyToRecordingStartMs: 25,
    recordingInfo: {
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
    },
    sttDurationMs: 800,
    stt: {
      text: "Hello world",
      model: "whisper-large-v3-turbo",
      retryCount: 0,
      sttProvider: {
        providerName: "groq",
        audioDurationMs: 1_500,
        transcriptionMs: 800,
        realtimeFactor: 0.533,
        fallbackUsed: false,
      },
    },
    sttError: null,
    cleanupDurationMs: 300,
    cleanup: {
      text: "Hello world (cleaned)",
      model: "llama-3.3-70b-versatile",
      retryCount: 0,
      validationMs: 12,
      fallbackUsed: false,
    },
    cleanupFallbackUsed: false,
    cleanupErrorCode: null,
    cleanupValidationMs: 12,
    clipboardWriteMs: 8,
    pasteAttemptMs: 80,
    clipboardSuccess: true,
    pasteSuccess: true,
    copiedOnly: false,
    errorStage: null,
    sanitizedErrorCode: null,
    ...overrides,
  };
}

describe("createPipelineDiagnostics", () => {
  it("produces a complete diagnostics object for a successful session", () => {
    const diag = createPipelineDiagnostics(sampleInput());

    expect(diag.app).toBe("Floe");
    expect(diag.trace_version).toBe(1);
    expect(diag.platform).toBe("macos");
    expect(diag.app_version).toBe("1.0.0");

    // Pipeline timings
    expect(diag.pipeline.total_ms).toBe(2_737);
    expect(diag.pipeline.hotkey_to_recording_start_ms).toBe(25);
    expect(diag.pipeline.recording_duration_ms).toBe(1_500);
    expect(diag.pipeline.stt_ms).toBe(800);
    expect(diag.pipeline.cleanup_ms).toBe(300);
    expect(diag.pipeline.clipboard_ms).toBe(8);
    expect(diag.pipeline.paste_ms).toBe(80);

    // Models
    expect(diag.models.stt).toBe("whisper-large-v3-turbo");
    expect(diag.models.cleanup).toBe("llama-3.3-70b-versatile");

    // STT provider
    expect(diag.stt_provider.provider_name).toBe("groq");
    expect(diag.stt_provider.audio_duration_ms).toBe(1_500);
    expect(diag.stt_provider.transcription_ms).toBe(800);
    expect(diag.stt_provider.realtime_factor).toBeCloseTo(0.533, 3);
    expect(diag.stt_provider.fallback_used).toBe(false);
    expect(diag.stt_provider.error_code).toBeNull();

    // Audio
    expect(diag.audio.format).toBe("wav");
    expect(diag.audio.sample_rate).toBe(16_000);
    expect(diag.audio.channels).toBe(1);
    expect(diag.audio.bytes).toBe(32_044);

    // Rate limit
    expect(diag.rate_limit).toBeUndefined();

    // Retries
    expect(diag.retries.stt).toBe(0);
    expect(diag.retries.cleanup).toBe(0);

    // Result
    expect(diag.result.stt_success).toBe(true);
    expect(diag.result.cleanup_success).toBe(true);
    expect(diag.result.cleanup_fallback_used).toBe(false);
    expect(diag.result.clipboard_success).toBe(true);
    expect(diag.result.paste_success).toBe(true);
    expect(diag.result.copied_only).toBe(false);
    expect(diag.result.error_stage).toBeNull();
    expect(diag.result.sanitized_error_code).toBeNull();

    // Bottleneck (STT should be the bottleneck at 800ms)
    expect(diag.bottleneck.stage).toBe("stt");
    expect(diag.bottleneck.duration_ms).toBe(800);
  });

  it("does not include rate_limit when not provided", () => {
    const diag = createPipelineDiagnostics(sampleInput());
    expect(diag.rate_limit).toBeUndefined();
  });

  it("handles missing recording info gracefully", () => {
    const input = sampleInput({ recordingInfo: null });
    const diag = createPipelineDiagnostics(input);

    expect(diag.audio.format).toBe("wav");
    expect(diag.audio.sample_rate).toBe(0);
    expect(diag.audio.channels).toBe(1);
    expect(diag.audio.bytes).toBe(0);
    expect(diag.pipeline.recording_duration_ms).toBe(0);
  });

  it("handles null stt result (transcription failure)", () => {
    const input = sampleInput({
      stt: null,
      sttError: {
        domain: "stt",
        code: "timeout",
        message: "Transcription timed out",
        model: "whisper-large-v3-turbo",
        retryCount: 2,
      },
      errorStage: "stt",
      sanitizedErrorCode: "timeout",
    });
    const diag = createPipelineDiagnostics(input);

    expect(diag.result.stt_success).toBe(false);
    expect(diag.result.error_stage).toBe("stt");
    expect(diag.result.sanitized_error_code).toBe("timeout");
    expect(diag.models.stt).toBe("whisper-large-v3-turbo");
    expect(diag.retries.stt).toBe(2);
  });

  it("handles cleanup fallback", () => {
    const input = sampleInput({
      cleanupDurationMs: 200,
      cleanup: {
        text: "raw transcript",
        model: "",
        retryCount: 1,
        validationMs: 0,
        fallbackUsed: true,
        errorCode: "server_error",
      },
      cleanupFallbackUsed: true,
      cleanupErrorCode: "server_error",
      errorStage: "cleanup",
      sanitizedErrorCode: "server_error",
    });
    const diag = createPipelineDiagnostics(input);

    expect(diag.result.cleanup_success).toBe(false);
    expect(diag.result.cleanup_fallback_used).toBe(true);
    expect(diag.result.error_stage).toBe("cleanup");
    expect(diag.result.sanitized_error_code).toBe("server_error");
    expect(diag.retries.cleanup).toBe(1);
  });

  it("records copied_only when paste fails after clipboard succeeds", () => {
    const input = sampleInput({
      pasteAttemptMs: 50,
      clipboardSuccess: true,
      pasteSuccess: false,
      copiedOnly: true,
      errorStage: "paste",
    });
    const diag = createPipelineDiagnostics(input);

    expect(diag.result.clipboard_success).toBe(true);
    expect(diag.result.paste_success).toBe(false);
    expect(diag.result.copied_only).toBe(true);
    expect(diag.result.error_stage).toBe("paste");
  });

  it("produces valid JSON and passes safety checks", () => {
    const diag = createPipelineDiagnostics(sampleInput());
    const json = diagnosticsToJson(diag);
    const parsed = JSON.parse(json);

    expect(parsed.app).toBe("Floe");
    expect(parsed.trace_version).toBe(1);
    expect(parsed.pipeline.total_ms).toBe(2_737);
    expect(parsed.models.stt).toBe("whisper-large-v3-turbo");
    expect(parsed.models.cleanup).toBe("llama-3.3-70b-versatile");
    expect(parsed.stt_provider.provider_name).toBe("groq");
    expect(parsed.result.stt_success).toBe(true);
    expect(parsed.result.cleanup_success).toBe(true);
    expect(parsed.bottleneck.stage).toBe("stt");

    // The JSON must pass the safety assertion
    expect(() => assertDiagnosticsSafe(diag)).not.toThrow();
  });
});

describe("bottleneckFor", () => {
  it("returns the stage with the highest duration", () => {
    const result = bottleneckFor({
      audio_encode: 7,
      stt: 800,
      cleanup: 300,
      cleanup_validation: 12,
      clipboard: 8,
      paste: 80,
    });

    expect(result.stage).toBe("stt");
    expect(result.duration_ms).toBe(800);
  });

  it("handles all-zero durations gracefully", () => {
    const result = bottleneckFor({
      audio_encode: 0,
      stt: 0,
      cleanup: 0,
      cleanup_validation: 0,
      clipboard: 0,
      paste: 0,
    });

    expect(result.stage).toBe("audio_encode");
    expect(result.duration_ms).toBe(0);
  });

  it("handles negative durations as zero", () => {
    const result = bottleneckFor({
      audio_encode: -1,
      stt: -5,
      cleanup: 0,
      cleanup_validation: 0,
      clipboard: 0,
      paste: 0,
    });

    expect(result.duration_ms).toBe(0);
  });

  it("identifies the correct bottleneck when paste is the longest", () => {
    const result = bottleneckFor({
      audio_encode: 5,
      stt: 50,
      cleanup: 30,
      cleanup_validation: 2,
      clipboard: 3,
      paste: 200,
    });

    expect(result.stage).toBe("paste");
    expect(result.duration_ms).toBe(200);
  });

  it("returns the first-winning stage on tie (stt wins over cleanup when equal)", () => {
    // stt is iterated before cleanup, so tie goes to stt
    const result = bottleneckFor({
      audio_encode: 100,
      stt: 500,
      cleanup: 500,
      clipboard: 10,
      paste: 50,
      cleanup_validation: 0,
    });

    expect(result.stage).toBe("stt");
  });
});

describe("sanitizeDiagnosticCode", () => {
  it("passes through safe codes", () => {
    expect(sanitizeDiagnosticCode("timeout")).toBe("timeout");
    expect(sanitizeDiagnosticCode("server_error")).toBe("server_error");
    expect(sanitizeDiagnosticCode("rate_limit")).toBe("rate_limit");
    expect(sanitizeDiagnosticCode("noInputDevice")).toBe("noinputdevice");
    expect(sanitizeDiagnosticCode("emptyAudio")).toBe("emptyaudio");
  });

  it("returns null for null/undefined/empty", () => {
    expect(sanitizeDiagnosticCode(null)).toBeNull();
    expect(sanitizeDiagnosticCode(undefined)).toBeNull();
    expect(sanitizeDiagnosticCode("")).toBeNull();
  });

  it("returns 'internal' for bearer tokens", () => {
    expect(sanitizeDiagnosticCode("Bearer gsk_abc123")).toBe("internal");
  });

  it("returns 'internal' for authorization headers", () => {
    expect(sanitizeDiagnosticCode("Authorization: Bearer xyz")).toBe(
      "internal",
    );
  });

  it("returns 'internal' for groq API keys", () => {
    expect(sanitizeDiagnosticCode("gsk_abc123")).toBe("internal");
  });

  it("returns 'internal' for OpenAI-style keys with sk_", () => {
    expect(sanitizeDiagnosticCode("sk_proj_abc123")).toBe("internal");
  });

  it("returns 'internal' for api_key patterns", () => {
    expect(sanitizeDiagnosticCode("api_key=secret")).toBe("internal");
    expect(sanitizeDiagnosticCode("api-key=secret")).toBe("internal");
  });

  it("returns 'internal' for overly long codes", () => {
    expect(sanitizeDiagnosticCode("a".repeat(65))).toBe("internal");
  });

  it("replaces special characters with underscores", () => {
    const result = sanitizeDiagnosticCode("error: timeout!");
    expect(result).toBe("error__timeout_");
    // Should not contain the original special characters
    expect(result).not.toContain(":");
    expect(result).not.toContain("!");
  });
});

describe("assertDiagnosticsSafe", () => {
  it("accepts a clean diagnostics object", () => {
    const diag = createPipelineDiagnostics(sampleInput());
    expect(() => assertDiagnosticsSafe(diag)).not.toThrow();
  });

  it("rejects diagnostics with a transcript field", () => {
    const diag = createPipelineDiagnostics(sampleInput());
    // Inject a forbidden key at runtime to simulate a future mistake
    const dirty = JSON.parse(JSON.stringify(diag));
    dirty.transcript = "this should never leak";
    expect(() => assertDiagnosticsSafe(dirty)).toThrow();
  });

  it("rejects diagnostics with api_key in the JSON string", () => {
    const diag = createPipelineDiagnostics(sampleInput());
    // The assertNoForbiddenPatterns check runs on the JSON string.
    // Inject a forbidden pattern into the JSON to verify it's caught.
    const dirtyJson = JSON.stringify(diag).replace(
      '"sanitized_error_code":null',
      '"sanitized_error_code":"gsk_abcdefghij"',
    );
    expect(() => assertNoForbiddenPatterns(dirtyJson)).toThrow(
      /forbidden pattern/i,
    );
  });
});

describe("createPipelineDiagnostics - stt error codes", () => {
  it("captures error code from sttError when stt is null", () => {
    const input = sampleInput({
      stt: null,
      sttError: {
        domain: "stt",
        code: "emptyAudio",
        message: "No audio recorded",
        sttProvider: {
          providerName: "groq",
          audioDurationMs: 0,
          transcriptionMs: 0,
          realtimeFactor: 0,
          fallbackUsed: false,
          errorCode: "emptyAudio",
        },
      },
      errorStage: "stt",
      sanitizedErrorCode: "emptyaudio",
    });
    const diag = createPipelineDiagnostics(input);

    expect(diag.stt_provider.error_code).toBe("emptyaudio");
    expect(diag.stt_provider.provider_name).toBe("groq");
    expect(diag.stt_provider.audio_duration_ms).toBe(0);
  });
});

describe("createPipelineDiagnostics - cleanup validation ms", () => {
  it("includes cleanup validation timing", () => {
    const input = sampleInput({ cleanupValidationMs: 42 });
    const diag = createPipelineDiagnostics(input);

    expect(diag.pipeline.cleanup_validation_ms).toBe(42);
  });
});
