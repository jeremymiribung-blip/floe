import type { RecordingError } from "../types/app";

export const MICROPHONE_UNAVAILABLE = "Microphone unavailable";
export const RECORDING_ALREADY_ACTIVE = "Recording already active";
export const RECORDING_TOO_SHORT = "Recording too short";

const MICROPHONE_UNAVAILABLE_CODES: ReadonlySet<RecordingError["code"]> =
  new Set([
    "noInputDevice",
    "permissionDenied",
    "streamBuildFailed",
    "unsupportedSampleFormat",
    "deviceDisconnected",
  ]);

export function recordingErrorMessage(caught: unknown): string {
  const recordingError = caught as Partial<RecordingError>;
  if (recordingError.code === "alreadyRecording") {
    return RECORDING_ALREADY_ACTIVE;
  }
  if (
    typeof recordingError.code === "string" &&
    MICROPHONE_UNAVAILABLE_CODES.has(
      recordingError.code as RecordingError["code"],
    )
  ) {
    return MICROPHONE_UNAVAILABLE;
  }
  if (recordingError.code === "emptyRecording") {
    return RECORDING_TOO_SHORT;
  }
  if (typeof recordingError.message === "string") {
    return recordingError.message;
  }
  return "Recording failed";
}
