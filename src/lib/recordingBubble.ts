import type { AppState } from "../types/app";

export function shouldShowBubble(state: AppState): boolean {
  return state === "recording";
}
