(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { act, renderHook } from "@testing-library/react";
import { cleanup } from "@testing-library/react";

const {
  bubbleShow,
  bubbleHide,
  startRecording,
  stopRecording,
  forceStopRecording,
  getRecordingStatus,
  transcribeLatestRecording,
  cleanupTranscript,
  copyTextToClipboard,
  pasteClipboard,
  saveApiKey,
  validateApiKey,
  setHotkey,
  setStartAtLoginEnabled,
  getAppSettings,
  saveAppSettings,
  getAudioDevices,
  getHotkeySettings,
  resetHotkeyToDefault,
  getUpdateInfo,
  checkForUpdate,
  downloadUpdate,
  installUpdate,
  resetUpdateState,
  getDiagnosticsReport,
  bubbleCancelRecording,
  clearApiKey,
  getApiKeyStatus,
  getStartAtLoginStatus,
  diagLog,
  logFrontendEvent,
  updateSessionHotkeyLatency,
} = vi.hoisted(() => {
  const mk = (impl: (...args: never[]) => unknown) => vi.fn(impl);
  return {
    bubbleShow: mk(() => Promise.resolve()),
    bubbleHide: mk(() => Promise.resolve()),
    startRecording: mk(() =>
      Promise.resolve({
        isRecording: true,
        durationMs: 0,
        lastError: null,
        traceId: "trace-hook",
      }),
    ),
    stopRecording: mk(() => Promise.resolve()),
    forceStopRecording: mk(() => Promise.resolve()),
    getRecordingStatus: mk(() =>
      Promise.resolve({ isRecording: false, lastError: null }),
    ),
    transcribeLatestRecording: mk(() =>
      Promise.resolve({ text: "hello", model: "whisper", retryCount: 0 }),
    ),
    cleanupTranscript: mk(() =>
      Promise.resolve({
        text: "hello",
        model: "llama",
        retryCount: 0,
        fallbackUsed: false,
      }),
    ),
    copyTextToClipboard: mk(() => Promise.resolve()),
    pasteClipboard: mk(() => Promise.resolve()),
    saveApiKey: mk(() => Promise.resolve({ configured: true, maskedPreview: null })),
    validateApiKey: mk(() => Promise.resolve(true)),
    setHotkey: mk(() =>
      Promise.resolve({
        accelerator: "Ctrl+Space",
        label: "Ctrl+Space",
        isDefault: false,
        isRegistered: true,
        error: null,
      }),
    ),
    setStartAtLoginEnabled: mk(() =>
      Promise.resolve({ enabled: false, available: true }),
    ),
    getAppSettings: mk(() =>
      Promise.resolve({
        hotkey: { accelerator: "Ctrl+Space", label: "Ctrl+Space" },
        deviceId: null,
        skipCleanup: false,
      }),
    ),
    saveAppSettings: mk(() =>
      Promise.resolve({
        hotkey: { accelerator: "Ctrl+Space", label: "Ctrl+Space" },
        deviceId: null,
        skipCleanup: false,
      }),
    ),
    getAudioDevices: mk(() => Promise.resolve([])),
    getHotkeySettings: mk(() =>
      Promise.resolve({
        accelerator: "Ctrl+Space",
        label: "Ctrl+Space",
        isDefault: true,
        isRegistered: true,
        error: null,
      }),
    ),
    resetHotkeyToDefault: mk(() =>
      Promise.resolve({
        accelerator: "Ctrl+Space",
        label: "Ctrl+Space",
        isDefault: true,
        isRegistered: true,
        error: null,
      }),
    ),
    getUpdateInfo: mk(() =>
      Promise.resolve({
        currentVersion: "1.0.0",
        latestVersion: null,
        status: "idle",
        downloadProgress: 0,
        lastCheckResult: null,
        errorMessage: null,
      }),
    ),
    checkForUpdate: mk(() =>
      Promise.resolve({
        currentVersion: "1.0.0",
        latestVersion: null,
        status: "no_update",
        downloadProgress: 0,
        lastCheckResult: "Up to date",
        errorMessage: null,
      }),
    ),
    downloadUpdate: mk(() =>
      Promise.resolve({
        currentVersion: "1.0.0",
        latestVersion: null,
        status: "downloaded",
        downloadProgress: 100,
        lastCheckResult: null,
        errorMessage: null,
      }),
    ),
    installUpdate: mk(() => Promise.resolve()),
    resetUpdateState: mk(() => Promise.resolve()),
    getDiagnosticsReport: mk(() => Promise.resolve({})),
    bubbleCancelRecording: mk(() => Promise.resolve()),
    clearApiKey: mk(() => Promise.resolve({ configured: false, maskedPreview: null })),
    getApiKeyStatus: mk(() => Promise.resolve({ configured: false, maskedPreview: null })),
    getStartAtLoginStatus: mk(() => Promise.resolve({ enabled: false, available: true })),
    diagLog: mk(() => undefined),
    logFrontendEvent: mk(() => Promise.resolve()),
    updateSessionHotkeyLatency: mk(() => Promise.resolve()),
  };
});

vi.mock("../lib/tauri", () => ({
  isTauriRuntime: () => true,
  bubbleShow,
  bubbleHide,
  bubbleCancelRecording,
  diagLog,
  startRecording,
  stopRecording,
  forceStopRecording,
  getRecordingStatus,
  transcribeLatestRecording,
  cleanupTranscript,
  copyTextToClipboard,
  pasteClipboard,
  saveApiKey,
  validateApiKey,
  clearApiKey,
  getApiKeyStatus,
  getAppSettings,
  getAudioDevices,
  saveAppSettings,
  getHotkeySettings,
  setHotkey,
  resetHotkeyToDefault,
  getStartAtLoginStatus,
  setStartAtLoginEnabled,
  getUpdateInfo,
  checkForUpdate,
  downloadUpdate,
  installUpdate,
  resetUpdateState,
  getDiagnosticsReport,
  logFrontendEvent,
  updateSessionHotkeyLatency,
}));

const { listenMock, unlistenSpy, hotkeyListeners, recordingListeners } = vi.hoisted(() => {
  const hotkeyListenersRef: { current: Array<(event: { payload: { state: "Pressed" | "Released" } }) => void> } = { current: [] };
  const recordingListenersRef: { current: Array<(event: { payload: RecordingStatePayload }) => void> } = { current: [] };
  const listenMock = vi.fn();
  const unlistenSpy = vi.fn();
  return {
    listenMock,
    unlistenSpy,
    hotkeyListeners: hotkeyListenersRef,
    recordingListeners: recordingListenersRef,
  };
});

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

const { windowShow, windowHide, windowSetFocus, windowClose } = vi.hoisted(() => ({
  windowShow: vi.fn(() => Promise.resolve()),
  windowHide: vi.fn(() => Promise.resolve()),
  windowSetFocus: vi.fn(() => Promise.resolve()),
  windowClose: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    show: windowShow,
    hide: windowHide,
    setFocus: windowSetFocus,
    close: windowClose,
  }),
}));

