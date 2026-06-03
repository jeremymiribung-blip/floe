import { describe, expect, it } from "vitest";
import { captureHotkey, type HotkeyCaptureEvent } from "./hotkeyCapture";

function event(overrides: Partial<HotkeyCaptureEvent>): HotkeyCaptureEvent {
  return {
    altKey: false,
    code: "KeyA",
    ctrlKey: true,
    key: "a",
    metaKey: false,
    repeat: false,
    shiftKey: true,
    ...overrides,
  };
}

describe("captureHotkey", () => {
  it("captures a valid shortcut from keyboard event data", () => {
    expect(captureHotkey(event({ code: "Space", key: " " }))).toEqual({
      accelerator: "Control+Shift+Space",
      label: "Control+Shift+Space",
    });
  });

  it("captures letter keys with code names for backend parsing", () => {
    expect(captureHotkey(event({ code: "KeyB", key: "b" }))).toEqual({
      accelerator: "Control+Shift+KeyB",
      label: "Control+Shift+B",
    });
  });

  it("rejects repeated keydown events", () => {
    expect(() => captureHotkey(event({ repeat: true }))).toThrow(
      "Hold one shortcut at a time.",
    );
  });

  it("rejects modifier-only shortcuts", () => {
    expect(() =>
      captureHotkey(event({ code: "ShiftLeft", key: "Shift" })),
    ).toThrow("Press a key with at least two modifier keys.");
  });

  it("rejects shortcuts with too few modifiers", () => {
    expect(() =>
      captureHotkey(event({ ctrlKey: true, shiftKey: false })),
    ).toThrow("Use at least two modifier keys.");
  });
});
