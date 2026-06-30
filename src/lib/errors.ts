import type { FloeError } from "../types/app";

const ERROR_DOMAINS = new Set([
  "settings",
  "hotkey",
  "recording",
  "stt",
  "clipboard",
  "startAtLogin",
]);

function extractMessage(caught: unknown): string {
  if (typeof caught === "object" && caught !== null && "message" in caught) {
    const msg = (caught as Record<string, string>).message;
    if (typeof msg === "string") return msg;
  }
  if (typeof caught === "string") return caught;
  return "An unexpected error occurred";
}

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
    message: extractMessage(caught),
  };
}

export function floeErrorCode(error: FloeError): string {
  return error.code;
}
