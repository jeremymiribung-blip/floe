import type { ClipboardError } from "../types/app";

export const CLIPBOARD_UNAVAILABLE = "Clipboard unavailable";
export const PASTE_FAILED = "Paste failed";

export function clipboardErrorMessage(caught: unknown): string {
  const clipboardError = caught as Partial<ClipboardError>;
  if (clipboardError.code === "clipboardUnavailable") {
    return CLIPBOARD_UNAVAILABLE;
  }
  if (clipboardError.code === "pasteUnavailable") {
    return PASTE_FAILED;
  }
  if (typeof clipboardError.message === "string") {
    return clipboardError.message;
  }
  return CLIPBOARD_UNAVAILABLE;
}
