import { describe, expect, it } from "vitest";
import { statusLabel } from "./status";

describe("statusLabel", () => {
  it("formats setup states for the UI", () => {
    expect(statusLabel("ready")).toBe("Ready");
    expect(statusLabel("checking")).toBe("Checking");
    expect(statusLabel("recording")).toBe("Recording");
  });
});
