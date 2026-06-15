import type { FloeError } from "../types/app";
import { isFloeErrorDomain } from "./errors";

export const MICROPHONE_UNAVAILABLE = "Microphone unavailable";
export const RECORDING_ALREADY_ACTIVE = "Recording already active";
export const RECORDING_TOO_SHORT = "Recording too short";

const MICROPHONE_UNAVAILABLE_CODES: ReadonlySet<string> = new Set([
  "noInputDevice",
  "permissionDenied",
  "streamBuildFailed",
  "unsupportedSampleFormat",
  "deviceDisconnected",
]);

export function recordingErrorMessage(error: FloeError): string {
  if (isFloeErrorDomain(error, "recording")) {
    if (error.code === "alreadyRecording") {
      return RECORDING_ALREADY_ACTIVE;
    }
    if (MICROPHONE_UNAVAILABLE_CODES.has(error.code)) {
      return MICROPHONE_UNAVAILABLE;
    }
    if (error.code === "emptyRecording") {
      return RECORDING_TOO_SHORT;
    }
    return error.message;
  }
  return "Recording failed";
}
