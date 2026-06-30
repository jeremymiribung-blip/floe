// ─────────────────────────────────────────────────────────────────────────────
// Tauri IPC + window + event mock helpers
//
// These helpers build fake Tauri modules for tests. They are used by
// pushToTalk.test.ts, usePushToTalk.test.tsx, and (where helpful) by other
// component tests. Because vi.mock calls are hoisted, the helpers expose a
// mutable registry that tests can populate before/after import resolution.
//
// Patterns:
//
//   import { installTauriStubs, tauriEventRegistry } from "../test-helpers/tauriMocks";
//
//   beforeEach(() => {
//     tauriEventRegistry.reset();
//   });
//
//   vi.mock("../lib/tauri", () => installTauriStubs());
// ─────────────────────────────────────────────────────────────────────────────

import { vi } from "vitest";

// ── Window stub ─────────────────────────────────────────────────────────────

export interface WindowStub {
  show: ReturnType<typeof vi.fn>;
  hide: ReturnType<typeof vi.fn>;
  setFocus: ReturnType<typeof vi.fn>;
  close: ReturnType<typeof vi.fn>;
  showSpy: ReturnType<typeof vi.fn>;
  setFocusSpy: ReturnType<typeof vi.fn>;
}

export function createWindowStub(): WindowStub {
  const showSpy = vi.fn(() => Promise.resolve());
  const setFocusSpy = vi.fn(() => Promise.resolve());
  return {
    showSpy,
    setFocusSpy,
    show: vi.fn(() => {
      showSpy();
      return Promise.resolve();
    }),
    hide: vi.fn(() => Promise.resolve()),
    setFocus: vi.fn(() => {
      setFocusSpy();
      return Promise.resolve();
    }),
    close: vi.fn(() => Promise.resolve()),
  };
}

// ── Event registry (listen + emit) ──────────────────────────────────────────

export type EventListener<T = unknown> = (event: {
  payload: T;
}) => void;

export interface EventRegistry {
  reset(): void;
  register<T>(event: string, listener: EventListener<T>): void;
  unregister<T>(event: string, listener: EventListener<T>): void;
  unlisten<T>(event: string, listener: EventListener<T>): void;
  emit<T>(event: string, payload: T): void;
  listenerCount(event: string): number;
  unlistenSpy: ReturnType<typeof vi.fn>;
}

export function createEventRegistry(): EventRegistry {
  const listeners = new Map<string, Set<EventListener<unknown>>>();
  const unlistenSpy = vi.fn();

  return {
    reset() {
      listeners.clear();
      unlistenSpy.mockClear();
    },
    register<T>(event: string, listener: EventListener<T>): void {
      let bucket = listeners.get(event);
      if (!bucket) {
        bucket = new Set();
        listeners.set(event, bucket);
      }
      bucket.add(listener as EventListener<unknown>);
    },
    unregister<T>(event: string, listener: EventListener<T>): void {
      listeners.get(event)?.delete(listener as EventListener<unknown>);
    },
    unlisten<T>(event: string, listener: EventListener<T>): void {
      listeners.get(event)?.delete(listener as EventListener<unknown>);
      unlistenSpy(event);
    },
    emit<T>(event: string, payload: T): void {
      const bucket = listeners.get(event);
      if (!bucket) return;
      for (const listener of [...bucket]) {
        listener({ payload });
      }
    },
    listenerCount(event: string): number {
      return listeners.get(event)?.size ?? 0;
    },
    unlistenSpy,
  };
}

// Global singleton; tests call `tauriEventRegistry.reset()` in `beforeEach`.
export const tauriEventRegistry = createEventRegistry();

/** Build a `listen` factory suitable for `vi.mock("@tauri-apps/api/event", ...)`. */
export function makeListenFactory(
  registry: EventRegistry = tauriEventRegistry,
): (event: string, cb: EventListener<unknown>) => Promise<() => void> {
  return (event: string, cb: EventListener<unknown>) => {
    registry.register(event, cb);
    let active = true;
    return Promise.resolve(() => {
      if (!active) return;
      active = false;
      registry.unlisten(event, cb);
      registry.unlistenSpy(event);
    });
  };
}

