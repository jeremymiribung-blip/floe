import type { AppState } from "../types/app";

const statusLabels: Record<AppState, string> = {
  loading: "Loading",
  ready: "Ready",
  checking: "Checking",
  error: "Needs attention",
};

export function statusLabel(state: AppState): string {
  return statusLabels[state];
}
