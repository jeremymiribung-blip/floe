import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  saveApiKey,
  clearApiKey,
  getApiKeyStatus,
  getHotkeySettings,
  setHotkey,
  resetHotkeyToDefault,
  getStartAtLoginStatus,
  setStartAtLoginEnabled,
  getRecordingStatus,
  getDiagnosticsReport,
  updateSessionHotkeyLatency,
} from "./tauri";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockedInvoke = vi.mocked(invoke);

afterEach(() => {
  vi.clearAllMocks();
});

describe("IPC wrappers", () => {
  it("saveApiKey calls correct command", async () => {
    mockedInvoke.mockResolvedValue({ configured: true, maskedPreview: null });
    await saveApiKey("sk-abc");
    expect(mockedInvoke).toHaveBeenCalledWith("save_api_key", {
      apiKey: "sk-abc",
    });
  });

  it("clearApiKey calls correct command", async () => {
    mockedInvoke.mockResolvedValue({ configured: false, maskedPreview: null });
    await clearApiKey();
    expect(mockedInvoke).toHaveBeenCalledWith("clear_api_key");
  });

  it("getApiKeyStatus calls correct command", async () => {
    mockedInvoke.mockResolvedValue({ configured: true, maskedPreview: "sk-…" });
    await getApiKeyStatus();
    expect(mockedInvoke).toHaveBeenCalledWith("get_api_key_status");
  });

  it("getHotkeySettings calls correct command", async () => {
    mockedInvoke.mockResolvedValue({
      label: "Ctrl+Space",
      isRegistered: true,
      error: null,
    });
    await getHotkeySettings();
    expect(mockedInvoke).toHaveBeenCalledWith("get_hotkey_settings");
  });

  it("setHotkey calls correct command", async () => {
    mockedInvoke.mockResolvedValue({
      label: "Ctrl+Shift+A",
      isRegistered: true,
      error: null,
    });
    await setHotkey("Ctrl+Shift+A");
    expect(mockedInvoke).toHaveBeenCalledWith("set_hotkey", {
      accelerator: "Ctrl+Shift+A",
    });
  });

  it("resetHotkeyToDefault calls correct command", async () => {
    mockedInvoke.mockResolvedValue({
      label: "Ctrl+Space",
      isRegistered: true,
      error: null,
    });
    await resetHotkeyToDefault();
    expect(mockedInvoke).toHaveBeenCalledWith("reset_hotkey_to_default");
  });

  it("getStartAtLoginStatus calls correct command", async () => {
    mockedInvoke.mockResolvedValue({ enabled: true, available: true });
    await getStartAtLoginStatus();
    expect(mockedInvoke).toHaveBeenCalledWith("get_start_at_login_status");
  });

  it("setStartAtLoginEnabled calls correct command", async () => {
    mockedInvoke.mockResolvedValue({ enabled: true, available: true });
    await setStartAtLoginEnabled(true);
    expect(mockedInvoke).toHaveBeenCalledWith("set_start_at_login_enabled", {
      enabled: true,
    });
  });

  it("getRecordingStatus calls correct command", async () => {
    mockedInvoke.mockResolvedValue({ isRecording: false });
    await getRecordingStatus();
    expect(mockedInvoke).toHaveBeenCalledWith("get_recording_status");
  });

  it("getDiagnosticsReport calls correct command", async () => {
    mockedInvoke.mockResolvedValue({ schema_version: 1, app: "Floe" });
    await getDiagnosticsReport();
    expect(mockedInvoke).toHaveBeenCalledWith("get_diagnostics_report");
  });

  it("updateSessionHotkeyLatency calls correct command", async () => {
    mockedInvoke.mockResolvedValue(undefined);
    await updateSessionHotkeyLatency("abc123", 42);
    expect(mockedInvoke).toHaveBeenCalledWith("update_session_hotkey_latency", {
      trace_id: "abc123",
      hotkey_to_recording_start_ms: 42,
    });
  });
});
