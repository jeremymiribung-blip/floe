import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";
import type { AppState } from "./types/app";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    show: vi.fn(() => Promise.resolve()),
    hide: vi.fn(() => Promise.resolve()),
    setFocus: vi.fn(() => Promise.resolve()),
  })),
}));

vi.mock("./lib/tauri", () => ({
  isTauriRuntime: () => true,
  getHotkeySettings: () =>
    Promise.resolve({ label: "Ctrl+Space", isRegistered: true, error: null }),
  getApiKeyStatus: () =>
    Promise.resolve({ configured: false, maskedPreview: null }),
  diagLog: vi.fn(),
  startRecording: vi.fn(() => Promise.resolve()),
  stopRecording: vi.fn(() => Promise.resolve()),
  transcribeLatestRecording: vi.fn(() =>
    Promise.resolve({ success: true, text: "" }),
  ),
  cleanupTranscript: vi.fn(() => Promise.resolve({ ok: true })),
  copyTextToClipboard: vi.fn(() => Promise.resolve()),
  pasteClipboard: vi.fn(() => Promise.resolve()),
  bubbleHide: vi.fn(() => Promise.resolve()),
  bubbleShow: vi.fn(() => Promise.resolve()),
  getRecordingStatus: vi.fn(() => Promise.resolve({ lastError: null })),
  saveApiKey: vi.fn(() => Promise.resolve()),
  setHotkey: vi.fn(() => Promise.resolve()),
  setStartAtLoginEnabled: vi.fn(() => Promise.resolve()),
  getStartAtLoginStatus: vi.fn(() =>
    Promise.resolve({ enabled: true, available: true }),
  ),
  resetHotkeyToDefault: vi.fn(() => Promise.resolve()),
}));

let mockAppState: AppState = "idle";
vi.mock("./hooks/usePushToTalk", () => ({
  usePushToTalk: () => ({ appState: mockAppState }),
}));

describe("App", () => {
  it("renders the settings window with title", () => {
    render(<App />);
    expect(screen.getByText("Floe Settings")).toBeDefined();
  });

  it("renders at least one close button", () => {
    render(<App />);
    const closeButtons = screen.getAllByLabelText("Close settings");
    expect(closeButtons.length).toBeGreaterThanOrEqual(1);
  });

  it("renders at least one API key input field", () => {
    render(<App />);
    const inputs = screen.getAllByPlaceholderText("Enter your API key");
    expect(inputs.length).toBeGreaterThanOrEqual(1);
  });
});
