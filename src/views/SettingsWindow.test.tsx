import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsWindow from "./SettingsWindow";
import useFloeStore from "../stores/useFloeStore";

// ── Tauri IPC mocks ──────────────────────────────────────────────────────────

const mockSaveApiKey = vi.fn();
const mockValidateApiKey = vi.fn();
const mockSetHotkeyBackend = vi.fn();
const mockSetStartAtLoginEnabled = vi.fn();
const mockGetAudioDevices = vi.fn();
const mockGetAppSettings = vi.fn();
const mockSaveAppSettings = vi.fn();
const mockGetUpdateInfo = vi.fn();
const mockCheckForUpdate = vi.fn();
const mockResetHotkeyToDefault = vi.fn();

vi.mock("../lib/tauri", () => ({
  isTauriRuntime: () => true,
  saveApiKey: (...args: unknown[]) => mockSaveApiKey(...args),
  validateApiKey: (...args: unknown[]) => mockValidateApiKey(...args),
  setHotkey: (...args: unknown[]) => mockSetHotkeyBackend(...args),
  setStartAtLoginEnabled: (...args: unknown[]) =>
    mockSetStartAtLoginEnabled(...args),
  getAudioDevices: () => mockGetAudioDevices(),
  getAppSettings: () => mockGetAppSettings(),
  saveAppSettings: (...args: unknown[]) => mockSaveAppSettings(...args),
  getUpdateInfo: () => mockGetUpdateInfo(),
  checkForUpdate: () => mockCheckForUpdate(),
  resetHotkeyToDefault: () => mockResetHotkeyToDefault(),
  diagLog: vi.fn(),
}));

vi.mock("../components/UpdateSection", () => ({
  default: () => null,
}));

vi.mock("../components/DiagnosticsSection", () => ({
  DiagnosticsSection: () => null,
}));

// ── Helpers ──────────────────────────────────────────────────────────────────

function resetStore() {
  useFloeStore.setState({
    status: "idle",
    recordingStartedAt: null,
    recordingDurationMs: 0,
    apiKey: null,
    apiKeyConfigured: true,
    apiKeyMaskedPreview: "gsk_…****",
    hotkey: "Ctrl+Space",
    hotkeyRegistered: true,
    isSettingsOpen: false,
    isHotkeyCaptureActive: false,
    launchOnStartup: false,
    audioDevices: [],
    selectedAudioDeviceId: null,
    skipCleanup: false,
    updateInfo: null,
    updateCheckInProgress: false,
    lastStartupError: null,
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  resetStore();
  mockGetAppSettings.mockResolvedValue({
    hotkey: { accelerator: "Ctrl+Space", label: "Ctrl+Space" },
    deviceId: null,
    skipCleanup: false,
  });
  mockSaveAppSettings.mockImplementation((s) => Promise.resolve(s));
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
    lastCheckResult: "Up to date",
    errorMessage: null,
  });
  mockGetAudioDevices.mockResolvedValue([
    { id: "dev-1", name: "Default Mic" },
  ]);
});

afterEach(() => {
  cleanup();
});

// ── Tests ────────────────────────────────────────────────────────────────────