// ── Full lib/tauri module factory ───────────────────────────────────────────

export interface TauriModuleStub {
  isTauriRuntime: () => boolean;
  startRecording: ReturnType<typeof vi.fn>;
  stopRecording: ReturnType<typeof vi.fn>;
  forceStopRecording: ReturnType<typeof vi.fn>;
  getRecordingStatus: ReturnType<typeof vi.fn>;
  getLatestRecordingInfo: ReturnType<typeof vi.fn>;
  transcribeLatestRecording: ReturnType<typeof vi.fn>;
  cleanupTranscript: ReturnType<typeof vi.fn>;
  copyTextToClipboard: ReturnType<typeof vi.fn>;
  pasteClipboard: ReturnType<typeof vi.fn>;
  bubbleShow: ReturnType<typeof vi.fn>;
  bubbleHide: ReturnType<typeof vi.fn>;
  bubbleCancelRecording: ReturnType<typeof vi.fn>;
  diagLog: ReturnType<typeof vi.fn>;
  logFrontendEvent: ReturnType<typeof vi.fn>;
  updateSessionHotkeyLatency: ReturnType<typeof vi.fn>;
  // Settings + onboarding (additional callbacks available if tests need them)
  saveApiKey: ReturnType<typeof vi.fn>;
  validateApiKey: ReturnType<typeof vi.fn>;
  clearApiKey: ReturnType<typeof vi.fn>;
  getApiKeyStatus: ReturnType<typeof vi.fn>;
  getAppSettings: ReturnType<typeof vi.fn>;
  getAudioDevices: ReturnType<typeof vi.fn>;
  saveAppSettings: ReturnType<typeof vi.fn>;
  getHotkeySettings: ReturnType<typeof vi.fn>;
  setHotkey: ReturnType<typeof vi.fn>;
  resetHotkeyToDefault: ReturnType<typeof vi.fn>;
  getStartAtLoginStatus: ReturnType<typeof vi.fn>;
  setStartAtLoginEnabled: ReturnType<typeof vi.fn>;
  getUpdateInfo: ReturnType<typeof vi.fn>;
  checkForUpdate: ReturnType<typeof vi.fn>;
  downloadUpdate: ReturnType<typeof vi.fn>;
  installUpdate: ReturnType<typeof vi.fn>;
  resetUpdateState: ReturnType<typeof vi.fn>;
  getDiagnosticsReport: ReturnType<typeof vi.fn>;
  errorMessage: (err: unknown) => string;
}

export interface InstallTauriStubsOptions {
  isTauriRuntime?: boolean;
  bubbleShow?: (...args: unknown[]) => Promise<void>;
  bubbleHide?: (...args: unknown[]) => Promise<void>;
}

