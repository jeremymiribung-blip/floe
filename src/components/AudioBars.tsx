import { clamp01 } from "../hooks/useRecordingLevel";

const BAR_COUNT: number = 12;
const MIN_BAR_RATIO: number = 0.18;
const MAX_BAR_RATIO: number = 1;

interface AudioBarsProps {
  level: number;
}

export function AudioBars({ level }: AudioBarsProps) {
  const normalized = clamp01(level);
  const bars = Array.from({ length: BAR_COUNT }, (_, index) => {
    const distribution = symmetricDistribution(index, BAR_COUNT);
    return scaleBar(normalized, distribution);
  });

  return (
    <div className="audio-bars" aria-hidden="true">
      {bars.map((height, index) => (
        <span
          key={index}
          className="audio-bars__bar"
          style={{ height: `${height * 100}%` }}
        />
      ))}
    </div>
  );
}

function symmetricDistribution(index: number, total: number): number {
  const center = (total - 1) / 2;
  const distance = Math.abs(index - center) / center;
  return 1 - distance * 0.6;
}

function scaleBar(level: number, distribution: number): number {
  const scaled =
    MIN_BAR_RATIO + (MAX_BAR_RATIO - MIN_BAR_RATIO) * level * distribution;
  if (scaled < MIN_BAR_RATIO) {
    return MIN_BAR_RATIO;
  }
  if (scaled > MAX_BAR_RATIO) {
    return MAX_BAR_RATIO;
  }
  return scaled;
}
