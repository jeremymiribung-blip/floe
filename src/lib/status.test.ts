import { describe, expect, it } from "vitest";
import { statusLabel } from "./status";

describe("statusLabel", () => {
  it("formats push-to-talk states for the UI", () => {
    expect(statusLabel("idle")).toBe("Idle");
    expect(statusLabel("capturing_hotkey")).toBe("Capturing hotkey");
    expect(statusLabel("recording")).toBe("Recording");
    expect(statusLabel("transcribing")).toBe("Transcribing");
    expect(statusLabel("cleaning")).toBe("Cleaning");
    expect(statusLabel("pasting")).toBe("Pasting");
    expect(statusLabel("pasted")).toBe("Pasted");
    expect(statusLabel("error")).toBe("Needs attention");
  });
});
