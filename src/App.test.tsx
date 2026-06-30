import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import App from "./App";
import useFloeStore from "./stores/useFloeStore";
import type { AppState } from "./types/app";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    show: vi.fn(() => Promise.resolve()),
    hide: vi.fn(() => Promise.resolve()),
    setFocus: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
  })),
}));

const mockGetHotkeySettings = vi.fn();
const mockGetApiKeyStatus = vi.fn();
const mockGetUpdateInfo = vi.fn();
const mockCheckForUpdate = vi.fn();

vi.mock("./lib/tauri", () => ({
  isTauriRuntime: () => true,
  getHotkeySettings: () => mockGetHotkeySettings(),
  getApiKeyStatus: () => mockGetApiKeyStatus(),
  getAudioDevices: () => Promise.resolve([]),
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
  getAppSettings: vi.fn(() =>
    Promise.resolve({
      hotkey: { accelerator: "Ctrl+Space", label: "Ctrl+Space" },
      deviceId: null,
      skipCleanup: false,
    }),
  ),
  saveAppSettings: vi.fn(() => Promise.resolve({})),
  getUpdateInfo: () => mockGetUpdateInfo(),
  checkForUpdate: () => mockCheckForUpdate(),
  resetHotkeyToDefault: vi.fn(() => Promise.resolve()),
}));

let mockAppState: AppState = "idle";
vi.mock("./hooks/usePushToTalk", () => ({
  usePushToTalk: () => ({
    appState: mockAppState,
    latestTranscript: null,
    confirmPreview: vi.fn(),
    discardPreview: vi.fn(),
    error: null,
  }),
}));

function resetStore() {
  useFloeStore.setState({
    status: "idle",
    recordingStartedAt: null,
    recordingDurationMs: 0,
    apiKey: null,
    apiKeyConfigured: false,
    apiKeyMaskedPreview: null,
    hotkey: null,
    hotkeyRegistered: false,
    isSettingsOpen: false,
    isHotkeyCaptureActive: false,
    launchOnStartup: false,
    updateInfo: null,
    updateCheckInProgress: false,
    lastStartupError: null,
  });
}

