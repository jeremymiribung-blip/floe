import type { FloeError } from "../types/app";

/**
 * Normalizes any error (from backend IPC or JS) into a consistent
 * { domain, code, message } shape for safe use in UI components.
 * All backend errors follow { domain: string, code: string, message: string }.
 */
export function normalizeError(caught: unknown): FloeError {
  if (typeof caught === "object" && caught !== null) {
    const obj = caught as Record<string, unknown>;

    const domain = typeof obj.domain === "string" ? obj.domain : "internal";
    const code = typeof obj.code === "string" ? obj.code : "unknownError";
    const message =
      typeof obj.message === "string"
        ? obj.message
        : extractFallbackMessage(caught);

    return { domain, code, message } as FloeError;
  }

  if (typeof caught === "string") {
    return {
      domain: "internal",
      code: "unknownError",
      message: caught,
    } as FloeError;
  }

  return {
    domain: "internal",
    code: "unknownError",
    message: "An unexpected error occurred",
  } as FloeError;
}

function extractFallbackMessage(caught: unknown): string {
  if (typeof caught === "object" && caught !== null && "message" in caught) {
    const msg = (caught as Record<string, unknown>).message;
    if (typeof msg === "string") return msg;
  }
  return "An unexpected error occurred";
}

// Re-export for convenience if callers used the old parseFloeError
export { normalizeError as parseFloeError };