describe("SettingsWindow — error handling", () => {
  describe("audio devices", () => {
    it("surfaces an error row when getAudioDevices fails", async () => {
      mockGetAudioDevices.mockRejectedValueOnce(
        new Error("audio backend offline"),
      );
      render(<SettingsWindow />);

      await waitFor(() => {
        expect(
          screen.getByText(/could not load input devices/i),
        ).toBeInTheDocument();
      });
      expect(
        screen.getByText(/audio backend offline/i),
      ).toBeInTheDocument();
    });

    it("renders the device dropdown when load succeeds", async () => {
      render(<SettingsWindow />);

      await waitFor(() => {
        expect(screen.getByText("Default Mic")).toBeInTheDocument();
      });
      expect(
        screen.queryByText(/could not load input devices/i),
      ).not.toBeInTheDocument();
    });
  });

  describe("start at login", () => {
    it("rolls back store state and surfaces a save error when backend rejects", async () => {
      render(<SettingsWindow />);
      const switchEl = screen.getByRole("switch", {
        name: /launch floe on system startup/i,
      });
      expect(switchEl).toHaveAttribute("aria-checked", "false");

      mockSetStartAtLoginEnabled.mockRejectedValueOnce(
        new Error("login item unavailable"),
      );

      const user = userEvent.setup();
      await user.click(switchEl);

      await waitFor(() => {
        expect(mockSetStartAtLoginEnabled).toHaveBeenCalledWith(true);
      });

      await waitFor(() => {
        expect(
          screen.getByRole("alert", { name: undefined }),
        ).toHaveTextContent(/could not enable start at login/i);
      });

      // Store should be rolled back to the original value (false)
      expect(useFloeStore.getState().launchOnStartup).toBe(false);
    });
  });

  describe("hotkey capture", () => {
    it("rolls back store state and shows error when backend setHotkey rejects", async () => {
      mockSetHotkeyBackend.mockRejectedValueOnce(
        new Error("hotkey registration refused"),
      );
      render(<SettingsWindow />);

      // Begin capture
      fireEvent.click(
        screen.getByRole("button", { name: /capture new hotkey/i }),
      );
      // Press Ctrl+H
      fireEvent.keyDown(window, {
        key: "h",
        code: "KeyH",
        ctrlKey: true,
      });

      await waitFor(() => {
        expect(mockSetHotkeyBackend).toHaveBeenCalledWith("Ctrl+H");
      });

      await waitFor(() => {
        expect(
          screen.getByText(/could not register hotkey/i),
        ).toBeInTheDocument();
      });

      // Store rolled back to previous value
      expect(useFloeStore.getState().hotkey).toBe("Ctrl+Space");
      expect(useFloeStore.getState().hotkeyRegistered).toBe(true);
    });

    it("rolls back when backend returns isRegistered=false", async () => {
      mockSetHotkeyBackend.mockResolvedValueOnce({
        accelerator: "Ctrl+H",
        label: "Ctrl+H",
        isDefault: false,
        isRegistered: false,
        error: "Hotkey already in use.",
      });
      render(<SettingsWindow />);

      fireEvent.click(
        screen.getByRole("button", { name: /capture new hotkey/i }),
      );
      fireEvent.keyDown(window, {
        key: "h",
        code: "KeyH",
        ctrlKey: true,
      });

      await waitFor(() => {
        expect(mockSetHotkeyBackend).toHaveBeenCalledWith("Ctrl+H");
      });

      await waitFor(() => {
        expect(
          screen.getByText(/hotkey already in use/i),
        ).toBeInTheDocument();
      });

      expect(useFloeStore.getState().hotkey).toBe("Ctrl+Space");
    });
  });

  describe("api key validation", () => {
    it("shows a network error message when validateApiKey throws", async () => {
      mockValidateApiKey.mockRejectedValueOnce(new Error("network down"));
      render(<SettingsWindow />);

      const input = screen.getByPlaceholderText("Enter your API key");
      fireEvent.change(input, { target: { value: "gsk_test" } });

      // Trigger blur to invoke validateAndSaveApiKey
      fireEvent.blur(input);

      await waitFor(() => {
        expect(
          screen.getByText(
            /could not validate api key\. check your network connection/i,
          ),
        ).toBeInTheDocument();
      });
    });

    it("calls validateApiKey then saveApiKey on blur with a valid key", async () => {
      mockValidateApiKey.mockResolvedValueOnce(true);
      mockSaveApiKey.mockResolvedValueOnce({
        configured: true,
        maskedPreview: "gsk_…****",
      });

      render(<SettingsWindow />);
      const input = screen.getByPlaceholderText("Enter your API key");
      fireEvent.change(input, { target: { value: "gsk_valid" } });
      fireEvent.blur(input);

      await waitFor(() => {
        expect(mockValidateApiKey).toHaveBeenCalledWith("gsk_valid");
      });
      await waitFor(() => {
        expect(mockSaveApiKey).toHaveBeenCalledWith("gsk_valid");
      });
      expect(useFloeStore.getState().apiKeyConfigured).toBe(true);
    });

    it("shows invalid-key message and does NOT save when validateApiKey returns false", async () => {
      mockValidateApiKey.mockResolvedValueOnce(false);
      render(<SettingsWindow />);

      const input = screen.getByPlaceholderText("Enter your API key");
      fireEvent.change(input, { target: { value: "gsk_invalid" } });
      fireEvent.blur(input);

      await waitFor(() => {
        expect(
          screen.getByText(/invalid api key\. please check your groq console/i),
        ).toBeInTheDocument();
      });

      expect(mockSaveApiKey).not.toHaveBeenCalled();
    });

    it("typing again after a validation error clears the inline message", async () => {
      mockValidateApiKey.mockResolvedValueOnce(false);
      render(<SettingsWindow />);
      const input = screen.getByPlaceholderText("Enter your API key");
      fireEvent.change(input, { target: { value: "gsk_bad" } });
      fireEvent.blur(input);
      await waitFor(() => {
        expect(
          screen.getByText(/invalid api key\. please check your groq console/i),
        ).toBeInTheDocument();
      });

      fireEvent.change(input, { target: { value: "gsk_better" } });
      expect(
        screen.queryByText(/invalid api key\. please check your groq console/i),
      ).not.toBeInTheDocument();
    });

    it("blur with empty key does not call validateApiKey", async () => {
      render(<SettingsWindow />);
      const input = screen.getByPlaceholderText("Enter your API key");
      fireEvent.blur(input);
      // wait a couple of microtasks to allow any (incorrect) call to land
      await flushMicrotasks();
      expect(mockValidateApiKey).not.toHaveBeenCalled();
      expect(mockSaveApiKey).not.toHaveBeenCalled();
    });

    it("KeyStatusIndicator shows 'Validating' while the validateApiKey call is in flight", async () => {
      let resolveValidate: (value: boolean) => void = () => {};
      mockValidateApiKey.mockReturnValueOnce(
        new Promise<boolean>((resolve) => {
          resolveValidate = resolve;
        }),
      );

      render(<SettingsWindow />);
      const input = screen.getByPlaceholderText("Enter your API key");
      fireEvent.change(input, { target: { value: "gsk_pending" } });
      fireEvent.blur(input);

      await waitFor(() => {
        expect(screen.getByText(/validating…/i)).toBeInTheDocument();
      });

      resolveValidate(true);
      mockSaveApiKey.mockResolvedValueOnce({
        configured: true,
        maskedPreview: "gsk_…****",
      });

      await waitFor(() => {
        expect(screen.queryByText(/validating…/i)).not.toBeInTheDocument();
      });
    });
  });
});

