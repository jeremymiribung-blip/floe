import { describe, expect, it } from "vitest";
import { statusLabel } from "./status";

describe("statusLabel", () => {
  it("formats push-to-talk states for the UI", () => {
    expect(statusLabel("idle")).toBe("Ready");
    expect(statusLabel("ready")).toBe("Ready");
    expect(statusLabel("recording")).toBe("Recording");
    expect(statusLabel("transcribing")).toBe("Transcribing");
    expect(statusLabel("cleaning")).toBe("Cleaning");
    expect(statusLabel("pasting")).toBe("Pasting");
    expect(statusLabel("pasted")).toBe("Pasted");
    expect(statusLabel("copied")).toBe("Copied to clipboard");
    expect(statusLabel("error")).toBe("Error");
  });
});
