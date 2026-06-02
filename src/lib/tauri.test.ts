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

describe("browser settings fallback", () => {
  afterEach(() => {
    vi.resetModules();
  });

  it("uses a customization-ready default hotkey", async () => {
    const { getAppSettings } = await import("./tauri");

    await expect(getAppSettings()).resolves.toEqual({
      hotkey: {
        accelerator: "Ctrl+Space",
        label: "Ctrl+Space",
      },
    });
  });

  it("saves trimmed hotkey settings", async () => {
    const { getAppSettings, saveAppSettings } = await import("./tauri");

    await saveAppSettings({
      hotkey: {
        accelerator: "  Ctrl+Space  ",
        label: "  Ctrl+Space  ",
      },
    });

    await expect(getAppSettings()).resolves.toEqual({
      hotkey: {
        accelerator: "Ctrl+Space",
        label: "Ctrl+Space",
      },
    });
  });
});

describe("browser clipboard fallback", () => {
  afterEach(() => {
    vi.resetModules();
  });

  it("copies text into browser test clipboard state", async () => {
    const { copyTextToClipboard, getBrowserClipboardTextForTest } =
      await import("./tauri");

    await copyTextToClipboard("copied text");

    expect(getBrowserClipboardTextForTest()).toBe("copied text");
  });

  it("paste writes text into browser test clipboard state", async () => {
    const { getBrowserClipboardTextForTest, pasteText } =
      await import("./tauri");

    await pasteText("pasted text");

    expect(getBrowserClipboardTextForTest()).toBe("pasted text");
  });

  it("pasteClipboard leaves existing browser clipboard text alone", async () => {
    const {
      copyTextToClipboard,
      getBrowserClipboardTextForTest,
      pasteClipboard,
    } = await import("./tauri");

    await copyTextToClipboard("already copied");
    await pasteClipboard();

    expect(getBrowserClipboardTextForTest()).toBe("already copied");
  });
});