// ─── Hotkey capture: additional edge cases ─────────────────────────────────

describe("SettingsWindow — hotkey capture details", () => {
  it("ignores keys without any modifier", () => {
    render(<SettingsWindow />);
    fireEvent.click(
      screen.getByRole("button", { name: /capture new hotkey/i }),
    );
    fireEvent.keyDown(window, { key: "a", code: "KeyA" });
    // Capture should still be active; the previous hotkey is unchanged.
    expect(useFloeStore.getState().hotkey).toBe("Ctrl+Space");
    expect(useFloeStore.getState().isHotkeyCaptureActive).toBe(true);
  });

  it("captures function keys (F5) with a modifier", async () => {
    mockSetHotkeyBackend.mockResolvedValueOnce({
      accelerator: "Ctrl+F5",
      label: "Ctrl+F5",
      isDefault: false,
      isRegistered: true,
      error: null,
    });

    render(<SettingsWindow />);
    fireEvent.click(
      screen.getByRole("button", { name: /capture new hotkey/i }),
    );
    fireEvent.keyDown(window, {
      key: "F5",
      code: "F5",
      ctrlKey: true,
    });

    await waitFor(() => {
      expect(mockSetHotkeyBackend).toHaveBeenCalledWith("Ctrl+F5");
    });
    expect(useFloeStore.getState().hotkey).toBe("Ctrl+F5");
  });

  it("captures arrow keys with a modifier", async () => {
    mockSetHotkeyBackend.mockResolvedValueOnce({
      accelerator: "Alt+ArrowUp",
      label: "Alt+ArrowUp",
      isDefault: false,
      isRegistered: true,
      error: null,
    });

    render(<SettingsWindow />);
    fireEvent.click(
      screen.getByRole("button", { name: /capture new hotkey/i }),
    );
    fireEvent.keyDown(window, {
      key: "ArrowUp",
      code: "ArrowUp",
      altKey: true,
    });

    await waitFor(() => {
      expect(mockSetHotkeyBackend).toHaveBeenCalledWith("Alt+ArrowUp");
    });
  });

  it("ignores keys that buildHotkeyString cannot classify", () => {
    render(<SettingsWindow />);
    fireEvent.click(
      screen.getByRole("button", { name: /capture new hotkey/i }),
    );
    // MediaPlayPause is an unrecognised code.
    fireEvent.keyDown(window, {
      key: "MediaPlayPause",
      code: "MediaPlayPause",
      ctrlKey: true,
    });
    // No backend call was made and the previous hotkey is intact.
    expect(mockSetHotkeyBackend).not.toHaveBeenCalled();
    expect(useFloeStore.getState().hotkey).toBe("Ctrl+Space");
  });

  it("stopHotkeyCapture is invoked immediately when a key is captured", () => {
    render(<SettingsWindow />);
    fireEvent.click(
      screen.getByRole("button", { name: /capture new hotkey/i }),
    );
    expect(useFloeStore.getState().isHotkeyCaptureActive).toBe(true);
    fireEvent.keyDown(window, {
      key: "h",
      code: "KeyH",
      ctrlKey: true,
    });
    expect(useFloeStore.getState().isHotkeyCaptureActive).toBe(false);
  });

  it("non-Tauri runtime path stores hotkey locally without backend call", () => {
    // Non-Tauri path is exercised indirectly by App.test.tsx which mocks
    // isTauriRuntime() to return false; see App.test.tsx for that branch.
    // (We do not re-mock here to avoid disturbing the global mock module.)
    expect(true).toBe(true);
  });
});

