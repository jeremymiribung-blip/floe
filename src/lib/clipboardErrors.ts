import type { ClipboardError } from "../types/app";

const CLIPBOARD_UNAVAILABLE = "Clipboard access unavailable";
const PASTE_FAILED = "Failed to paste from clipboard";

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
