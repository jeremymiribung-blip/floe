import type { FloeError } from "../types/app";
import { isFloeErrorDomain } from "./errors";

export const CLIPBOARD_UNAVAILABLE = "Clipboard unavailable";
export const PASTE_FAILED = "Paste failed";

export function clipboardErrorMessage(error: FloeError): string {
  if (isFloeErrorDomain(error, "clipboard")) {
    if (error.code === "clipboardUnavailable") {
      return CLIPBOARD_UNAVAILABLE;
    }
    if (error.code === "pasteUnavailable") {
      return PASTE_FAILED;
    }
    return error.message;
  }
  return CLIPBOARD_UNAVAILABLE;
}
