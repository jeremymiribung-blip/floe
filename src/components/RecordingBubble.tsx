import { AudioBars } from "./AudioBars";
import { useBubbleRecordingActive } from "../hooks/useBubbleRecordingActive";
import { useRollingWaveform } from "../hooks/useRollingWaveform";

export function RecordingBubble() {
  const active = useBubbleRecordingActive();
  const samples = useRollingWaveform(active);

  return (
    <div className="recording-bubble" role="presentation">
      <AudioBars samples={samples} />
    </div>
  );
}
