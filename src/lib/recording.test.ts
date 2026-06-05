import { describe, expect, it } from "vitest";
import type { RecordingInfo } from "../types/app";
import { formatDurationMs, formatRecordingInfo } from "./recording";

describe("recording helpers", () => {
  it("formats durations for recording metadata", () => {
    expect(formatDurationMs(950)).toBe("0s");
    expect(formatDurationMs(61_000)).toBe("1m 01s");
  });

  it("formats recording metadata without samples", () => {
    const info: RecordingInfo = {
      sampleRate: 48_000,
      inputChannels: 2,
      outputChannels: 1,
      wavFormat: "wav",
      wavSampleRate: 16_000,
      wavChannels: 1,
      durationMs: 2_500,
      sampleCount: 120_000,
      wavByteCount: 240_044,
      wavBitsPerSample: 16,
      recordingStopToEncodeStartMs: 0,
      audioEncodeMs: 1,
      startedAtMs: 1_000,
      endedAtMs: 3_500,
      maxDurationReached: false,
      endedReason: "manual",
    };

    expect(formatRecordingInfo(info)).toBe(
      "2s | 48000 Hz input | 16000 Hz WAV | 2->1 channel | 120000 samples | 240044 WAV bytes | 16-bit PCM | Stopped manually",
    );
  });

  it("formats watchdog timeout end reason", () => {
    const info: RecordingInfo = {
      sampleRate: 48_000,
      inputChannels: 1,
      outputChannels: 1,
      wavFormat: "wav",
      wavSampleRate: 16_000,
      wavChannels: 1,
      durationMs: 125_000,
      sampleCount: 6_000_000,
      wavByteCount: 12_000_044,
      wavBitsPerSample: 16,
      recordingStopToEncodeStartMs: 0,
      audioEncodeMs: 1,
      startedAtMs: 1_000,
      endedAtMs: 126_000,
      maxDurationReached: true,
      endedReason: "watchdogTimeout",
    };

    expect(formatRecordingInfo(info)).toContain("Stopped after timeout");
  });
});