export function createTauriModule(
  options: InstallTauriStubsOptions = {},
): TauriModuleStub {
  return {
    isTauriRuntime: () => options.isTauriRuntime ?? true,
    startRecording: vi.fn(() => Promise.resolve()),
    stopRecording: vi.fn(() => Promise.resolve()),
    forceStopRecording: vi.fn(() => Promise.resolve()),
    getRecordingStatus: vi.fn(() =>
      Promise.resolve({ isRecording: true, lastError: null }),
    ),
    getLatestRecordingInfo: vi.fn(() => Promise.resolve(null)),
    transcribeLatestRecording: vi.fn(() => Promise.resolve()),
    cleanupTranscript: vi.fn(() => Promise.resolve({ fallbackUsed: false })),
    copyTextToClipboard: vi.fn(() => Promise.resolve()),
    pasteClipboard: vi.fn(() => Promise.resolve()),
    bubbleShow: vi.fn(options.bubbleShow ?? (() => Promise.resolve())),
    bubbleHide: vi.fn(options.bubbleHide ?? (() => Promise.resolve())),
    bubbleCancelRecording: vi.fn(() => Promise.resolve()),
    diagLog: vi.fn(),
    logFrontendEvent: vi.fn(() => Promise.resolve()),
    updateSessionHotkeyLatency: vi.fn(() => Promise.resolve()),

    saveApiKey: vi.fn(() =>
      Promise.resolve({ configured: true, maskedPreview: null }),
    ),
    validateApiKey: vi.fn(() => Promise.resolve(true)),
    clearApiKey: vi.fn(() =>
      Promise.resolve({ configured: false, maskedPreview: null }),
    ),
    getApiKeyStatus: vi.fn(() =>
      Promise.resolve({ configured: false, maskedPreview: null }),
    ),
    getAppSettings: vi.fn(() =>
      Promise.resolve({
        hotkey: { accelerator: "Ctrl+Space", label: "Ctrl+Space" },
        deviceId: null,
        skipCleanup: false,
      }),
    ),
    getAudioDevices: vi.fn(() => Promise.resolve([])),
    saveAppSettings: vi.fn(() =>
      Promise.resolve({
        hotkey: { accelerator: "Ctrl+Space", label: "Ctrl+Space" },
        deviceId: null,
        skipCleanup: false,
      }),
    ),
    getHotkeySettings: vi.fn(() =>
      Promise.resolve({
        accelerator: "Ctrl+Space",
        label: "Ctrl+Space",
        isDefault: true,
        isRegistered: true,
        error: null,
      }),
    ),
    setHotkey: vi.fn(() =>
      Promise.resolve({
        accelerator: "Ctrl+Space",
        label: "Ctrl+Space",
        isDefault: false,
        isRegistered: true,
        error: null,
      }),
    ),
    resetHotkeyToDefault: vi.fn(() =>
      Promise.resolve({
        accelerator: "Ctrl+Space",
        label: "Ctrl+Space",
        isDefault: true,
        isRegistered: true,
        error: null,
      }),
    ),
    getStartAtLoginStatus: vi.fn(() =>
      Promise.resolve({ enabled: false, available: true }),
    ),
    setStartAtLoginEnabled: vi.fn(() =>
      Promise.resolve({ enabled: false, available: true }),
    ),
    getUpdateInfo: vi.fn(() =>
      Promise.resolve({
        currentVersion: "1.0.0",
        latestVersion: null,
        status: "idle",
        downloadProgress: 0,
        lastCheckResult: null,
        errorMessage: null,
      }),
    ),
    checkForUpdate: vi.fn(() =>
      Promise.resolve({
        currentVersion: "1.0.0",
        latestVersion: null,
        status: "no_update",
        downloadProgress: 0,
        lastCheckResult: "Up to date",
        errorMessage: null,
      }),
    ),
    downloadUpdate: vi.fn(() =>
      Promise.resolve({
        currentVersion: "1.0.0",
        latestVersion: null,
        status: "downloaded",
        downloadProgress: 100,
        lastCheckResult: null,
        errorMessage: null,
      }),
    ),
    installUpdate: vi.fn(() => Promise.resolve()),
    resetUpdateState: vi.fn(() => Promise.resolve()),
    getDiagnosticsReport: vi.fn(() => Promise.resolve({})),
    errorMessage: (err: unknown) => {
      if (err instanceof Error) return err.message;
      if (typeof err === "string") return err;
      if (
        err &&
        typeof err === "object" &&
        "message" in err &&
        typeof (err as { message: unknown }).message === "string"
      ) {
        return (err as { message: string }).message;
      }
      return "Unknown error";
    },
  };
}

/**
 * Convenience function for `vi.mock("../lib/tauri", () => installTauriStubs())`.
 *
 * Returns a factory that builds the stub module synchronously when called.
 * Test files can pass additional overrides by mutating the returned module
 * after `import * as tauri from "../lib/tauri"`.
 */
export function installTauriStubs(
  options: InstallTauriStubsOptions = {},
): () => Record<string, unknown> {
  return () => createTauriModule(options) as unknown as Record<string, unknown>;
}
