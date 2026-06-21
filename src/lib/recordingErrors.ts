import type { RecordingError } from "../types/app";

export const RECORDING_ALREADY_ACTIVE = "Recording already active";
export const RECORDING_TOO_SHORT = "Recording too short";
export const NO_INPUT_DEVICE = "No active microphone found. Please check your connections.";

export const PERMISSION_DENIED_MACOS =
  "Microphone access denied. Please allow Floe in System Settings > Privacy & Security > Microphone.";
export const PERMISSION_DENIED_WINDOWS =
  "Microphone access denied. Please enable it in Windows Settings > Privacy > Microphone.";
export const PERMISSION_DENIED_LINUX =
  "Please check your system microphone permissions.";
export const PERMISSION_DENIED_FALLBACK =
  "Microphone access was denied. Please check your system privacy settings.";

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