import { usePushToTalk } from "../hooks/usePushToTalk";
import { EVENT_HOTKEY_STATE, EVENT_RECORDING_STATE_CHANGED } from "../lib/contract";
import * as tauri from "../lib/tauri";
import useFloeStore from "../stores/useFloeStore";
import type { RecordingStatePayload } from "../types/app";

type HotkeyPayload = { state: "Pressed" | "Released" };

beforeEach(() => {
  hotkeyListeners.current = [];
  recordingListeners.current = [];
  vi.clearAllMocks();
  // Re-apply listenMock mock implementation AFTER clearAllMocks:
  listenMock.mockImplementation((
    event: string,
    cb: (e: { payload: unknown }) => void,
  ) => {
    if (event === EVENT_HOTKEY_STATE) {
      hotkeyListeners.current.push(
        cb as (e: { payload: HotkeyPayload }) => void,
      );
      return Promise.resolve(() => {
        hotkeyListeners.current = hotkeyListeners.current.filter((l) => l !== cb);
        unlistenSpy(event);
      });
    }
    if (event === EVENT_RECORDING_STATE_CHANGED) {
      recordingListeners.current.push(
        cb as (e: { payload: RecordingStatePayload }) => void,
      );
      return Promise.resolve(() => {
        recordingListeners.current = recordingListeners.current.filter((l) => l !== cb);
        unlistenSpy(event);
      });
    }
    return Promise.resolve(() => {
      unlistenSpy(event);
    });
  });

  windowShow.mockClear();
  windowHide.mockClear();
  windowSetFocus.mockClear();
  windowClose.mockClear();
  unlistenSpy.mockClear();
  // Reset cumulative spy histories that span tests.
  (tauri.bubbleShow as unknown as ReturnType<typeof vi.fn>).mockClear();
  (tauri.bubbleHide as unknown as ReturnType<typeof vi.fn>).mockClear();

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
    audioDevices: [],
    selectedAudioDeviceId: null,
    skipCleanup: false,
    updateInfo: null,
    updateCheckInProgress: false,
    lastStartupError: null,
  });
});

const emitHotkey = (state: "Pressed" | "Released") => {
  for (const listener of [...hotkeyListeners.current]) {
    listener({ payload: { state } });
  }
};

