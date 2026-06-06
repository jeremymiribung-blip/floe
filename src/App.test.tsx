import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type {
  GlobalHotkeyEvent,
  GroqApiKeyStatus,
  HotkeyStatus,
  RecordingInfo,
} from "./types/app";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

type Listener<T> = (event: { payload: T }) => void;

const hoisted = vi.hoisted(() => {
  const latestRecording = {
    sampleRate: 48_000,
    inputChannels: 1,
    outputChannels: 1,
    wavFormat: "wav",
    wavSampleRate: 16_000,
    wavChannels: 1,
    durationMs: 1_000,
    sampleCount: 48_000,
    wavByteCount: 96_044,
    wavBitsPerSample: 16,
    recordingStopToEncodeStartMs: 0,
    audioEncodeMs: 4,
    startedAtMs: 1_000,
    endedAtMs: 2_000,
    maxDurationReached: false,
    endedReason: "manual",
  };
  const idleStatus = {
    isRecording: false,
    sampleRate: null,
    inputChannels: null,
    outputChannels: 1,
    durationMs: 0,
    sampleCount: 0,
    startedAtMs: null,
    maxDurationSeconds: 120,
    latestRecording,
    lastError: null,
  };

  return {
    eventListeners: new Map<
      string,
      Array<(event: { payload: unknown }) => void>
    >(),
    groqConfigured: {
      configured: true,
      maskedPreview: "gsk_...abcd",
    },
    groqMissing: {
      configured: false,
      maskedPreview: null,
    },
    hotkeyRegistered: {
      accelerator: "Control+Space",
      label: "Ctrl + Space",
      isDefault: true,
      isRegistered: true,
      error: null,
    },
    latestRecording,
    idleStatus,
    recordingStatus: {
      ...idleStatus,
      isRecording: true,
      sampleRate: 48_000,
      inputChannels: 1,
      durationMs: 100,
      sampleCount: 4_800,
      startedAtMs: 1_000,
      latestRecording: null,
    },
  };
});

const eventListeners = hoisted.eventListeners as Map<
  string,
  Array<Listener<unknown>>
>;
const groqConfigured = hoisted.groqConfigured as GroqApiKeyStatus;
const hotkeyRegistered = hoisted.hotkeyRegistered as HotkeyStatus;
const latestRecording = hoisted.latestRecording as RecordingInfo;

vi.mock("@tauri-apps/api/event", () => {
  return {
    listen: (event: string, listener: Listener<unknown>) => {
      const listeners = hoisted.eventListeners.get(event) ?? [];
      listeners.push(listener);
      hoisted.eventListeners.set(event, listeners);

      return Promise.resolve(() => {
        hoisted.eventListeners.set(
          event,
          (hoisted.eventListeners.get(event) ?? []).filter(
            (registered) => registered !== listener,
          ),
        );
      });
    },
  };
});

vi.mock("./lib/tauri", () => {
  return {
    bubbleHide: vi.fn(() => Promise.resolve()),
    bubbleShow: vi.fn(() => Promise.resolve()),
    cleanupTranscript: vi.fn((transcript: string) =>
      Promise.resolve({
        text: transcript,
        model: "llama-3.3-70b-versatile",
        retryCount: 0,
        validationMs: 0,
        fallbackUsed: false,
      }),
    ),
    clearGroqApiKey: vi.fn(() => Promise.resolve(hoisted.groqMissing)),
    copyTextToClipboard: vi.fn(() => Promise.resolve()),
    getGroqApiKeyStatus: vi.fn(() => Promise.resolve(hoisted.groqConfigured)),
    getHotkeySettings: vi.fn(() => Promise.resolve(hoisted.hotkeyRegistered)),
    getRecordingStatus: vi.fn(() => Promise.resolve(hoisted.idleStatus)),
    getStartAtLoginStatus: vi.fn(() =>
      Promise.resolve({
        enabled: false,
        available: true,
      }),
    ),
    isTauriRuntime: vi.fn(() => true),
    pasteClipboard: vi.fn(() => Promise.resolve()),
    resetHotkeyToDefault: vi.fn(() =>
      Promise.resolve(hoisted.hotkeyRegistered),
    ),
    saveGroqApiKey: vi.fn(() => Promise.resolve(hoisted.groqConfigured)),
    setHotkey: vi.fn(() => Promise.resolve(hoisted.hotkeyRegistered)),
    setStartAtLoginEnabled: vi.fn((enabled: boolean) =>
      Promise.resolve({
        enabled,
        available: true,
      }),
    ),
    startRecording: vi.fn(() => Promise.resolve(hoisted.recordingStatus)),
    stopRecording: vi.fn(() => Promise.resolve(hoisted.latestRecording)),
    transcribeLatestRecording: vi.fn(() =>
      Promise.resolve({
        text: "raw transcript",
        model: "whisper-large-v3-turbo",
        retryCount: 0,
      }),
    ),
  };
});

let roots: Root[] = [];
let containers: HTMLElement[] = [];

beforeEach(async () => {
  eventListeners.clear();
  vi.clearAllMocks();
  const tauri = await import("./lib/tauri");
  vi.mocked(tauri.getGroqApiKeyStatus).mockResolvedValue(groqConfigured);
  vi.mocked(tauri.getHotkeySettings).mockResolvedValue(hotkeyRegistered);
  vi.mocked(tauri.stopRecording).mockResolvedValue(latestRecording);
});

afterEach(() => {
  for (const root of roots) {
    act(() => root.unmount());
  }
  for (const container of containers) {
    container.remove();
  }
  roots = [];
  containers = [];
});

describe("App setup and recording lifecycle", () => {
  it("keeps configured Groq status when hotkey status loading fails", async () => {
    const tauri = await import("./lib/tauri");
    vi.mocked(tauri.getHotkeySettings).mockRejectedValue(
      new Error("hotkey status failed"),
    );
    const { container } = renderApp();

    await flushPromises();

    expect(container.textContent).toContain("Hotkey");
    expect(container.textContent).not.toContain("Groq API key");
  });

  it("hides the bubble immediately on release before slow stop resolves", async () => {
    const tauri = await import("./lib/tauri");
    let resolveStop: (recording: RecordingInfo) => void = () => undefined;
    vi.mocked(tauri.stopRecording).mockImplementation(
      () =>
        new Promise<RecordingInfo>((resolve) => {
          resolveStop = resolve;
        }),
    );
    renderApp();
    await flushPromises();

    await emitHotkeyState("Pressed");
    await flushPromises();
    expect(tauri.bubbleShow).toHaveBeenCalled();

    await emitHotkeyState("Released");
    await flushPromises();

    expect(tauri.stopRecording).toHaveBeenCalledTimes(1);
    expect(tauri.bubbleHide).toHaveBeenCalled();
    expect(tauri.transcribeLatestRecording).not.toHaveBeenCalled();

    resolveStop(latestRecording);
    await flushPromises();

    expect(tauri.transcribeLatestRecording).toHaveBeenCalledTimes(1);
  });
});

function renderApp(): { container: HTMLElement } {
  const container = document.createElement("div");
  document.body.appendChild(container);
  containers.push(container);
  const root = createRoot(container);
  roots.push(root);

  act(() => {
    root.render(<App />);
  });

  return { container };
}

async function emitHotkeyState(state: GlobalHotkeyEvent["state"]) {
  await act(async () => {
    for (const listener of [
      ...(eventListeners.get("floe-global-hotkey-state") ?? []),
    ]) {
      listener({ payload: { state } });
    }
    await Promise.resolve();
  });
}

async function flushPromises() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}
