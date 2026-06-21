import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import UpdateSection from "./UpdateSection";
import useFloeStore from "../stores/useFloeStore";
import type { UpdateInfo } from "../types/app";

// ── Mock Tauri IPC ──────────────────────────────────────────────────────────

const mockCheckForUpdate = vi.fn();
const mockDownloadUpdate = vi.fn();
const mockInstallUpdate = vi.fn();
const mockResetUpdateState = vi.fn();

vi.mock("../lib/tauri", () => ({
  isTauriRuntime: () => true,
  checkForUpdate: (...args: unknown[]) => mockCheckForUpdate(...args),
  downloadUpdate: (...args: unknown[]) => mockDownloadUpdate(...args),
  installUpdate: (...args: unknown[]) => mockInstallUpdate(...args),
  resetUpdateState: (...args: unknown[]) => mockResetUpdateState(...args),
}));

// ── Helpers ─────────────────────────────────────────────────────────────────

function makeInfo(overrides: Partial<UpdateInfo> = {}): UpdateInfo {
  return {
    currentVersion: "1.0.0",
    latestVersion: null,
    status: "idle",
    downloadProgress: 0,
    lastCheckResult: null,
    errorMessage: null,
    ...overrides,
  };
}

function setStoreState(overrides: Partial<ReturnType<typeof useFloeStore.getState>> = {}) {
  useFloeStore.setState({
    updateInfo: null,
    updateCheckInProgress: false,
    ...overrides,
  });
}

function renderUpdateSection() {
  return render(<UpdateSection />);
}

