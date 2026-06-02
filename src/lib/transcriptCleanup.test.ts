import { describe, expect, it } from "vitest";
import { cleanupTranscript } from "./transcriptCleanup";

describe("transcript cleanup", () => {
  it("capitalizes and adds final punctuation", () => {
    expect(cleanupTranscript("hallo welt")).toBe("Hallo welt.");
  });

  it("trims and collapses repeated whitespace", () => {
    expect(cleanupTranscript(" ich   teste das  ")).toBe("Ich teste das.");
  });

  it("keeps existing final punctuation", () => {
    expect(cleanupTranscript("okay danke!")).toBe("Okay danke!");
  });

  it("keeps empty input empty", () => {
    expect(cleanupTranscript("")).toBe("");
    expect(cleanupTranscript("   ")).toBe("");
  });

  it("cleans spaces before punctuation", () => {
    expect(cleanupTranscript("hallo , welt !")).toBe("Hallo, welt!");
  });

  it("normalizes spacing after punctuation where safe", () => {
    expect(cleanupTranscript("okay,danke!weiter")).toBe("Okay, danke! weiter.");
  });

  it("preserves decimal punctuation spacing", () => {
    expect(cleanupTranscript("pi ist 3.14")).toBe("Pi ist 3.14.");
  });
});