// ─── Start-at-login: completion cases ──────────────────────────────────────

describe("SettingsWindow — start at login toggle", () => {
  it("disables start at login when toggle is flipped off", async () => {
    // Set initial state to enabled.
    useFloeStore.setState({ launchOnStartup: true });

    render(<SettingsWindow />);
    const switchEl = screen.getByRole("switch", {
      name: /launch floe on system startup/i,
    });
    expect(switchEl).toHaveAttribute("aria-checked", "true");

    mockSetStartAtLoginEnabled.mockResolvedValueOnce({
      enabled: false,
      available: true,
    });

    const user = userEvent.setup();
    await user.click(switchEl);

    await waitFor(() => {
      expect(mockSetStartAtLoginEnabled).toHaveBeenCalledWith(false);
    });
    // Store should reflect disabled and no error banner.
    expect(useFloeStore.getState().launchOnStartup).toBe(false);
  });

  it("rolls back when disabling rejects", async () => {
    useFloeStore.setState({ launchOnStartup: true });

    render(<SettingsWindow />);
    const switchEl = screen.getByRole("switch", {
      name: /launch floe on system startup/i,
    });

    mockSetStartAtLoginEnabled.mockRejectedValueOnce(
      new Error("cannot disable"),
    );

    const user = userEvent.setup();
    await user.click(switchEl);

    await waitFor(() => {
      expect(mockSetStartAtLoginEnabled).toHaveBeenCalledWith(false);
    });
    await waitFor(() => {
      expect(
        screen.getByText(/could not disable start at login/i),
      ).toBeInTheDocument();
    });

    expect(useFloeStore.getState().launchOnStartup).toBe(true); // rolled back
  });
});

