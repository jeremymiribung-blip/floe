import { describe, expect, it } from "vitest";
import {
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
        model: "llama-3.1-8b-instant",
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
          model: "llama-3.1-8b-instant",
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

  it("uses llama-3.1-8b-instant for cleanup and whisper-large-v3-turbo for stt", () => {
    expect(CLEANUP_MODEL).toBe("llama-3.1-8b-instant");
    expect(STT_MODEL).toBe("whisper-large-v3-turbo");
    expect(CLEANUP_MODEL).not.toContain("gpt-oss");
    expect(CLEANUP_MODEL).not.toContain("openai/");
  });

  it("falls back to llama-3.1-8b-instant when cleanup data is missing", () => {
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

    expect(diagnostics.models.cleanup).toBe("llama-3.1-8b-instant");
    expect(diagnostics.models.stt).toBe("whisper-large-v3-turbo");
    expect(diagnostics.models.cleanup).not.toContain("gpt-oss");
  });
});
