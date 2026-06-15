import type { FloeError } from "../types/app";

const ERROR_DOMAINS = new Set([
  "settings",
  "hotkey",
  "recording",
  "stt",
  "clipboard",
  "startAtLogin",
]);

export function parseFloeError(caught: unknown): FloeError {
  if (typeof caught === "object" && caught !== null) {
    const obj = caught as Record<string, unknown>;
    if (typeof obj.domain === "string" && ERROR_DOMAINS.has(obj.domain)) {
      return caught as FloeError;
    }
  }
  return {
    domain: "internal",
    code: "unknownError",
    message:
      typeof caught === "object" &&
      caught !== null &&
      "message" in (caught as object)
        ? String((caught as Record<string, unknown>).message)
        : "An unexpected error occurred",
  } as unknown as FloeError;
}

export function floeErrorMessage(error: FloeError): string {
  return error.message;
}

export function floeErrorCode(error: FloeError): string {
  return error.code;
}

export function isFloeErrorDomain<T extends FloeError["domain"]>(
  error: FloeError,
  domain: T,
): error is Extract<FloeError, { domain: T }> {
  return error.domain === domain;
}

export function startAtLoginErrorMessage(
  error: FloeError,
  enabling: boolean,
): string {
  if (isFloeErrorDomain(error, "startAtLogin")) {
    if (error.code === "unavailable") {
      return "Start at login unavailable";
    }
    return error.message;
  }
  return enabling
    ? "Could not enable start at login"
    : "Could not disable start at login";
}