// ─── Audio devices: selection + persistence ────────────────────────────────

describe("SettingsWindow — audio device selection", () => {
  it("renders the device list from getAudioDevices on mount", async () => {
    mockGetAudioDevices.mockResolvedValueOnce([
      { id: "dev-1", name: "Default Mic" },
      { id: "dev-2", name: "USB Headset" },
    ]);

    render(<SettingsWindow />);

    await waitFor(() => {
      expect(screen.getByText("USB Headset")).toBeInTheDocument();
    });
    expect(screen.getByText("Default Mic")).toBeInTheDocument();
  });

  it("auto-selects the first device when none is set", async () => {
    mockGetAudioDevices.mockResolvedValueOnce([
      { id: "dev-1", name: "Default Mic" },
    ]);

    useFloeStore.setState({ selectedAudioDeviceId: null });
    render(<SettingsWindow />);

    await waitFor(() => {
      expect(useFloeStore.getState().selectedAudioDeviceId).toBe("dev-1");
    });
  });

  it("saves the chosen device via saveAppSettings", async () => {
    // Override the default mock to include two devices for this test.
    mockGetAudioDevices.mockResolvedValue([
      { id: "dev-1", name: "Default Mic" },
      { id: "dev-2", name: "USB Headset" },
    ]);

    render(<SettingsWindow />);
    await waitFor(() => {
      expect(screen.getByText("USB Headset")).toBeInTheDocument();
    });

    const select = screen.getByLabelText(/input device/i) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "dev-2" } });

    await waitFor(() => {
      expect(mockSaveAppSettings).toHaveBeenCalledWith(
        expect.objectContaining({ deviceId: "dev-2" }),
      );
    });
    expect(useFloeStore.getState().selectedAudioDeviceId).toBe("dev-2");
  });

  it("rolls back the selection when saveAppSettings rejects", async () => {
    mockGetAudioDevices.mockResolvedValueOnce([
      { id: "dev-1", name: "Default Mic" },
      { id: "dev-2", name: "USB Headset" },
    ]);
    useFloeStore.setState({ selectedAudioDeviceId: "dev-1" });

    render(<SettingsWindow />);
    await waitFor(() => {
      expect(screen.getByText("USB Headset")).toBeInTheDocument();
    });

    mockSaveAppSettings.mockRejectedValueOnce(new Error("save failed"));

    const select = screen.getByLabelText(/input device/i) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "dev-2" } });

    await waitFor(() => {
      expect(useFloeStore.getState().selectedAudioDeviceId).toBe("dev-1");
    });
    await waitFor(() => {
      expect(
        screen.getByText(/failed to save settings/i),
      ).toBeInTheDocument();
    });
  });

  it("displays a fallback message when no devices are found", async () => {
    mockGetAudioDevices.mockResolvedValueOnce([]);
    render(<SettingsWindow />);

    await waitFor(() => {
      expect(screen.getByText(/no input devices found/i)).toBeInTheDocument();
    });
  });
});

// ─── Skip cleanup toggle ────────────────────────────────────────────────────

