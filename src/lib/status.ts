import type { AppState } from "../types/app";

const statusLabels: Record<AppState, string> = {
  idle: "Idle",
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
