import { getCurrentWindow } from "@tauri-apps/api/window";
import { logRecoverable } from "./errorLog";

export const OVERLAY_WINDOW_LABEL: string = "recording-bubble";

export function getOverlayWindowLabel(): string {
  return OVERLAY_WINDOW_LABEL;
}

export function isOverlayWindow(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  const internals = (window as unknown as { __TAURI_INTERNALS__?: unknown })
    .__TAURI_INTERNALS__;
  if (!internals) {
    return false;
  }
  try {
    return getCurrentWindow().label === OVERLAY_WINDOW_LABEL;
  } catch (err) {
    logRecoverable("isOverlayWindow probe", err);
    return false;
  }
}