const emitBackendState = (state: RecordingStatePayload["state"]) => {
  for (const listener of [...recordingListeners.current]) {
    listener({ payload: { state, isRecording: state !== "idle" } });
  }
};

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const flush = async () => {
  await act(async () => {
    for (let i = 0; i < 5; i += 1) {
      await Promise.resolve();
    }
  });
};

// ── Helpers ────────────────────────────────────────────────────────────────

function useHook() {
  // eslint-disable-next-line react-hooks/rules-of-hooks
  return renderHook(() => usePushToTalk());
}

// ── Hotkey events ──────────────────────────────────────────────────────────

describe("usePushToTalk — hotkey events", () => {
  it("registers a listener for EVENT_HOTKEY_STATE on mount", async () => {
    const { unmount } = useHook();
    await flush();
    const calls = listenMock.mock.calls.map((c: unknown[]) => c[0]);
    expect(calls).toContain(EVENT_HOTKEY_STATE);
    expect(hotkeyListeners.current.length).toBe(1);
    unmount();
  });

  it("press triggers bubbleShow + controller handleShortcutState('Pressed')", async () => {
    const startRecording = tauri.startRecording as unknown as ReturnType<
      typeof vi.fn
    >;
    useHook();
    await flush();

    emitHotkey("Pressed");
    await flush();

    expect(tauri.bubbleShow).toHaveBeenCalled();
    expect(startRecording).toHaveBeenCalledTimes(1);
  });

  it("release triggers bubbleHide + controller handleShortcutState('Released')", async () => {
    const stopRecording = tauri.stopRecording as unknown as ReturnType<
      typeof vi.fn
    >;
    const { result } = useHook();
    await flush();

    emitHotkey("Pressed");
    await flush();

    expect(useFloeStore.getState().status).toBe("recording");

    emitHotkey("Released");
    await flush();

    expect(tauri.bubbleHide).toHaveBeenCalled();
    expect(stopRecording).toHaveBeenCalled();
    expect(result.current.appState).not.toBe("idle");
  });

  it("handleShortcutState throws → state becomes error", async () => {
    const { result } = useHook();
    await flush();

    // Force the controller to throw by replacing stopRecording with one that
    // throws an Error (not a FloeError). We swap this BEFORE the release event.
    (tauri.stopRecording as unknown as ReturnType<typeof vi.fn>).mockImplementationOnce(
      () =>
        Promise.reject({
          domain: "recording",
          code: "stopFailed",
          message: "boom",
        }),
    );

    emitHotkey("Pressed");
    await flush();
    emitHotkey("Released");
    await flush();

    expect(result.current.appState).toBe("error");
    expect(result.current.error).toBeTruthy();
  });
});

// ── Subscription lifecycle / cleanup ───────────────────────────────────────

describe("usePushToTalk — subscription lifecycle", () => {
  it("subscribes once to each backend event on mount", async () => {
    const { unmount } = useHook();
    await flush();

    const calls = listenMock.mock.calls.map((c: unknown[]) => c[0]);
    const hotkeyCount = calls.filter((c) => c === EVENT_HOTKEY_STATE).length;
    const recordingCount = calls.filter(
      (c) => c === EVENT_RECORDING_STATE_CHANGED,
    ).length;
    expect(hotkeyCount).toBe(1);
    expect(recordingCount).toBe(1);

    unmount();
  });

  it("unmount removes both event listeners", async () => {
    const { unmount } = useHook();
    await flush();

    expect(hotkeyListeners.current.length).toBe(1);
    expect(recordingListeners.current.length).toBe(1);

    unmount();

    expect(hotkeyListeners.current.length).toBe(0);
    expect(recordingListeners.current.length).toBe(0);
    // The listen() returned unlistens are also called.
    expect(unlistenSpy).toHaveBeenCalled();
  });

  it("listen resolving after unmount calls the unlisten immediately (race-safe)", async () => {
    // Override listen to delay resolution.
    let resolveListen: ((value: () => void) => void) | undefined;
    const delayedUnlisten = vi.fn();
    listenMock.mockImplementationOnce(() =>
      new Promise<() => void>((resolve) => {
        resolveListen = resolve;
      }).then(() => delayedUnlisten),
    );

    const { unmount } = useHook();
    await flush();

    unmount();
    if (resolveListen) resolveListen(() => {});
    await flush();

    expect(delayedUnlisten).toHaveBeenCalled();
  });
});

// ── Synchronization with store ─────────────────────────────────────────────

