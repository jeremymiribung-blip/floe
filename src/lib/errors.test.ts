import { describe, expect, it } from "vitest";
import {
  parseFloeError,
  floeErrorMessage,
  floeErrorCode,
  isFloeErrorDomain,
  startAtLoginErrorMessage,
} from "./errors";

describe("parseFloeError", () => {
  it("returns a typed error for a valid recording domain", () => {
    const error = parseFloeError({
      domain: "recording",
      code: "noInputDevice",
      message: "No input device found",
    });
    expect(isFloeErrorDomain(error, "recording")).toBe(true);
    expect(error.code).toBe("noInputDevice");
    expect(error.message).toBe("No input device found");
  });

  it("returns a typed error for a valid stt domain", () => {
    const error = parseFloeError({
      domain: "stt",
      code: "rateLimit",
      message: "Rate limited",
      retryCount: 2,
    });
    expect(isFloeErrorDomain(error, "stt")).toBe(true);
    expect(error.code).toBe("rateLimit");
  });

  it("returns a typed error for a valid clipboard domain", () => {
    const error = parseFloeError({
      domain: "clipboard",
      code: "clipboardUnavailable",
      message: "Clipboard unavailable",
    });
    expect(isFloeErrorDomain(error, "clipboard")).toBe(true);
  });

  it("returns a typed error for a valid settings domain", () => {
    const error = parseFloeError({
      domain: "settings",
      code: "invalidGroqApiKey",
      message: "Invalid API key",
    });
    expect(isFloeErrorDomain(error, "settings")).toBe(true);
  });

  it("returns a typed error for a valid hotkey domain", () => {
    const error = parseFloeError({
      domain: "hotkey",
      code: "alreadyInUse",
      message: "Hotkey already in use",
    });
    expect(isFloeErrorDomain(error, "hotkey")).toBe(true);
  });

  it("returns a typed error for a valid startAtLogin domain", () => {
    const error = parseFloeError({
      domain: "startAtLogin",
      code: "unavailable",
      message: "Start at login unavailable",
    });
    expect(isFloeErrorDomain(error, "startAtLogin")).toBe(true);
  });

  it("returns internal error for null", () => {
    const error = parseFloeError(null);
    expect(error.domain).toBe("internal");
    expect(error.code).toBe("unknownError");
    expect(error.message).toBe("An unexpected error occurred");
  });

  it("returns internal error for undefined", () => {
    const error = parseFloeError(undefined);
    expect(error.domain).toBe("internal");
    expect(error.code).toBe("unknownError");
  });

  it("returns internal error for a primitive string", () => {
    const error = parseFloeError("something went wrong");
    expect(error.domain).toBe("internal");
  });

  it("returns internal error for a number", () => {
    const error = parseFloeError(42);
    expect(error.domain).toBe("internal");
  });

  it("returns internal error for an unknown domain", () => {
    const error = parseFloeError({
      domain: "unknownDomain",
      code: "someCode",
      message: "some message",
    });
    expect(error.domain).toBe("internal");
    expect(error.code).toBe("unknownError");
  });

  it("preserves the message from a domain-less object with message", () => {
    const error = parseFloeError({
      message: "preserved message",
    });
    expect(error.domain).toBe("internal");
    expect(error.code).toBe("unknownError");
    expect(error.message).toBe("preserved message");
  });

  it("returns internal for a plain Error instance", () => {
    const error = parseFloeError(new Error("plain error"));
    expect(error.domain).toBe("internal");
    expect(error.code).toBe("unknownError");
    expect(error.message).toBe("plain error");
  });

  it("passes through extra fields on the object", () => {
    const error = parseFloeError({
      domain: "recording",
      code: "alreadyRecording",
      message: "Already recording",
      extraField: "present",
    });
    expect(isFloeErrorDomain(error, "recording")).toBe(true);
    expect((error as { extraField?: string }).extraField).toBe("present");
  });
});

describe("floeErrorMessage", () => {
  it("returns the message from a typed error", () => {
    const error = parseFloeError({
      domain: "recording",
      code: "permissionDenied",
      message: "Permission denied",
    });
    expect(floeErrorMessage(error)).toBe("Permission denied");
  });

  it("returns the message from an internal error", () => {
    const error = parseFloeError(null);
    expect(floeErrorMessage(error)).toBe("An unexpected error occurred");
  });
});

describe("floeErrorCode", () => {
  it("returns the code from a typed error", () => {
    const error = parseFloeError({
      domain: "clipboard",
      code: "pasteUnavailable",
      message: "Paste failed",
    });
    expect(floeErrorCode(error)).toBe("pasteUnavailable");
  });

  it("returns unknownError for internal errors", () => {
    const error = parseFloeError("unknown");
    expect(floeErrorCode(error)).toBe("unknownError");
  });
});

describe("isFloeErrorDomain", () => {
  it("returns true when domain matches", () => {
    const error = parseFloeError({
      domain: "stt",
      code: "timeout",
      message: "Request timed out",
    });
    expect(isFloeErrorDomain(error, "stt")).toBe(true);
  });

  it("returns false when domain does not match", () => {
    const error = parseFloeError({
      domain: "stt",
      code: "timeout",
      message: "Request timed out",
    });
    expect(isFloeErrorDomain(error, "recording")).toBe(false);
  });

  it("narrows the type for TypeScript when domain matches", () => {
    const error = parseFloeError({
      domain: "hotkey",
      code: "registrationFailed",
      message: "Registration failed",
    });
    if (isFloeErrorDomain(error, "hotkey")) {
      expect(error.code).toBe("registrationFailed");
    }
  });
});

describe("startAtLoginErrorMessage", () => {
  it("returns message from startAtLogin error code", () => {
    const error = parseFloeError({
      domain: "startAtLogin",
      code: "unavailable",
      message: "Start at login unavailable",
    });
    expect(startAtLoginErrorMessage(error, false)).toBe(
      "Start at login unavailable",
    );
  });

  it("returns message from startAtLogin error message", () => {
    const error = parseFloeError({
      domain: "startAtLogin",
      code: "enableFailed",
      message: "Could not enable start at login",
    });
    expect(startAtLoginErrorMessage(error, true)).toBe(
      "Could not enable start at login",
    );
  });

  it("falls back for non-startAtLogin errors", () => {
    const error = parseFloeError({
      domain: "settings",
      code: "invalidGroqApiKey",
      message: "Invalid API key",
    });
    expect(startAtLoginErrorMessage(error, true)).toBe(
      "Could not enable start at login",
    );
    expect(startAtLoginErrorMessage(error, false)).toBe(
      "Could not disable start at login",
    );
  });
});
