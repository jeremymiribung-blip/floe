import { AudioBars } from "./AudioBars";
import { useRecordingLevel } from "../hooks/useRecordingLevel";

export function RecordingBubble() {
  const level = useRecordingLevel(true);

  return (
    <div className="recording-bubble" role="presentation">
      <AudioBars level={level} />
    </div>
  );
}