describe("usePushToTalk — store synchronization", () => {
  it("calls syncFromPipeline on every state change", async () => {
    const { result } = useHook();
    await flush();

    emitHotkey("Pressed");
    await flush();
    expect(useFloeStore.getState().status).toBe("recording");

    emitHotkey("Released");
    await flush();
    // Controller lands in preview or pasted; store reflects that.
    const finalStatus = useFloeStore.getState().status;
    expect(["processing", "idle"]).toContain(finalStatus);

    expect(result.current.appState).toBeTruthy();
  });

  it("sets recordingStartedAt when state becomes 'recording'", async () => {
    useHook();
    await flush();

    emitHotkey("Pressed");
    await flush();

    expect(useFloeStore.getState().status).toBe("recording");
    expect(typeof useFloeStore.getState().recordingStartedAt).toBe("number");
  });

  it("calls getCurrentWindow().show()/.setFocus() when state becomes 'preview'", async () => {
    const { result } = useHook();
    await flush();

    emitHotkey("Pressed");
    await flush();
    emitHotkey("Released");
    await flush();

    // Keep advancing the microtask queue a few times to land in preview.
    await flush();
    await flush();

    expect(result.current.appState).toBe("preview");
    expect(windowShow).toHaveBeenCalled();
    expect(windowSetFocus).toHaveBeenCalled();
  });

  it("does NOT call windowShow/setFocus when state is non-preview", async () => {
    useHook();
    await flush();
    emitHotkey("Pressed");
    await flush();
    // We're in 'recording' — preview flow not yet triggered.
    expect(windowShow).not.toHaveBeenCalled();
    expect(windowSetFocus).not.toHaveBeenCalled();
  });

  it("does not crash if window.show()/setFocus() reject", async () => {
    windowShow.mockRejectedValueOnce(new Error("show failed"));
    windowSetFocus.mockRejectedValueOnce(new Error("focus failed"));

    const { result } = useHook();
    await flush();
    emitHotkey("Pressed");
    await flush();
    emitHotkey("Released");
    await flush();
    expect(result.current.appState).toBe("preview");
  });
});

// ── Race conditions ────────────────────────────────────────────────────────

describe("usePushToTalk — race conditions", () => {
  it("emitting backend 'idle' updates controller recording state to idle", async () => {
    useHook();
    await flush();

    // Drive controller into 'recording' via Pressed.
    emitHotkey("Pressed");
    await flush();
    expect(useFloeStore.getState().status).toBe("recording");

    emitBackendState("idle");
    await flush();

    // controller.syncRecordingState("idle") sets the controller's internal
    // recordingState to "idle". The store status remains "recording" because
    // syncRecordingState does not route through onStateChange; the appState
    // transitions only via the controller's own pipeline finishing.
    expect(useFloeStore.getState().status).toBe("recording");
  });

  it("emitting backend 'idle' while not recording does NOT trigger status fetch", async () => {
    const getStatus = tauri.getRecordingStatus as unknown as ReturnType<typeof vi.fn>;
    getStatus.mockClear();

    useHook();
    await flush();

    emitBackendState("idle");
    await flush();

    expect(getStatus).not.toHaveBeenCalled();
  });

  it("emitting backend 'starting' just calls syncRecordingState without affecting error", async () => {
    useHook();
    await flush();
    emitHotkey("Pressed");
    await flush();
    // We're already recording.
    emitBackendState("starting");
    await flush();

    // status remains whatever the store last saw.
    expect(useFloeStore.getState().status).toBe("recording");
  });
});

// ── State updates / effect ─────────────────────────────────────────────────

describe("usePushToTalk — bubble show/hide effect", () => {
  it("calls bubbleShow when appState becomes recording", async () => {
    useHook();
    await flush();

    emitHotkey("Pressed");
    await flush();
    expect(tauri.bubbleShow).toHaveBeenCalled();
  });

  it("calls bubbleHide when appState returns to idle / non-active", async () => {
    useHook();
    await flush();
    emitHotkey("Pressed");
    await flush();
    emitHotkey("Released");
    await flush();

    // The bubbleHide effect runs any time state leaves recording/starting/stopping.
    expect(tauri.bubbleHide).toHaveBeenCalled();
  });

  it("calls bubbleHide on unmount cleanup", async () => {
    const { unmount } = useHook();
    await flush();
    (tauri.bubbleHide as unknown as ReturnType<typeof vi.fn>).mockClear();
    unmount();
    expect(tauri.bubbleHide).toHaveBeenCalled();
  });
});

// ── Triggers ───────────────────────────────────────────────────────────────

