export const WAVEFORM_SAMPLE_COUNT: number = 11;
export const WAVEFORM_BUCKET_MS: number = 200;
export const SILENT_SAMPLE_LEVEL: number = 0;

const MIN_BAR_RATIO: number = 0.18;
const MAX_BAR_RATIO: number = 1;

export function createSilentWaveformBuffer(
  length: number = WAVEFORM_SAMPLE_COUNT,
): number[] {
  return Array.from({ length }, () => SILENT_SAMPLE_LEVEL);
}

export function appendWaveformSample(
  buffer: readonly number[],
  level: number,
  length: number = WAVEFORM_SAMPLE_COUNT,
): number[] {
  const normalized = clamp01(level);
  const base =
    buffer.length >= length
      ? buffer.slice(buffer.length - length + 1)
      : [
          ...createSilentWaveformBuffer(length - buffer.length - 1),
          ...buffer.map(clamp01),
        ];

  return [...base, normalized];
}

export function levelToBarRatio(level: number): number {
  const normalized = clamp01(level);
  const shaped = Math.sqrt(normalized);
  return MIN_BAR_RATIO + (MAX_BAR_RATIO - MIN_BAR_RATIO) * shaped;
}

export function smoothWaveformInput(previous: number, next: number): number {
  const normalizedPrevious = clamp01(previous);
  const normalizedNext = clamp01(next);
  const coefficient = normalizedNext > normalizedPrevious ? 0.55 : 0.16;
  return clamp01(
    normalizedPrevious + (normalizedNext - normalizedPrevious) * coefficient,
  );
}

export function clamp01(value: number): number {
  if (Number.isNaN(value) || !Number.isFinite(value)) {
    return 0;
  }
  if (value < 0) {
    return 0;
  }
  if (value > 1) {
    return 1;
  }
  return value;
}
