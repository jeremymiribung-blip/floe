import { useRecordingStore } from "../../stores/recording";
import { useRollingWaveform } from "../../hooks/useRollingWaveform";
import { AudioBars } from "../AudioBars";

export function RecordingDot() {
  const state = useRecordingStore((s) => s.overlayState);
  const active = state === "active";
  const samples = useRollingWaveform(active);

  if (!active) return null;

  return (
    <div
      role="status"
      aria-live="polite"
      aria-label="Recording"
      className="fixed bottom-1 left-1/2 z-50 -translate-x-1/2 select-none"
    >
      <div
        className="flex items-center rounded-[999px] border px-3 py-1.5 shadow-[0_4px_8px_rgba(0,0,0,0.4),0_8px_24px_rgba(0,0,0,0.3)]"
        style={{
          backgroundColor: "rgba(10, 10, 10, 0.93)",
          borderColor: "rgba(42, 42, 42, 0.7)",
          backdropFilter: "blur(12px)",
          WebkitBackdropFilter: "blur(12px)",
        }}
        aria-hidden="true"
      >
        <AudioBars samples={samples} />
      </div>
    </div>
  );
}
