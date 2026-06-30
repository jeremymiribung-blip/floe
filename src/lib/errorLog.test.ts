import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  logRecoverable,
  logCritical,
  toError,
  errorMessage,
  _resetErrorLogForTests,
} from "./errorLog";

vi.mock("./tauri", () => ({
  diagLog: vi.fn(),
}));

import { diagLog } from "./tauri";

const mockedDiagLog = vi.mocked(diagLog);

beforeEach(() => {
  _resetErrorLogForTests();
  vi.clearAllMocks();
  vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("toError", () => {
  it("returns the original Error untouched (identity preserved)", () => {
    const original = new Error("boom");
    const result = toError(original);
    expect(result).toBe(original);
    expect(result.message).toBe("boom");
    expect(result.stack).toBe(original.stack);
  });

  it("preserves Error subclasses (TypeError, RangeError, ...)", () => {
    const original = new TypeError("bad type");
    const result = toError(original);
    expect(result).toBe(original);
    expect(result).toBeInstanceOf(TypeError);
  });

  it("wraps non-Error values into Error while preserving message", () => {
    const result = toError("plain string");
    expect(result).toBeInstanceOf(Error);
    expect(result.message).toBe("plain string");
  });

  it("extracts message from object-shaped errors", () => {
    const result = toError({ message: "backend said no", code: "E_BAD" });
    expect(result.message).toBe("backend said no");
  });

  it("handles null and undefined without throwing", () => {
    expect(() => toError(null)).not.toThrow();
    expect(() => toError(undefined)).not.toThrow();
    expect(toError(null)).toBeInstanceOf(Error);
    expect(toError(undefined)).toBeInstanceOf(Error);
  });

  it("handles circular objects without throwing", () => {
    const circ: Record<string, unknown> = {};
    circ.self = circ;
    expect(() => toError(circ)).not.toThrow();
  });
});

describe("errorMessage", () => {
  it("returns Error.message", () => {
    expect(errorMessage(new Error("x"))).toBe("x");
  });

  it("returns strings as-is", () => {
    expect(errorMessage("oops")).toBe("oops");
  });

  it("extracts .message from object-shaped errors", () => {
    expect(errorMessage({ message: "nope" })).toBe("nope");
  });

  it("falls back to a placeholder for unknown values", () => {
    expect(errorMessage(undefined)).toBeTruthy();
    expect(errorMessage(null)).toBeTruthy();
    expect(errorMessage(42)).toBeTruthy();
  });
});

describe("logRecoverable / logCritical", () => {
  it("emits both console.error and diagLog exactly once for a fresh call", () => {
    logRecoverable("ctx-a", new Error("e1"));

    expect(console.error).toHaveBeenCalledTimes(1);
    expect(mockedDiagLog).toHaveBeenCalledTimes(1);
    expect(mockedDiagLog).toHaveBeenCalledWith(
      expect.stringContaining("[FE][recoverable] ctx-a: e1"),
    );
  });

  it("uses [critical] tag for logCritical", () => {
    logCritical("ctx-b", new Error("e2"));

    expect(console.error).toHaveBeenCalledTimes(1);
    expect(mockedDiagLog).toHaveBeenCalledWith(
      expect.stringContaining("[FE][critical] ctx-b: e2"),
    );
  });

  it("preserves the Error object (not a stringified version) in console.error", () => {
    const err = new Error("preserved");
    logRecoverable("ctx-c", err);

    const call = (console.error as unknown as ReturnType<typeof vi.fn>).mock
      .calls[0];
    expect(call[0]).toMatch(/ctx-c/);
    expect(call[1]).toBe(err);
  });

  it("deduplicates identical (context, message) pairs within the dedupe window", () => {
    logRecoverable("ctx-dup", new Error("same"));
    logRecoverable("ctx-dup", new Error("same"));
    logRecoverable("ctx-dup", new Error("same"));

    expect(console.error).toHaveBeenCalledTimes(1);
    expect(mockedDiagLog).toHaveBeenCalledTimes(1);
  });

  it("does NOT dedupe different messages", () => {
    logRecoverable("ctx-mix", new Error("a"));
    logRecoverable("ctx-mix", new Error("b"));

    expect(console.error).toHaveBeenCalledTimes(2);
    expect(mockedDiagLog).toHaveBeenCalledTimes(2);
  });

  it("does NOT dedupe different contexts with the same message", () => {
    logRecoverable("ctx-x", new Error("same"));
    logRecoverable("ctx-y", new Error("same"));

    expect(console.error).toHaveBeenCalledTimes(2);
  });

  it("handles non-Error values without throwing", () => {
    expect(() => logRecoverable("ctx-non", "string error")).not.toThrow();
    expect(() => logRecoverable("ctx-obj", { message: "obj error" })).not.toThrow();
    expect(() => logRecoverable("ctx-null", null)).not.toThrow();
    expect(() => logRecoverable("ctx-undef", undefined)).not.toThrow();
  });

  it("emits a non-empty message even when the value has no message field", () => {
    logRecoverable("ctx-nomsg", { code: "E_X" });
    expect(mockedDiagLog).toHaveBeenCalledTimes(1);
    const message = mockedDiagLog.mock.calls[0][0] as string;
    expect(message).toContain("ctx-nomsg");
    expect(message.length).toBeGreaterThan("ctx-nomsg".length);
  });
});