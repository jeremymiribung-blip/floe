import type { AppState } from "../types/app";

const statusLabels: Record<AppState, string> = {
  idle: "Ready",
  ready: "Ready",
  recording: "Recording",
  transcribing: "Transcribing",
  cleaning: "Cleaning",
  pasting: "Pasting",
  pasted: "Pasted",
  copied: "Copied",
  error: "Error",
};

export function statusLabel(state: AppState): string {
  return statusLabels[state];
}
