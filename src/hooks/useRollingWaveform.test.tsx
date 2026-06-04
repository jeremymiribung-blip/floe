import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRollingWaveform } from "./useRollingWaveform";
import { WAVEFORM_BUCKET_MS, WAVEFORM_SAMPLE_COUNT } from "../lib/waveform";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

type Listener = (event: { payload: { level: number } }) => void;

let activeListeners: Listener[] = [];
let listenImpl: ((event: string, cb: Listener) => Promise<() => void>) | null =
  null;
let rafCallbacks = new Map<number, (timestamp: number) => void>();
let nextRafId = 1;
const cancelAnimationFrameSpy = vi.fn();

vi.mock("@tauri-apps/api/event", () => {
  return {
    listen: (event: string, cb: Listener) => {
      if (listenImpl) {
        return listenImpl(event, cb);
      }
      activeListeners.push(cb);
      return Promise.resolve(() => {
        activeListeners = activeListeners.filter((listener) => listener !== cb);
      });
    },
  };
});

vi.mock("../lib/tauri", () => {
  return {
    isTauriRuntime: () => true,
  };
});

beforeEach(() => {
  activeListeners = [];
  listenImpl = null;
  rafCallbacks = new Map();
  nextRafId = 1;
  cancelAnimationFrameSpy.mockClear();
  vi.stubGlobal(
    "requestAnimationFrame",
    (callback: (timestamp: number) => void) => {
      const id = nextRafId;
      nextRafId += 1;
      rafCallbacks.set(id, callback);
      return id;
    },
  );
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    cancelAnimationFrameSpy(id);
    rafCallbacks.delete(id);
  });
});

afterEach(() => {
  activeListeners = [];
  listenImpl = null;
  rafCallbacks.clear();
  vi.unstubAllGlobals();
});

describe("useRollingWaveform", () => {
  it("returns a fixed-size silent buffer when inactive", async () => {
    const { harness } = renderHookHarness(false);
    await flushPromises();

    expect(harness.current).toHaveLength(WAVEFORM_SAMPLE_COUNT);
    expect(harness.current.every((sample) => sample === 0)).toBe(true);
    harness.unmount();
  });

  it("appends a max-pool sample from recording-level events after one bucket", async () => {
    const { harness } = renderHookHarness(true);
    await flushPromises();

    emit(0.8);
    flushNextRaf(0);
    flushNextRaf(WAVEFORM_BUCKET_MS + 20);

    expect(harness.current).toHaveLength(WAVEFORM_SAMPLE_COUNT);
    expect(harness.current[harness.current.length - 1]).toBeGreaterThan(0);
    harness.unmount();
  });

  it("does not shift the buffer before a bucket boundary is crossed", async () => {
    const { harness } = renderHookHarness(true);
    await flushPromises();

    emit(1);
    flushNextRaf(0);
    flushNextRaf(WAVEFORM_BUCKET_MS - 20);

    expect(harness.current[harness.current.length - 1]).toBe(0);
    harness.unmount();
  });

  it("appends zero for buckets with no level events", async () => {
    const { harness } = renderHookHarness(true);
    await flushPromises();

    flushNextRaf(0);
    flushNextRaf(WAVEFORM_BUCKET_MS + 20);

    expect(harness.current).toHaveLength(WAVEFORM_SAMPLE_COUNT);
    expect(harness.current[harness.current.length - 1]).toBe(0);
    harness.unmount();
  });

  it("takes the loudest peak within a bucket (max-pool)", async () => {
    const { harness } = renderHookHarness(true);
    await flushPromises();

    emit(0.3);
    flushNextRaf(0);
    emit(0.9);
    flushNextRaf(40);
    emit(0.2);
    flushNextRaf(WAVEFORM_BUCKET_MS + 20);

    expect(harness.current[harness.current.length - 1]).toBeGreaterThan(0.8);
    harness.unmount();
  });

  it("resets the buffer when active starts again", async () => {
    const { harness, setActive } = renderHookHarness(true);
    await flushPromises();

    emit(1);
    flushNextRaf(0);
    flushNextRaf(WAVEFORM_BUCKET_MS + 20);
    expect(harness.current[harness.current.length - 1]).toBeGreaterThan(0);

    setActive(false);
    expect(harness.current.every((sample) => sample === 0)).toBe(true);

    setActive(true);
    await flushPromises();
    expect(harness.current.every((sample) => sample === 0)).toBe(true);
    harness.unmount();
  });

  it("cleans up listeners and animation frames on unmount", async () => {
    const unsubscribe = vi.fn();
    listenImpl = (_event, cb) => {
      activeListeners.push(cb);
      return Promise.resolve(unsubscribe);
    };
    const { harness } = renderHookHarness(true);
    await flushPromises();

    harness.unmount();

    expect(unsubscribe).toHaveBeenCalledTimes(1);
    expect(cancelAnimationFrameSpy).toHaveBeenCalled();
  });
});

type Harness = { current: number[]; unmount: () => void };

function renderHookHarness(initialActive: boolean): {
  harness: Harness;
  setActive: (active: boolean) => void;
} {
  let container: HTMLDivElement | null = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  let active = initialActive;
  let currentValue: number[] = [];

  function HookHarness() {
    currentValue = useRollingWaveform(active);
    return null;
  }

  act(() => {
    root.render(<HookHarness />);
  });

  return {
    harness: {
      get current() {
        return currentValue;
      },
      unmount: () => {
        act(() => root.unmount());
        if (container) {
          container.remove();
          container = null;
        }
      },
    },
    setActive(next: boolean) {
      active = next;
      act(() => {
        root.render(<HookHarness />);
      });
    },
  };
}

function emit(level: number) {
  for (const listener of [...activeListeners]) {
    listener({ payload: { level } });
  }
}

function flushNextRaf(timestamp: number) {
  const [id, callback] = [...rafCallbacks.entries()][0] ?? [];
  if (callback === undefined) {
    return;
  }
  rafCallbacks.delete(id);
  act(() => {
    callback(timestamp);
  });
}

async function flushPromises() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}
