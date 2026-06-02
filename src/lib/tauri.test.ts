import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

describe("browser transcription fallback", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.resetModules();
  });

  it("rejects transcription before a recording exists", async () => {
    const { transcribeLatestRecording } = await import("./tauri");

    await expect(transcribeLatestRecording()).rejects.toMatchObject({
      code: "emptyAudio",
    });
  });

  it("returns a manual-flow mock transcript after recording stops", async () => {
    const { startRecording, stopRecording, transcribeLatestRecording } =
      await import("./tauri");

    await startRecording();
    vi.setSystemTime(2_500);
    await stopRecording();

    await expect(transcribeLatestRecording()).resolves.toEqual({
      text: "Mock transcript from the latest manual recording.",
    });
  });
});
