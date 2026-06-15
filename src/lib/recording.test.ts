import { describe, expect, it } from "vitest";
import { formatDurationMs } from "./recording";

describe("recording helpers", () => {
  it("formats durations for recording metadata", () => {
    expect(formatDurationMs(950)).toBe("0s");
    expect(formatDurationMs(61_000)).toBe("1m 01s");
  });
});
