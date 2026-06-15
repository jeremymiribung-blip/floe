import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Re-import after clearing state
async function freshImports() {
  return await import("./tauri");
}

describe("tauri runtime detection", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("detects Tauri runtime when __TAURI_INTERNALS__ is present", async () => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    const { isTauriRuntime } = await freshImports();
    expect(isTauriRuntime()).toBe(true);
  });

  it("detects browser runtime when __TAURI_INTERNALS__ is absent", async () => {
    vi.stubGlobal("__TAURI_INTERNALS__", undefined);
    const { isTauriRuntime } = await freshImports();
    expect(isTauriRuntime()).toBe(false);
  });
});

describe("command rejection in browser mode", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    vi.stubGlobal("__TAURI_INTERNALS__", undefined);
  });

  it("saveApiKey rejects in browser mode", async () => {
    const { saveApiKey } = await freshImports();
    await expect(saveApiKey("test-key")).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("clearApiKey rejects in browser mode", async () => {
    const { clearApiKey } = await freshImports();
    await expect(clearApiKey()).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("getApiKeyStatus rejects in browser mode", async () => {
    const { getApiKeyStatus } = await freshImports();
    await expect(getApiKeyStatus()).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("getHotkeySettings rejects in browser mode", async () => {
    const { getHotkeySettings } = await freshImports();
    await expect(getHotkeySettings()).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("setHotkey rejects in browser mode", async () => {
    const { setHotkey } = await freshImports();
    await expect(setHotkey("Control+Space")).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("resetHotkeyToDefault rejects in browser mode", async () => {
    const { resetHotkeyToDefault } = await freshImports();
    await expect(resetHotkeyToDefault()).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("startRecording rejects in browser mode", async () => {
    const { startRecording } = await freshImports();
    await expect(startRecording()).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("stopRecording rejects in browser mode", async () => {
    const { stopRecording } = await freshImports();
    await expect(stopRecording()).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("transcribeLatestRecording rejects in browser mode", async () => {
    const { transcribeLatestRecording } = await freshImports();
    await expect(transcribeLatestRecording()).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("cleanupTranscript rejects in browser mode", async () => {
    const { cleanupTranscript } = await freshImports();
    await expect(cleanupTranscript("test")).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("getStartAtLoginStatus rejects in browser mode", async () => {
    const { getStartAtLoginStatus } = await freshImports();
    await expect(getStartAtLoginStatus()).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("setStartAtLoginEnabled rejects in browser mode", async () => {
    const { setStartAtLoginEnabled } = await freshImports();
    await expect(setStartAtLoginEnabled(true)).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("copyTextToClipboard rejects in browser mode", async () => {
    const { copyTextToClipboard } = await freshImports();
    await expect(copyTextToClipboard("test")).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });

  it("pasteClipboard rejects in browser mode", async () => {
    const { pasteClipboard } = await freshImports();
    await expect(pasteClipboard()).rejects.toThrow(
      "only available in the Tauri runtime",
    );
  });
});

describe("bubble commands in browser mode", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    vi.stubGlobal("__TAURI_INTERNALS__", undefined);
  });

  it("bubbleShow resolves silently in browser mode", async () => {
    const { bubbleShow } = await freshImports();
    await expect(bubbleShow()).resolves.toBeUndefined();
  });

  it("bubbleHide resolves silently in browser mode", async () => {
    const { bubbleHide } = await freshImports();
    await expect(bubbleHide()).resolves.toBeUndefined();
  });
});

describe("diagLog in browser mode", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    vi.stubGlobal("__TAURI_INTERNALS__", undefined);
  });

  it("diagLog does not throw in browser mode", async () => {
    const { diagLog } = await freshImports();
    expect(() => diagLog("test log")).not.toThrow();
  });
});
