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

  it("rejects empty browser recordings without creating a latest recording", async () => {
    const { getLatestRecordingInfo, startRecording, stopRecording } =
      await import("./tauri");

    await startRecording();

    await expect(stopRecording()).rejects.toMatchObject({
      code: "emptyRecording",
    });
    await expect(getLatestRecordingInfo()).resolves.toBeNull();
  });

  it("rejects overlapping browser recording starts", async () => {
    const { startRecording } = await import("./tauri");

    await startRecording();

    await expect(startRecording()).rejects.toMatchObject({
      code: "alreadyRecording",
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
        accelerator: "Control+Shift+Space",
        label: "Control+Shift+Space",
      },
    });
  });

  it("saves trimmed hotkey settings", async () => {
    const { getAppSettings, saveAppSettings } = await import("./tauri");

    await saveAppSettings({
      hotkey: {
        accelerator: "  Control+Shift+KeyA  ",
        label: "  Control+Shift+A  ",
      },
    });

    await expect(getAppSettings()).resolves.toEqual({
      hotkey: {
        accelerator: "Control+Shift+KeyA",
        label: "Control+Shift+A",
      },
    });
  });

  it("changes and resets browser hotkey settings", async () => {
    const { getHotkeySettings, resetHotkeyToDefault, setHotkey } =
      await import("./tauri");

    await expect(setHotkey("Control+Shift+KeyB")).resolves.toMatchObject({
      configured: {
        accelerator: "Control+Shift+KeyB",
        label: "Control+Shift+B",
      },
      isRegistered: true,
    });
    await expect(getHotkeySettings()).resolves.toMatchObject({
      registered: {
        accelerator: "Control+Shift+KeyB",
        label: "Control+Shift+B",
      },
    });
    await expect(resetHotkeyToDefault()).resolves.toMatchObject({
      configured: {
        accelerator: "Control+Shift+Space",
        label: "Control+Shift+Space",
      },
    });
  });

  it("gets and updates browser start at login status", async () => {
    const { getStartAtLoginStatus, setStartAtLoginEnabled } =
      await import("./tauri");

    await expect(getStartAtLoginStatus()).resolves.toEqual({
      enabled: false,
      available: true,
    });
    await expect(setStartAtLoginEnabled(true)).resolves.toEqual({
      enabled: true,
      available: true,
    });
    await expect(getStartAtLoginStatus()).resolves.toEqual({
      enabled: true,
      available: true,
    });
    await expect(setStartAtLoginEnabled(false)).resolves.toEqual({
      enabled: false,
      available: true,
    });
  });

  it("masks and clears browser Groq API key status without exposing the full key", async () => {
    const { clearGroqApiKey, getGroqApiKeyStatus, saveGroqApiKey } =
      await import("./tauri");

    await expect(saveGroqApiKey("  gsk_12345678abcd  ")).resolves.toEqual({
      configured: true,
      maskedPreview: "gsk_...abcd",
    });
    await expect(getGroqApiKeyStatus()).resolves.toEqual({
      configured: true,
      maskedPreview: "gsk_...abcd",
    });
    await expect(clearGroqApiKey()).resolves.toEqual({
      configured: false,
      maskedPreview: null,
    });
  });

  it("uses a generic browser mask for short Groq API keys", async () => {
    const { saveGroqApiKey } = await import("./tauri");

    await expect(saveGroqApiKey("short")).resolves.toEqual({
      configured: true,
      maskedPreview: "Configured key",
    });
  });

  it("masks and clears browser Cerebras API key status without exposing the full key", async () => {
    const { clearCerebrasApiKey, getCerebrasApiKeyStatus, saveCerebrasApiKey } =
      await import("./tauri");

    await expect(saveCerebrasApiKey("  csk_12345678abcd  ")).resolves.toEqual({
      configured: true,
      maskedPreview: "csk_...abcd",
    });
    await expect(getCerebrasApiKeyStatus()).resolves.toEqual({
      configured: true,
      maskedPreview: "csk_...abcd",
    });
    await expect(clearCerebrasApiKey()).resolves.toEqual({
      configured: false,
      maskedPreview: null,
    });
  });

  it("browser cleanupTranscript returns the raw transcript without invoking cleanup", async () => {
    const { cleanupTranscript } = await import("./tauri");

    await expect(cleanupTranscript("raw text")).resolves.toEqual({
      text: "raw text",
    });
  });

  it("default browser app settings have no cleanupMode field", async () => {
    const { getAppSettings } = await import("./tauri");

    const settings = await getAppSettings();

    expect(settings).not.toHaveProperty("cleanupMode");
    expect(JSON.stringify(settings)).not.toContain("cleanupMode");
  });

  it("clearCerebrasApiKey does not touch any cleanup mode in the browser", async () => {
    const { clearCerebrasApiKey, getAppSettings, saveCerebrasApiKey } =
      await import("./tauri");

    await saveCerebrasApiKey("csk_12345678abcd");
    await clearCerebrasApiKey();
    const settings = await getAppSettings();

    expect(settings).not.toHaveProperty("cleanupMode");
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
