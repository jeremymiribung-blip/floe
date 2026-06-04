import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  captureHotkey,
  isMacLikePlatform,
  type HotkeyCaptureEvent,
} from "./hotkeyCapture";

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

describe("captureHotkey on non-mac platforms", () => {
  beforeEach(() => {
    vi.spyOn(window.navigator, "platform", "get").mockReturnValue(
      "Win32" as unknown as string,
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("captures the Windows default Control+Space", () => {
    expect(
      captureHotkey(event({ code: "Space", key: " ", shiftKey: false })),
    ).toEqual({
      accelerator: "Control+Space",
      label: "Ctrl + Space",
    });
  });

  it("captures a multi-modifier shortcut like Control+Shift+Space", () => {
    expect(captureHotkey(event({ code: "Space", key: " " }))).toEqual({
      accelerator: "Control+Shift+Space",
      label: "Ctrl + Shift + Space",
    });
  });

  it("captures letter keys with code names for backend parsing", () => {
    expect(
      captureHotkey(event({ code: "KeyB", key: "b", shiftKey: false })),
    ).toEqual({
      accelerator: "Control+KeyB",
      label: "Ctrl + B",
    });
  });

  it("captures Alt+Space", () => {
    expect(
      captureHotkey(
        event({
          altKey: true,
          code: "Space",
          key: " ",
          ctrlKey: false,
          shiftKey: false,
        }),
      ),
    ).toEqual({
      accelerator: "Alt+Space",
      label: "Alt + Space",
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
    ).toThrow("Press a key with at least one modifier.");
  });

  it("rejects plain Space without a modifier", () => {
    expect(() =>
      captureHotkey(
        event({
          code: "Space",
          key: " ",
          ctrlKey: false,
          altKey: false,
          metaKey: false,
          shiftKey: false,
        }),
      ),
    ).toThrow("Press a key with at least one modifier.");
  });

  it("rejects an unsupported key", () => {
    expect(() =>
      captureHotkey(event({ code: "UnknownKey", key: "UnknownKey" })),
    ).toThrow("This shortcut is not supported.");
  });
});

describe("captureHotkey on macOS", () => {
  beforeEach(() => {
    vi.spyOn(window.navigator, "platform", "get").mockReturnValue(
      "MacIntel" as unknown as string,
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("captures Alt+Space as Option + Space", () => {
    expect(
      captureHotkey(
        event({
          altKey: true,
          code: "Space",
          key: " ",
          ctrlKey: false,
          shiftKey: false,
        }),
      ),
    ).toEqual({
      accelerator: "Alt+Space",
      label: "Option + Space",
    });
  });

  it("captures Control as Control on macOS", () => {
    expect(
      captureHotkey(event({ code: "Space", key: " ", shiftKey: false })),
    ).toEqual({
      accelerator: "Control+Space",
      label: "Control + Space",
    });
  });
});

describe("isMacLikePlatform", () => {
  it("detects macOS", () => {
    vi.spyOn(window.navigator, "platform", "get").mockReturnValue(
      "MacIntel" as unknown as string,
    );
    expect(isMacLikePlatform()).toBe(true);
  });

  it("detects Windows", () => {
    vi.spyOn(window.navigator, "platform", "get").mockReturnValue(
      "Win32" as unknown as string,
    );
    expect(isMacLikePlatform()).toBe(false);
  });
});
