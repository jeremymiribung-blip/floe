import { describe, expect, it } from "vitest";
import {
  appendWaveformSample,
  createSilentWaveformBuffer,
  levelToBarRatio,
  smoothWaveformInput,
  WAVEFORM_BUCKET_MS,
  WAVEFORM_SAMPLE_COUNT,
} from "./waveform";

describe("waveform helpers", () => {
  it("creates a fixed-size silent buffer", () => {
    const buffer = createSilentWaveformBuffer();

    expect(buffer).toHaveLength(WAVEFORM_SAMPLE_COUNT);
    expect(buffer.every((sample) => sample === 0)).toBe(true);
  });

  it("keeps the visible bar count in the recorder-style range", () => {
    expect(WAVEFORM_SAMPLE_COUNT).toBeGreaterThanOrEqual(5);
    expect(WAVEFORM_SAMPLE_COUNT).toBeLessThanOrEqual(15);
  });

  it("uses a bucket interval that preserves a longer recent history", () => {
    expect(WAVEFORM_BUCKET_MS).toBeGreaterThanOrEqual(150);
    expect(WAVEFORM_BUCKET_MS).toBeLessThanOrEqual(400);
  });

  it("appends new samples on the right", () => {
    const buffer = createSilentWaveformBuffer(4);
    const next = appendWaveformSample(buffer, 0.7, 4);

    expect(next).toEqual([0, 0, 0, 0.7]);
  });

  it("drops old samples from the left", () => {
    const buffer = [0.1, 0.2, 0.3, 0.4];
    const next = appendWaveformSample(buffer, 0.9, 4);

    expect(next).toEqual([0.2, 0.3, 0.4, 0.9]);
  });

  it("keeps short buffers at the requested fixed length", () => {
    const next = appendWaveformSample([0.4], 0.8, 4);

    expect(next).toEqual([0, 0, 0.4, 0.8]);
  });

  it("maps louder input to taller bars", () => {
    expect(levelToBarRatio(0.9)).toBeGreaterThan(levelToBarRatio(0.1));
  });

  it("maps silence to a low visible bar", () => {
    const silent = levelToBarRatio(0);

    expect(silent).toBeGreaterThan(0);
    expect(silent).toBeLessThan(0.2);
  });

  it("smooths rising input faster than falling input", () => {
    const rising = smoothWaveformInput(0, 1);
    const falling = smoothWaveformInput(1, 0);

    expect(rising).toBeGreaterThan(0.5);
    expect(falling).toBeGreaterThan(0.8);
  });
});
