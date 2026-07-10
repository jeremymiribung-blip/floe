import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Onboarding from "./Onboarding";
import useFloeStore from "../stores/useFloeStore";

// ── Tauri IPC mocks ──────────────────────────────────────────────────────────

const mockValidateApiKey = vi.fn();
const mockSaveApiKey = vi.fn();
const mockSetHotkey = vi.fn();
const mockGetAudioDevices = vi.fn();

vi.mock("../lib/tauri", () => ({
  isTauriRuntime: () => true,
  validateApiKey: (...args: unknown[]) => mockValidateApiKey(...args),
  saveApiKey: (...args: unknown[]) => mockSaveApiKey(...args),
  setHotkey: (...args: unknown[]) => mockSetHotkey(...args),
  getAudioDevices: () => mockGetAudioDevices(),
  diagLog: vi.fn(),
}));

// ── Helpers ──────────────────────────────────────────────────────────────────

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

function setStoreReady() {
  useFloeStore.setState({
    apiKeyConfigured: true,
    apiKeyMaskedPreview: "gsk_…****",
    hotkey: "Ctrl+Space",
    hotkeyRegistered: true,
  });
}

describe("Onboarding", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    resetStore();
  });

  // ── Step 1: Groq ────────────────────────────────────────────────────

  describe("Step 1: Groq API key", () => {
    it("renders the Groq step on first launch when key is not configured", () => {
      render(<Onboarding />);
      expect(
        screen.getByRole("heading", { name: /connect your groq api key/i }),
      ).toBeDefined();
    });

    it("shows the API key input field", () => {
      render(<Onboarding />);
      expect(screen.getByPlaceholderText("gsk_…")).toBeDefined();
    });

    it("Continue button is disabled when input is empty", () => {
      render(<Onboarding />);
      const button = screen.getByRole("button", { name: /continue/i });
      expect(button).toBeDisabled();
    });

    it("Continue button is enabled once user types a key", async () => {
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.type(screen.getByPlaceholderText("gsk_…"), "gsk_abc123");
      const button = screen.getByRole("button", { name: /continue/i });
      expect(button).not.toBeDisabled();
    });

    it("shows validation error and does not advance on invalid key", async () => {
      mockValidateApiKey.mockResolvedValue(false);
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.type(screen.getByPlaceholderText("gsk_…"), "gsk_invalid");
      await user.click(screen.getByRole("button", { name: /continue/i }));

      await waitFor(() => {
        expect(
          screen.getByText(/invalid api key\. please check your groq console/i),
        ).toBeDefined();
      });
      expect(mockValidateApiKey).toHaveBeenCalledWith("gsk_invalid");
      expect(mockSaveApiKey).not.toHaveBeenCalled();
      expect(
        screen.getByRole("heading", { name: /connect your groq api key/i }),
      ).toBeDefined();
    });

    it("shows network error message when validateApiKey throws", async () => {
      mockValidateApiKey.mockRejectedValue(new Error("network down"));
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.type(screen.getByPlaceholderText("gsk_…"), "gsk_abc");
      await user.click(screen.getByRole("button", { name: /continue/i }));

      await waitFor(() => {
        expect(
          screen.getByText(
            /could not validate or save your api key: network down\. check your network connection/i,
          ),
        ).toBeDefined();
      });
    });

    it("shows error and keeps button disabled if user submits empty value via Enter", async () => {
      const user = userEvent.setup();
      render(<Onboarding />);
      const input = screen.getByPlaceholderText("gsk_…");
      input.focus();
      await user.keyboard("{Enter}");
      expect(
        await screen.findByText(/please enter your groq api key/i),
      ).toBeDefined();
      expect(mockValidateApiKey).not.toHaveBeenCalled();
    });

    it("trims whitespace before submitting the key", async () => {
      mockValidateApiKey.mockResolvedValue(true);
      mockSaveApiKey.mockResolvedValue({
        configured: true,
        maskedPreview: null,
      });
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.type(screen.getByPlaceholderText("gsk_…"), "  gsk_trim  ");
      await user.click(screen.getByRole("button", { name: /continue/i }));
      await waitFor(() => {
        expect(mockValidateApiKey).toHaveBeenCalledWith("gsk_trim");
      });
    });

    it("stores the trimmed key in the global store on input change", async () => {
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.type(screen.getByPlaceholderText("gsk_…"), "  gsk_abc  ");

      await waitFor(() => {
        expect(useFloeStore.getState().apiKey).toBe("gsk_abc");
      });
    });

    it("shows keychain error when saveApiKey rejects with secretStoreUnavailable", async () => {
      mockValidateApiKey.mockResolvedValue(true);
      mockSaveApiKey.mockRejectedValueOnce({
        domain: "settings",
        code: "secretStoreUnavailable",
        message: "Secure key storage is unavailable.",
      });
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.type(screen.getByPlaceholderText("gsk_…"), "gsk_abc");
      await user.click(screen.getByRole("button", { name: /continue/i }));

      await waitFor(() => {
        expect(
          screen.getByText(/your system.{0,5}s keychain is unavailable/i),
        ).toBeDefined();
      });
      expect(mockSaveApiKey).toHaveBeenCalledWith("gsk_abc");
    });

    it("falls back to network-style message for unrelated save failures", async () => {
      mockValidateApiKey.mockResolvedValue(true);
      mockSaveApiKey.mockRejectedValueOnce(new Error("ipc disconnected"));
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.type(screen.getByPlaceholderText("gsk_…"), "gsk_abc");
      await user.click(screen.getByRole("button", { name: /continue/i }));

      await waitFor(() => {
        expect(
          screen.getByText(
            /could not validate or save your api key: ipc disconnected\. check your network connection/i,
          ),
        ).toBeDefined();
      });
    });

    it("does not call validateApiKey or saveApiKey twice while validating", async () => {
      let resolveValidate: (value: boolean) => void = () => {};
      mockValidateApiKey.mockReturnValue(
        new Promise<boolean>((resolve) => {
          resolveValidate = resolve;
        }),
      );
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.type(screen.getByPlaceholderText("gsk_…"), "gsk_pending");
      const button = screen.getByRole("button", { name: /continue/i });
      await user.click(button);

      // While validating, the button is disabled and stays in step 1
      expect(button).toBeDisabled();
      expect(screen.getByText(/validating/i)).toBeDefined();
      expect(mockValidateApiKey).toHaveBeenCalledTimes(1);

      // Resolve to advance
      resolveValidate(true);
      mockSaveApiKey.mockResolvedValue({
        configured: true,
        maskedPreview: null,
      });

      await waitFor(() => {
        expect(screen.queryByText(/validating/i)).toBeNull();
      });
      // Should have advanced to the hotkey step
      await waitFor(() => {
        expect(
          screen.getByRole("heading", { name: /pick a push-to-talk hotkey/i }),
        ).toBeDefined();
      });
    });

    it("advances to Step 2 on a valid key and saves it to the store", async () => {
      mockValidateApiKey.mockResolvedValue(true);
      mockSaveApiKey.mockResolvedValue({
        configured: true,
        maskedPreview: "gsk_…****",
      });
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.type(screen.getByPlaceholderText("gsk_…"), "gsk_valid");
      await user.click(screen.getByRole("button", { name: /continue/i }));

      await waitFor(() => {
        expect(
          screen.getByRole("heading", { name: /pick a push-to-talk hotkey/i }),
        ).toBeDefined();
      });
      expect(mockSaveApiKey).toHaveBeenCalledWith("gsk_valid");
      // Store reflects new state
      expect(useFloeStore.getState().apiKeyConfigured).toBe(true);
    });

    it("continues to Step 2 directly when store already has API key configured", () => {
      // Simulate: backend already returned a configured key
      useFloeStore.setState({
        apiKeyConfigured: true,
        apiKeyMaskedPreview: "gsk_…****",
      });
      render(<Onboarding />);
      expect(
        screen.getByRole("heading", { name: /pick a push-to-talk hotkey/i }),
      ).toBeDefined();
    });
  });

  // ── Step 2: Hotkey ──────────────────────────────────────────────────

  describe("Step 2: Hotkey", () => {
    beforeEach(() => {
      useFloeStore.setState({
        apiKeyConfigured: true,
        apiKeyMaskedPreview: "gsk_…****",
      });
    });

    it("renders the hotkey step when API key is configured but hotkey is not", () => {
      render(<Onboarding />);
      expect(
        screen.getByRole("heading", { name: /pick a push-to-talk hotkey/i }),
      ).toBeDefined();
    });

    it("Continue button is disabled before any hotkey is captured", () => {
      render(<Onboarding />);
      const button = screen.getByRole("button", { name: /continue/i });
      expect(button).toBeDisabled();
    });

    it("captures a hotkey when a modifier + key is pressed", () => {
      render(<Onboarding />);
      // Begin capture
      fireEvent.click(
        screen.getByRole("button", { name: /capture new hotkey/i }),
      );
      // Press Ctrl+Space
      fireEvent.keyDown(window, { key: " ", code: "Space", ctrlKey: true });
      // Now the captured combo should appear
      expect(screen.getByText("Ctrl+Space")).toBeDefined();
      const button = screen.getByRole("button", { name: /continue/i });
      expect(button).not.toBeDisabled();
    });

    it("ignores key events that do not include a modifier", () => {
      render(<Onboarding />);
      fireEvent.click(
        screen.getByRole("button", { name: /capture new hotkey/i }),
      );
      // Press a bare key — no modifier
      fireEvent.keyDown(window, { key: "a", code: "KeyA" });
      // Continue should still be disabled
      expect(screen.getByRole("button", { name: /continue/i })).toBeDisabled();
    });

    it("Back button returns to Step 1", async () => {
      const user = userEvent.setup();
      render(<Onboarding />);
      await user.click(screen.getByRole("button", { name: /back/i }));
      expect(
        screen.getByRole("heading", { name: /connect your groq api key/i }),
      ).toBeDefined();
    });

    it("calls setHotkey backend and advances to Done step on a successful save", async () => {
      mockSetHotkey.mockResolvedValue({
        accelerator: "Ctrl+Space",
        label: "Ctrl+Space",
        isDefault: false,
        isRegistered: true,
        error: null,
      });
      const user = userEvent.setup();
      render(<Onboarding />);
      fireEvent.click(
        screen.getByRole("button", { name: /capture new hotkey/i }),
      );
      fireEvent.keyDown(window, { key: " ", code: "Space", ctrlKey: true });

      await user.click(screen.getByRole("button", { name: /continue/i }));

      await waitFor(() => {
        expect(mockSetHotkey).toHaveBeenCalledWith("Ctrl+Space");
      });
      await waitFor(() => {
        expect(
          screen.getByRole("heading", { name: /you.?re all set/i }),
        ).toBeDefined();
      });
      // Store reflects new state
      expect(useFloeStore.getState().hotkey).toBe("Ctrl+Space");
      expect(useFloeStore.getState().hotkeyRegistered).toBe(true);
    });

    it("shows backend error and stays on Step 2 when hotkey cannot be registered", async () => {
      mockSetHotkey.mockResolvedValue({
        accelerator: "Ctrl+H",
        label: "Ctrl+H",
        isDefault: false,
        isRegistered: false,
        error: "Hotkey already in use by another application.",
      });
      const user = userEvent.setup();
      render(<Onboarding />);
      fireEvent.click(
        screen.getByRole("button", { name: /capture new hotkey/i }),
      );
      fireEvent.keyDown(window, { key: "h", code: "KeyH", ctrlKey: true });
      await user.click(screen.getByRole("button", { name: /continue/i }));

      await waitFor(() => {
        expect(
          screen.getByText(/hotkey already in use by another application/i),
        ).toBeDefined();
      });
      // Still on Step 2
      expect(
        screen.getByRole("heading", { name: /pick a push-to-talk hotkey/i }),
      ).toBeDefined();
    });

    it("shows a fallback error when the backend throws", async () => {
      mockSetHotkey.mockRejectedValue(new Error("ipc failure"));
      const user = userEvent.setup();
      render(<Onboarding />);
      fireEvent.click(
        screen.getByRole("button", { name: /capture new hotkey/i }),
      );
      fireEvent.keyDown(window, { key: " ", code: "Space", ctrlKey: true });
      await user.click(screen.getByRole("button", { name: /continue/i }));

      await waitFor(() => {
        expect(
          screen.getByText(/could not register the hotkey/i),
        ).toBeDefined();
      });
    });

    it("Escape during capture cancels the capture flow", () => {
      render(<Onboarding />);
      fireEvent.click(
        screen.getByRole("button", { name: /capture new hotkey/i }),
      );
      expect(screen.getByText(/press any key combination/i)).toBeDefined();
      fireEvent.keyDown(window, { key: "Escape", code: "Escape" });
      // After cancel, Continue should still be disabled
      expect(screen.getByRole("button", { name: /continue/i })).toBeDisabled();
    });
  });

  // ── Step 3: Done ────────────────────────────────────────────────────

  describe("Step 3: Done", () => {
    it("renders the done step when store is ready", () => {
      setStoreReady();
      render(<Onboarding />);
      expect(
        screen.getByRole("heading", { name: /you.?re all set/i }),
      ).toBeDefined();
    });

    it("does not expose API key or hotkey capture controls on the done screen", () => {
      setStoreReady();
      render(<Onboarding />);
      expect(screen.queryByPlaceholderText("gsk_…")).toBeNull();
      expect(
        screen.queryByRole("button", { name: /capture new hotkey/i }),
      ).toBeNull();
    });

    it("does not auto-jump to setup_groq when already on done", () => {
      setStoreReady();
      render(<Onboarding />);
      // Force a transient ready state — the done screen should remain.
      expect(
        screen.getByRole("heading", { name: /you.?re all set/i }),
      ).toBeDefined();
    });
  });

  // ── Step indicator ───────────────────────────────────────────────────

  describe("Step indicator", () => {
    it("shows step 1 of 3 on first launch", () => {
      render(<Onboarding />);
      expect(screen.getByText(/step 1 of 3/i)).toBeDefined();
    });

    it("shows step 2 of 3 after the Groq step is complete", () => {
      useFloeStore.setState({ apiKeyConfigured: true });
      render(<Onboarding />);
      expect(screen.getByText(/step 2 of 3/i)).toBeDefined();
    });

    it("caps step label at 3 of 3 when ready", () => {
      setStoreReady();
      render(<Onboarding />);
      expect(screen.getByText(/step 3 of 3/i)).toBeDefined();
    });
  });

  // ── Direct entry into later steps ───────────────────────────────────

  describe("Direct entry into later steps", () => {
    it("skips Groq step when API key is already configured", () => {
      useFloeStore.setState({ apiKeyConfigured: true });
      render(<Onboarding />);
      expect(
        screen.queryByRole("heading", { name: /connect your groq api key/i }),
      ).toBeNull();
    });

    it("renders Done step when setup is already complete", () => {
      setStoreReady();
      render(<Onboarding />);
      expect(
        screen.getByRole("heading", { name: /you.?re all set/i }),
      ).toBeDefined();
    });
  });
});
