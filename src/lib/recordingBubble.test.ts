import { describe, expect, it } from "vitest";
import { shouldShowBubble } from "./recordingBubble";
import type { AppState } from "../types/app";

describe("shouldShowBubble", () => {
  const allStates: AppState[] = [
    "idle",
    "ready",
    "recording",
    "transcribing",
    "cleaning",
    "pasting",
    "pasted",
    "copied",
    "error",
  ];

  it.each(allStates)("is true only for recording (%s)", (state) => {
    expect(shouldShowBubble(state)).toBe(state === "recording");
  });

  it("hides for transcribing", () => {
    expect(shouldShowBubble("transcribing")).toBe(false);
  });

  it("hides for cleaning", () => {
    expect(shouldShowBubble("cleaning")).toBe(false);
  });

  it("hides for pasting", () => {
    expect(shouldShowBubble("pasting")).toBe(false);
  });

  it("hides for pasted", () => {
    expect(shouldShowBubble("pasted")).toBe(false);
  });

  it("hides for copied", () => {
    expect(shouldShowBubble("copied")).toBe(false);
  });

  it("hides for error", () => {
    expect(shouldShowBubble("error")).toBe(false);
  });

  it("hides for idle and ready", () => {
    expect(shouldShowBubble("idle")).toBe(false);
    expect(shouldShowBubble("ready")).toBe(false);
  });
});
