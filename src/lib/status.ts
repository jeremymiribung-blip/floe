import type { AppState } from "../types/app";

const statusLabels: Record<AppState, string> = {
  idle: "Idle",
  capturing_hotkey: "Capturing hotkey",
  recording: "Recording",
  transcribing: "Transcribing",
  cleaning: "Cleaning",
  pasting: "Pasting",
  pasted: "Pasted",
  error: "Needs attention",
};

export function statusLabel(state: AppState): string {
  return statusLabels[state];
}