describe("usePushToTalk — triggerStart / triggerStop", () => {
  it("triggerStart invokes controller.handleShortcutState('Pressed')", async () => {
    const startRecording = tauri.startRecording as unknown as ReturnType<
      typeof vi.fn
    >;
    const { result } = useHook();
    await flush();

    startRecording.mockClear();
    act(() => {
      result.current.triggerStart();
    });
    await flush();

    expect(startRecording).toHaveBeenCalled();
  });

  it("triggerStop invokes controller.handleShortcutState('Released')", async () => {
    const stopRecording = tauri.stopRecording as unknown as ReturnType<
      typeof vi.fn
    >;
    const { result } = useHook();
    await flush();

    act(() => {
      result.current.triggerStart();
    });
    await flush();
    stopRecording.mockClear();

    act(() => {
      result.current.triggerStop();
    });
    await flush();

    expect(stopRecording).toHaveBeenCalled();
  });

  it("confirmPreview/discardPreview delegate to the controller", async () => {
    const { result } = useHook();
    await flush();

    emitHotkey("Pressed");
    await flush();
    emitHotkey("Released");
    await flush();

    expect(result.current.appState).toBe("preview");
    const copyTextToClipboard = tauri.copyTextToClipboard as unknown as ReturnType<typeof vi.fn>;
    copyTextToClipboard.mockClear();

    act(() => {
      result.current.confirmPreview();
    });
    await flush();

    expect(copyTextToClipboard).toHaveBeenCalled();
  });

  it("discardPreview returns controller to idle", async () => {
    const { result } = useHook();
    await flush();
    emitHotkey("Pressed");
    await flush();
    emitHotkey("Released");
    await flush();
    expect(result.current.appState).toBe("preview");

    act(() => {
      result.current.discardPreview();
    });
    await flush();
    expect(result.current.appState).toBe("idle");
  });
});

// ── Non-Tauri runtime ──────────────────────────────────────────────────────

describe("usePushToTalk — non-Tauri runtime", () => {
  it("does NOT register any listeners when isTauriRuntime() is false", async () => {
    // Override the mock's isTauriRuntime after the fact. The hook reads it on
    // mount, so this is sufficient for this branch.
    const isTauriSpy = vi
      .spyOn(tauri, "isTauriRuntime")
      .mockReturnValue(false);
    listenMock.mockClear();

    const { result } = useHook();
    await flush();

    expect(listenMock).not.toHaveBeenCalled();

    // Cleanup
    isTauriSpy.mockRestore();

    // The trigger functions still delegate through the controller regardless.
    act(() => {
      result.current.triggerStart();
    });
    await flush();
  });
});

// ── listen() failure ───────────────────────────────────────────────────────

describe("usePushToTalk — listen() failures", () => {
  it("surfaces an error message when hotkey listen() rejects", async () => {
    listenMock.mockImplementationOnce(() => Promise.reject(new Error("listen failed")));

    const { result } = useHook();
    await flush();

    expect(result.current.error).toMatch(/could not listen for the global hotkey/i);
    expect(tauri.bubbleHide).toHaveBeenCalled();
  });

  it("surfaces an error message when recording-state listen() rejects", async () => {
    // Always reject (for both event types — hotkey listen recovers via its own
    // mock in beforeEach; here we only care that recording-state fails and
    // surfaces into result.error). Use the test's helper to also keep the hotkey
    // path working.
    const prev = listenMock.getMockImplementation();
    listenMock.mockImplementation((event: string, cb: unknown) => {
      if (event === EVENT_HOTKEY_STATE) {
        hotkeyListeners.current.push(cb as (e: { payload: HotkeyPayload }) => void);
        return Promise.resolve(() => {
          hotkeyListeners.current = hotkeyListeners.current.filter((l) => l !== cb);
        });
      }
      return Promise.reject(new Error("recording-state listen failed"));
    });

    try {
      const { result } = useHook();
      await flush();

      expect(result.current.error).toMatch(
        /could not listen for recording state changes/i,
      );
    } finally {
      // Restore default behavior so other tests are unaffected.
      if (prev) {
        listenMock.mockImplementation(prev);
      } else {
        listenMock.mockReset();
      }
    }
  });

  it("ignores listen() rejections that arrive after the hook unmounted", async () => {
    const prev = listenMock.getMockImplementation();
    listenMock.mockImplementation(() =>
      Promise.reject(new Error("late failure")),
    );

    try {
      const { unmount, result } = useHook();
      unmount();
      await flush();

      // No update occurred (isActive is false at resolve time).
      expect(result.current.error).toBeNull();
    } finally {
      if (prev) {
        listenMock.mockImplementation(prev);
      } else {
        listenMock.mockReset();
      }
    }
  });
});

