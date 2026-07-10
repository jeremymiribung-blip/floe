// ─────────────────────────────────────────────────────────────────────────────
// Frontend error logging utilities
//
// Centralized helpers so we never discard Error objects, never log duplicates,
// and never sprinkle raw `console.error` / `catch {}` across the codebase.
//
// Two-tier classification:
//   • logRecoverable — failures that don't break the app (telemetry, overlays).
//   • logCritical    — failures that affect setup, recording, hotkeys, or the
//                      backend pipeline. The caller is responsible for also
//                      surfacing these into application state (store, toast,
//                      banner). The helper still emits console + diag log so
//                      developers see the failure.
//
// Both helpers deduplicate identical (context + message) pairs that occur in
// rapid succession (e.g., a Tauri window-show failing every animation frame).
// ─────────────────────────────────────────────────────────────────────────────

import { diagLog } from "./tauri";

const DEDUPE_WINDOW_MS = 1_500;
const recentSignatures = new Map<string, number>();

/**
 * Coerce any thrown value into an Error while preserving the original.
 *
 * - Error objects are returned untouched (identity preserved, original
 *   message/name/stack/cause intact).
 * - Non-Error values become `new Error(String(value))` with `.cause` pointing
 *   at the original value when it's an object. This guarantees we never
 *   lose the underlying object reference.
 */
export function toError(value: unknown): Error {
  if (value instanceof Error) {
    return value;
  }
  if (typeof value === "string") {
    return new Error(value);
  }
  try {
    const message =
      typeof value === "object" && value !== null && "message" in value
        ? String((value as { message: unknown }).message)
        : safeStringify(value);
    const error = new Error(message);
    if (typeof value === "object" && value !== null) {
      Object.defineProperty(error, "cause", {
        value,
        enumerable: false,
        writable: false,
        configurable: false,
      });
    }
    return error;
  } catch {
    return new Error("Unknown error");
  }
}

/** Extract a printable message from any thrown value. */
export function errorMessage(value: unknown): string {
  if (value instanceof Error) {
    return value.message || value.name || "Error";
  }
  if (typeof value === "string" && value.length > 0) return value;
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (
    typeof value === "object" &&
    value !== null &&
    "message" in value &&
    typeof (value as { message: unknown }).message === "string" &&
    ((value as { message: string }).message ?? "").length > 0
  ) {
    return (value as { message: string }).message;
  }
  try {
    const result = safeStringify(value);
    if (result && result.length > 0) return result;
  } catch {
    // fall through
  }
  return "Unknown error";
}

function safeStringify(value: unknown): string {
  try {
    const result = JSON.stringify(value);
    if (typeof result === "string" && result.length > 0) return result;
    return Object.prototype.toString.call(value);
  } catch {
    return Object.prototype.toString.call(value);
  }
}

function shouldEmit(context: string, message: string): boolean {
  const key = `${context}::${message}`;
  const now = Date.now();
  const last = recentSignatures.get(key);
  if (last !== undefined && now - last < DEDUPE_WINDOW_MS) {
    return false;
  }
  recentSignatures.set(key, now);
  if (recentSignatures.size > 128) {
    const cutoff = now - DEDUPE_WINDOW_MS * 4;
    for (const [k, t] of recentSignatures) {
      if (t < cutoff) recentSignatures.delete(k);
    }
    if (recentSignatures.size > 128) {
      const overflow = recentSignatures.size - 128;
      const keysToDrop = Array.from(recentSignatures.keys()).slice(0, overflow);
      for (const k of keysToDrop) recentSignatures.delete(k);
    }
  }
  return true;
}

function emit(
  context: string,
  err: unknown,
  tag: "recoverable" | "critical",
): void {
  const error = toError(err);
  const message = errorMessage(err);
  if (!shouldEmit(context, message)) return;
  console.error(`[Floe][${tag}] ${context}:`, error);
  diagLog(`[FE][${tag}] ${context}: ${message}`);
}

/** Log a failure that does not break the user-visible feature. */
export function logRecoverable(context: string, err: unknown): void {
  emit(context, err, "recoverable");
}

/**
 * Log a failure that should be surfaced in the UI. Callers are still
 * responsible for writing the failure into application state — this helper
 * only ensures the error reaches developers via console + diagnostics log.
 */
export function logCritical(context: string, err: unknown): void {
  emit(context, err, "critical");
}

/**
 * Detect whether an error originated from a failed keychain save
 * (`SettingsError.code === "secretStoreUnavailable"`).
 *
 * Rust serializes `Result<_, SettingsError>` rejections as plain
 * `{ domain, code, message }` objects, so the frontend can branch on the
 * structured code without relying on string-matching error messages.
 */
export function isKeychainError(err: unknown): boolean {
  if (typeof err !== "object" || err === null) return false;
  const code = (err as { code?: unknown }).code;
  return code === "secretStoreUnavailable";
}

/** User-facing message for a keychain storage failure. */
export const KEYCHAIN_UNAVAILABLE_MESSAGE =
  "Could not save your API key: your system’s keychain is unavailable. " +
  "Check that your OS keychain is unlocked (or that no other app is locking it) and try again.";

/** Test-only: clear the dedupe ring. */
export function _resetErrorLogForTests(): void {
  recentSignatures.clear();
}