describe("App — setup gating", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  beforeEach(() => {
    resetStore();
    mockGetUpdateInfo.mockResolvedValue({
      currentVersion: "1.0.0",
      latestVersion: null,
      status: "idle",
      downloadProgress: 0,
      lastCheckResult: null,
      errorMessage: null,
    });
    mockCheckForUpdate.mockResolvedValue({
      currentVersion: "1.0.0",
      latestVersion: null,
      status: "no_update",
      downloadProgress: 0,
      lastCheckResult: "You're up to date",
      errorMessage: null,
    });
  });

  it("renders Onboarding instead of SettingsWindow when API key is missing", async () => {
    mockGetApiKeyStatus.mockResolvedValue({
      configured: false,
      maskedPreview: null,
    });
    mockGetHotkeySettings.mockResolvedValue({
      accelerator: "Ctrl+Space",
      label: "Ctrl+Space",
      isDefault: true,
      isRegistered: true,
      error: null,
    });

    render(<App />);

    // Onboarding title appears; SettingsWindow title does not
    expect(await screen.findByText(/welcome to floe/i)).toBeDefined();
    expect(screen.queryByText("Floe Settings")).toBeNull();
  });

  it("renders Onboarding hotkey step when key is configured but hotkey is not registered", async () => {
    mockGetApiKeyStatus.mockResolvedValue({
      configured: true,
      maskedPreview: "gsk_…****",
    });
    mockGetHotkeySettings.mockResolvedValue({
      accelerator: "Ctrl+Space",
      label: "Ctrl+Space",
      isDefault: true,
      isRegistered: false,
      error: null,
    });

    render(<App />);

    expect(await screen.findByText(/welcome to floe/i)).toBeDefined();
    expect(
      await screen.findByRole("heading", {
        name: /pick a push-to-talk hotkey/i,
      }),
    ).toBeDefined();
    expect(screen.queryByText("Floe Settings")).toBeNull();
  });

  it("renders the SettingsWindow when both API key and hotkey are configured", async () => {
    mockGetApiKeyStatus.mockResolvedValue({
      configured: true,
      maskedPreview: "gsk_…****",
    });
    mockGetHotkeySettings.mockResolvedValue({
      accelerator: "Ctrl+Space",
      label: "Ctrl+Space",
      isDefault: true,
      isRegistered: true,
      error: null,
    });

    render(<App />);

    expect(await screen.findByText("Floe Settings")).toBeDefined();
    expect(screen.queryByText(/welcome to floe/i)).toBeNull();
    expect(screen.getAllByLabelText("Close settings").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByPlaceholderText("Enter your API key").length).toBeGreaterThanOrEqual(1);
  });

  it("renders at least one API key input field within SettingsWindow when ready", async () => {
    mockGetApiKeyStatus.mockResolvedValue({
      configured: true,
      maskedPreview: "gsk_…****",
    });
    mockGetHotkeySettings.mockResolvedValue({
      accelerator: "Ctrl+Space",
      label: "Ctrl+Space",
      isDefault: true,
      isRegistered: true,
      error: null,
    });

    render(<App />);
    await screen.findByText("Floe Settings");
    const inputs = screen.getAllByPlaceholderText("Enter your API key");
    expect(inputs.length).toBeGreaterThanOrEqual(1);
  });

  it("falls back to Onboarding when backend fails to respond", async () => {
    // Both calls reject — store stays at defaults, so setupState === setup_groq
    mockGetApiKeyStatus.mockRejectedValue(new Error("ipc failure"));
    mockGetHotkeySettings.mockRejectedValue(new Error("ipc failure"));

    render(<App />);

    expect(await screen.findByText(/welcome to floe/i)).toBeDefined();
  });

  it("returns to onboarding when hotkey becomes unregistered after ready", async () => {
    mockGetApiKeyStatus.mockResolvedValue({
      configured: true,
      maskedPreview: "gsk_…****",
    });
    mockGetHotkeySettings.mockResolvedValueOnce({
      accelerator: "Ctrl+Space",
      label: "Ctrl+Space",
      isDefault: true,
      isRegistered: true,
      error: null,
    });

    render(<App />);
    expect(await screen.findByText("Floe Settings")).toBeDefined();

    // Simulate the backend later reporting the hotkey as no longer registered
    useFloeStore.getState().setHotkeyStatus(null, false);
    // Allow React to re-render
    expect(await screen.findByText(/welcome to floe/i)).toBeDefined();
    expect(screen.queryByText("Floe Settings")).toBeNull();
  });

  it("returns to onboarding when API key is cleared after ready", async () => {
    mockGetApiKeyStatus.mockResolvedValue({
      configured: true,
      maskedPreview: "gsk_…****",
    });
    mockGetHotkeySettings.mockResolvedValue({
      accelerator: "Ctrl+Space",
      label: "Ctrl+Space",
      isDefault: true,
      isRegistered: true,
      error: null,
    });

    render(<App />);
    expect(await screen.findByText("Floe Settings")).toBeDefined();

    useFloeStore.getState().setApiKeyStatus(false, null);
    expect(await screen.findByText(/welcome to floe/i)).toBeDefined();
    expect(screen.queryByText("Floe Settings")).toBeNull();
  });

  // ── Startup error visibility ────────────────────────────────────────

  it("surfaces a startup-error banner when the API key backend rejects", async () => {
    mockGetApiKeyStatus.mockRejectedValue({
      domain: "settings",
      code: "secretStoreUnavailable",
      message: "Keychain locked",
    });
    mockGetHotkeySettings.mockResolvedValue({
      accelerator: "Ctrl+Space",
      label: "Ctrl+Space",
      isDefault: true,
      isRegistered: true,
      error: null,
    });

    render(<App />);

    await waitFor(() => {
      const state = useFloeStore.getState();
      expect(state.lastStartupError).toMatch(/api key/i);
    });

    // Banner renders inside Onboarding
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /keychain locked/i,
    );
  });

  it("surfaces a startup-error banner when the hotkey backend rejects", async () => {
    mockGetApiKeyStatus.mockResolvedValue({
      configured: true,
      maskedPreview: "gsk_…****",
    });
    mockGetHotkeySettings.mockRejectedValue({
      domain: "hotkey",
      code: "registrationFailed",
      message: "Global hotkey registration failed",
    });

    render(<App />);

    await waitFor(() => {
      const state = useFloeStore.getState();
      expect(state.lastStartupError).toMatch(/hotkey/i);
    });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /hotkey registration failed/i,
    );
  });

  it("falls back to onboarding when both startup calls reject and surfaces both errors", async () => {
    mockGetApiKeyStatus.mockRejectedValue(new Error("ipc failure"));
    mockGetHotkeySettings.mockRejectedValue(new Error("ipc failure"));

    render(<App />);

    expect(await screen.findByText(/welcome to floe/i)).toBeDefined();

    await waitFor(() => {
      expect(useFloeStore.getState().lastStartupError).not.toBeNull();
    });
  });

  it("surfaces update server unavailability into UpdateInfo status error", async () => {
    mockGetApiKeyStatus.mockResolvedValue({
      configured: true,
      maskedPreview: "gsk_…****",
    });
    mockGetHotkeySettings.mockResolvedValue({
      accelerator: "Ctrl+Space",
      label: "Ctrl+Space",
      isDefault: true,
      isRegistered: true,
      error: null,
    });
    mockCheckForUpdate.mockRejectedValue({
      message: "Could not reach GitHub",
      code: "gitHubApiUnreachable",
    });

    render(<App />);

    await waitFor(() => {
      const state = useFloeStore.getState();
      expect(state.updateInfo?.status).toBe("error");
      expect(state.updateInfo?.errorMessage).toMatch(/github/i);
    });
  });
});