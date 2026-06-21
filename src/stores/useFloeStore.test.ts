import { describe, it, expect, beforeEach } from "vitest";
import useFloeStore from "./useFloeStore";
import type { UpdateInfo } from "../types/app";

// Helper: reset the store to its initial state before each test.
// Zustand stores created with `create` are singletons; we use
// `getInitialState` to reset.
function resetStore() {
  useFloeStore.setState({
    status: "idle",
    recordingStartedAt: null,
    recordingDurationMs: 0,
    apiKey: null,
    apiKeyConfigured: false,
    apiKeyMaskedPreview: null,
    hotkey: null,
    isSettingsOpen: false,
    isHotkeyCaptureActive: false,
    launchOnStartup: false,
    updateInfo: null,
    updateCheckInProgress: false,
  });
}

function makeUpdateInfo(overrides: Partial<UpdateInfo> = {}): UpdateInfo {
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

describe("useFloeStore – update state", () => {
  beforeEach(() => {
    resetStore();
  });

  // ── Initial state ───────────────────────────────────────────────────

  it("starts with updateInfo null", () => {
    const { updateInfo } = useFloeStore.getState();
    expect(updateInfo).toBeNull();
  });

  it("starts with updateCheckInProgress false", () => {
    const { updateCheckInProgress } = useFloeStore.getState();
    expect(updateCheckInProgress).toBe(false);
  });

  // ── setUpdateInfo ───────────────────────────────────────────────────

  it("setUpdateInfo stores the info and resets inProgress", () => {
    const info = makeUpdateInfo({ status: "available", latestVersion: "v0.2.0" });

    // Set inProgress true first
    useFloeStore.getState().setUpdateCheckInProgress(true);
    expect(useFloeStore.getState().updateCheckInProgress).toBe(true);

    // setUpdateInfo should clear inProgress
    useFloeStore.getState().setUpdateInfo(info);

    const state = useFloeStore.getState();
    expect(state.updateInfo).toEqual(info);
    expect(state.updateCheckInProgress).toBe(false);
  });

  it("setUpdateInfo(null) clears the update info", () => {
    const info = makeUpdateInfo({ status: "available" });
    useFloeStore.getState().setUpdateInfo(info);
    expect(useFloeStore.getState().updateInfo).not.toBeNull();

    useFloeStore.getState().setUpdateInfo(null);
    expect(useFloeStore.getState().updateInfo).toBeNull();
  });

  // ── setUpdateCheckInProgress ────────────────────────────────────────

  it("setUpdateCheckInProgress(true) sets the flag", () => {
    useFloeStore.getState().setUpdateCheckInProgress(true);
    expect(useFloeStore.getState().updateCheckInProgress).toBe(true);
  });

  it("setUpdateCheckInProgress(false) clears the flag", () => {
    useFloeStore.getState().setUpdateCheckInProgress(true);
    useFloeStore.getState().setUpdateCheckInProgress(false);
    expect(useFloeStore.getState().updateCheckInProgress).toBe(false);
  });

  // ── State transitions ──────────────────────────────────────────────

  it("transitions idle → checking via setUpdateCheckInProgress", () => {
    useFloeStore.getState().setUpdateCheckInProgress(true);
    expect(useFloeStore.getState().updateCheckInProgress).toBe(true);
    expect(useFloeStore.getState().updateInfo).toBeNull(); // not set yet
  });

  it("transitions checking → available via setUpdateInfo", () => {
    useFloeStore.getState().setUpdateCheckInProgress(true);

    const availableInfo = makeUpdateInfo({
      status: "available",
      latestVersion: "v0.2.0",
    });
    useFloeStore.getState().setUpdateInfo(availableInfo);

    const state = useFloeStore.getState();
    expect(state.updateInfo?.status).toBe("available");
    expect(state.updateInfo?.latestVersion).toBe("v0.2.0");
    expect(state.updateCheckInProgress).toBe(false);
  });

  it("transitions available → downloading via setUpdateInfo", () => {
    useFloeStore.getState().setUpdateInfo(
      makeUpdateInfo({ status: "available", latestVersion: "v0.2.0" }),
    );

    const downloadingInfo = makeUpdateInfo({
      status: "downloading",
      latestVersion: "v0.2.0",
      downloadProgress: 35,
    });
    useFloeStore.getState().setUpdateInfo(downloadingInfo);

    const state = useFloeStore.getState();
    expect(state.updateInfo?.status).toBe("downloading");
    expect(state.updateInfo?.downloadProgress).toBe(35);
  });

  it("transitions downloading → downloaded via setUpdateInfo", () => {
    useFloeStore.getState().setUpdateInfo(
      makeUpdateInfo({ status: "downloading", downloadProgress: 100 }),
    );

    useFloeStore.getState().setUpdateInfo(
      makeUpdateInfo({ status: "downloaded", downloadProgress: 100 }),
    );

    expect(useFloeStore.getState().updateInfo?.status).toBe("downloaded");
  });

  it("transitions any state → error via setUpdateInfo", () => {
    useFloeStore.getState().setUpdateInfo(
      makeUpdateInfo({ status: "available", latestVersion: "v0.2.0" }),
    );

    useFloeStore.getState().setUpdateInfo(
      makeUpdateInfo({
        status: "error",
        errorMessage: "Download failed: connection timeout",
      }),
    );

    const state = useFloeStore.getState();
    expect(state.updateInfo?.status).toBe("error");
    expect(state.updateInfo?.errorMessage).toContain("connection timeout");
    expect(state.updateCheckInProgress).toBe(false);
  });

  it("transitions error → idle via setUpdateInfo(null) (dismiss)", () => {
    useFloeStore.getState().setUpdateInfo(
      makeUpdateInfo({ status: "error", errorMessage: "Something broke" }),
    );
    expect(useFloeStore.getState().updateInfo?.status).toBe("error");

    useFloeStore.getState().setUpdateInfo(null);
    expect(useFloeStore.getState().updateInfo).toBeNull();
  });

  it("transitions checking → no_update via setUpdateInfo", () => {
    useFloeStore.getState().setUpdateCheckInProgress(true);

    const noUpdateInfo = makeUpdateInfo({
      status: "no_update",
      latestVersion: "1.0.0",
      lastCheckResult: "You're up to date",
    });
    useFloeStore.getState().setUpdateInfo(noUpdateInfo);

    const state = useFloeStore.getState();
    expect(state.updateInfo?.status).toBe("no_update");
    expect(state.updateInfo?.lastCheckResult).toBe("You're up to date");
    expect(state.updateCheckInProgress).toBe(false);
  });

  // ── Other store state is not affected ──────────────────────────────

  it("update actions do not affect recording state", () => {
    // Set some recording state
    useFloeStore.getState().setApiKey("gsk_test");
    useFloeStore.getState().setHotkey("Ctrl+Space");

    // Perform update actions
    useFloeStore.getState().setUpdateCheckInProgress(true);
    useFloeStore.getState().setUpdateInfo(
      makeUpdateInfo({ status: "available" }),
    );

    const state = useFloeStore.getState();
    expect(state.apiKey).toBe("gsk_test");
    expect(state.hotkey).toBe("Ctrl+Space");
    expect(state.status).toBe("idle"); // default
  });
});
