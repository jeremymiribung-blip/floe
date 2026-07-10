import type { RecordingError } from "../types/app";

const RECORDING_ALREADY_ACTIVE = "Recording is already active";
const NO_INPUT_DEVICE = "No microphone found";
const RECORDING_TOO_SHORT = "Recording too short";
const PERMISSION_DENIED_FALLBACK = "Microphone access denied";
const PERMISSION_DENIED_MACOS =
  "Open System Settings > Privacy & Security > Microphone";
const PERMISSION_DENIED_WINDOWS =
  "Go to Settings > Privacy & Security > Microphone";
const PERMISSION_DENIED_LINUX =
  "Check your system sound settings to enable microphone access";

const MICROPHONE_UNAVAILABLE_CODES: ReadonlySet<RecordingError["code"]> =
  new Set([
    "streamBuildFailed",
    "unsupportedSampleFormat",
    "deviceDisconnected",
  ]);

export function recordingErrorMessage(caught: unknown): string {
  const recordingError = caught as Partial<RecordingError>;
  if (recordingError.code === "alreadyRecording") {
    return RECORDING_ALREADY_ACTIVE;
  }
  if (recordingError.code === "permissionDenied") {
    return getPermissionDeniedMessage();
  }
  if (recordingError.code === "noInputDevice") {
    return NO_INPUT_DEVICE;
  }
  if (
    typeof recordingError.code === "string" &&
    MICROPHONE_UNAVAILABLE_CODES.has(
      recordingError.code as RecordingError["code"],
    )
  ) {
    return "Microphone unavailable";
  }
  if (recordingError.code === "emptyRecording") {
    return RECORDING_TOO_SHORT;
  }
  if (typeof recordingError.message === "string") {
    return recordingError.message;
  }
  return "Recording failed";
}

function getPermissionDeniedMessage(): string {
  if (typeof navigator === "undefined") {
    return PERMISSION_DENIED_FALLBACK;
  }
  const platform = navigator.platform.toLowerCase();
  if (platform.includes("mac")) {
    return PERMISSION_DENIED_MACOS;
  }
  if (platform.includes("win")) {
    return PERMISSION_DENIED_WINDOWS;
  }
  if (platform.includes("linux")) {
    return PERMISSION_DENIED_LINUX;
  }
  return PERMISSION_DENIED_FALLBACK;
}
