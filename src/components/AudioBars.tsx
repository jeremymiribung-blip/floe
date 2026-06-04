import { levelToBarRatio } from "../lib/waveform";

interface AudioBarsProps {
  samples: readonly number[];
}

export function AudioBars({ samples }: AudioBarsProps) {
  return (
    <div className="audio-bars" aria-hidden="true">
      {samples.map((level, index) => (
        <span
          key={index}
          className="audio-bars__bar"
          style={{ height: `${levelToBarRatio(level) * 100}%` }}
        />
      ))}
    </div>
  );
}