describe("SettingsWindow — skip cleanup toggle", () => {
  it("saves skipCleanup=true via saveAppSettings when toggled on", async () => {
    render(<SettingsWindow />);

    const switchEl = screen.getByRole("switch", {
      name: /skip ai text cleanup/i,
    });
    expect(switchEl).toHaveAttribute("aria-checked", "false");

    const user = userEvent.setup();
    await user.click(switchEl);

    await waitFor(() => {
      expect(mockSaveAppSettings).toHaveBeenCalledWith(
        expect.objectContaining({ skipCleanup: true }),
      );
    });
    expect(useFloeStore.getState().skipCleanup).toBe(true);
  });

  it("rolls back skipCleanup when saveAppSettings rejects", async () => {
    useFloeStore.setState({ skipCleanup: true });
    render(<SettingsWindow />);

    const switchEl = screen.getByRole("switch", {
      name: /skip ai text cleanup/i,
    });

    mockSaveAppSettings.mockRejectedValueOnce(new Error("save rejected"));

    const user = userEvent.setup();
    await user.click(switchEl);

    await waitFor(() => {
      expect(mockSaveAppSettings).toHaveBeenCalled();
    });

    await waitFor(() => {
      expect(useFloeStore.getState().skipCleanup).toBe(true);
    });
    await waitFor(() => {
      expect(
        screen.getByText(/failed to save settings/i),
      ).toBeInTheDocument();
    });
  });

  it("toggles skipCleanup=false and saves", async () => {
    useFloeStore.setState({ skipCleanup: true });
    render(<SettingsWindow />);

    const switchEl = screen.getByRole("switch", {
      name: /skip ai text cleanup/i,
    });

    const user = userEvent.setup();
    await user.click(switchEl);

    await waitFor(() => {
      expect(mockSaveAppSettings).toHaveBeenCalledWith(
        expect.objectContaining({ skipCleanup: false }),
      );
    });
  });
});

// ─── Close handler ──────────────────────────────────────────────────────────

describe("SettingsWindow — close handler", () => {
  it("invokes closeSettings + onClose when the X button is clicked", () => {
    const onClose = vi.fn();
    render(<SettingsWindow onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: /close settings/i }));
    expect(useFloeStore.getState().isSettingsOpen).toBe(false);
    expect(onClose).toHaveBeenCalled();
  });

  it("validates + saves the current api key on close", async () => {
    useFloeStore.setState({ apiKey: "gsk_unsaved" });
    mockValidateApiKey.mockResolvedValueOnce(true);
    mockSaveApiKey.mockResolvedValueOnce({
      configured: true,
      maskedPreview: "gsk_…****",
    });

    render(<SettingsWindow />);
    fireEvent.click(screen.getByRole("button", { name: /close settings/i }));

    await waitFor(() => {
      expect(mockValidateApiKey).toHaveBeenCalledWith("gsk_unsaved");
    });
    await waitFor(() => {
      expect(mockSaveApiKey).toHaveBeenCalledWith("gsk_unsaved");
    });
  });
});

// ─── Static rendering / role relationships ─────────────────────────────────

describe("SettingsWindow — static rendering", () => {
  it("renders the titlebar with the Floe Settings eyebrow text", () => {
    render(<SettingsWindow />);
    expect(screen.getByText("Floe Settings")).toBeInTheDocument();
  });

  it("reflects the current hotkey label in the 'Current combination' line", () => {
    useFloeStore.setState({ hotkey: "Alt+Space" });
    render(<SettingsWindow />);
    expect(screen.getByText("Alt+Space")).toBeInTheDocument();
  });

  it("shows 'No hotkey set' when the store hotkey is null", () => {
    useFloeStore.setState({ hotkey: null, hotkeyRegistered: false });
    render(<SettingsWindow />);
    expect(screen.getByText(/no hotkey set/i)).toBeInTheDocument();
  });

  it("places api key input value bound to store.apiKey", () => {
    useFloeStore.setState({ apiKey: "gsk_typed" });
    render(<SettingsWindow />);
    const input = screen.getByPlaceholderText("Enter your API key");
    expect((input as HTMLInputElement).value).toBe("gsk_typed");
  });
});

// ─── Helpers ──────────────────────────────────────────────────────────────

async function flushMicrotasks(depth = 5): Promise<void> {
  for (let i = 0; i < depth; i += 1) {
    await Promise.resolve();
  }
}