describe("UpdateSection", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    setStoreState();
  });

  // ── Idle state ───────────────────────────────────────────────────────

  it('shows "Check for updates" button when idle', () => {
    setStoreState({ updateInfo: makeInfo({ status: "idle" }) });
    renderUpdateSection();

    expect(screen.getByText("Check for updates")).toBeDefined();
  });

  it("shows current version when idle", () => {
    setStoreState({ updateInfo: makeInfo({ status: "idle", currentVersion: "1.0.0" }) });
    renderUpdateSection();

    expect(screen.getByText((content) => content.includes("1.0.0"))).toBeDefined();
  });

  // ── Checking state ───────────────────────────────────────────────────

  it('shows "Checking..." when updateCheckInProgress is true', () => {
    setStoreState({
      updateInfo: makeInfo({ status: "idle" }),
      updateCheckInProgress: true,
    });
    renderUpdateSection();

    expect(screen.getByText("Checking\u2026")).toBeDefined();
  });

  it('button is disabled during "Checking..."', () => {
    setStoreState({
      updateInfo: makeInfo({ status: "idle" }),
      updateCheckInProgress: true,
    });
    renderUpdateSection();

    const btn = screen.getByRole("button", { name: /checking/i });
    expect(btn).toBeDisabled();
  });

  // ── Available state ──────────────────────────────────────────────────

  it('shows "Download update" button when update is available', () => {
    setStoreState({
      updateInfo: makeInfo({ status: "available", latestVersion: "v0.2.0" }),
    });
    renderUpdateSection();

    expect(screen.getByText("Download update")).toBeDefined();
  });

  it('shows "Available" label with version when update is available', () => {
    setStoreState({
      updateInfo: makeInfo({ status: "available", latestVersion: "v0.2.0" }),
    });
    renderUpdateSection();

    expect(screen.getByText("Available")).toBeDefined();
    expect(screen.getByText((content) => content.includes("0.2.0"))).toBeDefined();
  });

  it('shows "Dismiss" button when update is available', () => {
    setStoreState({
      updateInfo: makeInfo({ status: "available", latestVersion: "v0.2.0" }),
    });
    renderUpdateSection();

    expect(screen.getByText("Dismiss")).toBeDefined();
  });

  // ── Downloading state ────────────────────────────────────────────────

  it("shows progress bar when downloading", () => {
    setStoreState({
      updateInfo: makeInfo({ status: "downloading", downloadProgress: 45 }),
    });
    renderUpdateSection();

    expect(screen.getByText(/Downloading/)).toBeDefined();
    expect(screen.getByText((content) => content.includes("45") && content.includes("%"))).toBeDefined();
  });

  it('shows "Dismiss" when downloading', () => {
    setStoreState({
      updateInfo: makeInfo({ status: "downloading" }),
    });
    renderUpdateSection();

    expect(screen.getByText("Dismiss")).toBeDefined();
  });

  // ── Downloaded state ─────────────────────────────────────────────────

  it('shows "Restart to update" button when downloaded', () => {
    setStoreState({
      updateInfo: makeInfo({ status: "downloaded", latestVersion: "v0.2.0" }),
    });
    renderUpdateSection();

    expect(screen.getByText("Restart to update")).toBeDefined();
  });

  // ── No_update state ──────────────────────────────────────────────────

  it('shows "You\'re up to date" message', () => {
    setStoreState({
      updateInfo: makeInfo({
        status: "no_update",
        latestVersion: "1.0.0",
        lastCheckResult: "You're up to date",
      }),
    });
    renderUpdateSection();

    expect(screen.getByText("You're up to date")).toBeDefined();
  });

  it('shows "Latest" label when no update', () => {
    setStoreState({
      updateInfo: makeInfo({ status: "no_update", latestVersion: "1.0.0" }),
    });
    renderUpdateSection();

    expect(screen.getByText("Latest")).toBeDefined();
  });

  // ── Error state ──────────────────────────────────────────────────────

  it("shows error message in a red-bordered box", () => {
    setStoreState({
      updateInfo: makeInfo({
        status: "error",
        errorMessage: "Could not reach GitHub.",
      }),
    });
    renderUpdateSection();

    // The component renders the friendly error
    expect(screen.getByText("Could not check for updates")).toBeDefined();
  });

  it('shows "Retry check" button after error', () => {
    setStoreState({
      updateInfo: makeInfo({
        status: "error",
        errorMessage: "Something failed.",
      }),
    });
    renderUpdateSection();

    expect(screen.getByText("Retry check")).toBeDefined();
  });

  it("shows Dismiss button after error", () => {
    setStoreState({
      updateInfo: makeInfo({
        status: "error",
        errorMessage: "Something failed.",
      }),
    });
    renderUpdateSection();

    expect(screen.getByText("Dismiss")).toBeDefined();
  });

  // ── Interaction: Check for updates ──────────────────────────────────

  it("calls checkForUpdate when Check button is clicked", async () => {
    mockCheckForUpdate.mockResolvedValue(
      makeInfo({ status: "no_update", lastCheckResult: "You're up to date" }),
    );
    setStoreState({ updateInfo: makeInfo({ status: "idle" }) });
    renderUpdateSection();

    const user = userEvent.setup();
    await user.click(screen.getByText("Check for updates"));

    expect(mockCheckForUpdate).toHaveBeenCalledOnce();
  });

  it("handles checkForUpdate error and sets error state", async () => {
    mockCheckForUpdate.mockRejectedValue({
      message: "Network error",
      code: "gitHubApiUnreachable",
    });
    setStoreState({
      updateInfo: makeInfo({ status: "idle", currentVersion: "1.0.0" }),
    });
    renderUpdateSection();

    const user = userEvent.setup();
    await user.click(screen.getByText("Check for updates"));

    // Wait for the async handler to complete
    await vi.waitFor(() => {
      const state = useFloeStore.getState();
      expect(state.updateInfo?.status).toBe("error");
      expect(state.updateInfo?.errorMessage).toContain("Network error");
    });
  });

  // ── Interaction: Download update ─────────────────────────────────────

  it("calls downloadUpdate when Download button is clicked", async () => {
    mockDownloadUpdate.mockResolvedValue(
      makeInfo({ status: "downloading", downloadProgress: 10 }),
    );
    setStoreState({
      updateInfo: makeInfo({ status: "available", latestVersion: "v0.2.0" }),
    });
    renderUpdateSection();

    const user = userEvent.setup();
    await user.click(screen.getByText("Download update"));

    expect(mockDownloadUpdate).toHaveBeenCalledOnce();
  });

  // ── Interaction: Install update ──────────────────────────────────────

  it("calls installUpdate when Restart button is clicked", async () => {
    mockInstallUpdate.mockResolvedValue(undefined);
    setStoreState({
      updateInfo: makeInfo({ status: "downloaded", latestVersion: "v0.2.0" }),
    });
    renderUpdateSection();

    const user = userEvent.setup();
    await user.click(screen.getByText("Restart to update"));

    expect(mockInstallUpdate).toHaveBeenCalledOnce();
  });

  // ── Interaction: Dismiss ────────────────────────────────────────────

  it("calls resetUpdateState and clears info when Dismiss is clicked", async () => {
    mockResetUpdateState.mockResolvedValue(undefined);
    setStoreState({
      updateInfo: makeInfo({ status: "available", latestVersion: "v0.2.0" }),
    });
    renderUpdateSection();

    const user = userEvent.setup();
    await user.click(screen.getByText("Dismiss"));

    expect(mockResetUpdateState).toHaveBeenCalledOnce();
    const state = useFloeStore.getState();
    expect(state.updateInfo).toBeNull();
  });

  // ── Error categorization ─────────────────────────────────────────────

  it("shows rate limit error message correctly", () => {
    setStoreState({
      updateInfo: makeInfo({
        status: "error",
        errorMessage: "GitHub API rate limit exceeded. Please try again later.",
      }),
    });
    renderUpdateSection();

    // Should match the friendly error for rate limit
    expect(screen.getByText("GitHub rate limit hit")).toBeDefined();
  });

  it("shows generic error for unknown error code", () => {
    setStoreState({
      updateInfo: makeInfo({
        status: "error",
        errorMessage: "Some cryptic error",
      }),
    });
    renderUpdateSection();

    expect(screen.getByText("Update error")).toBeDefined();
  });
});
