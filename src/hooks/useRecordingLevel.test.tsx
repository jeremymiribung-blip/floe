import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRecordingLevel, clamp01, smoothOnePole } from "./useRecordingLevel";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

type Listener = (event: { payload: { level: number } }) => void;

let activeListeners: Listener[] = [];
let listenImpl: ((event: string, cb: Listener) => Promise<() => void>) | null =
  null;
let pendingRaf: (() => void) | null = null;

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
  pendingRaf = null;
  vi.stubGlobal(
    "requestAnimationFrame",
    (callback: (timestamp: number) => void) => {
      pendingRaf = () => callback(Date.now());
      return 1;
    },
  );
  vi.stubGlobal("cancelAnimationFrame", () => {
    pendingRaf = null;
  });
});

afterEach(() => {
  activeListeners = [];
  listenImpl = null;
  pendingRaf = null;
  vi.unstubAllGlobals();
});

function flushRaf() {
  const callback = pendingRaf;
  pendingRaf = null;
  if (callback) {
    act(() => {
      callback();
    });
  }
}

function emit(level: number) {
  for (const listener of [...activeListeners]) {
    listener({ payload: { level } });
  }
}

describe("clamp01", () => {
  it("clamps below zero", () => {
    expect(clamp01(-0.5)).toBe(0);
  });

  it("clamps above one", () => {
    expect(clamp01(2)).toBe(1);
  });

  it("passes values inside the range", () => {
    expect(clamp01(0.5)).toBe(0.5);
  });

  it("rejects NaN and infinity", () => {
    expect(clamp01(Number.NaN)).toBe(0);
    expect(clamp01(Number.POSITIVE_INFINITY)).toBe(0);
    expect(clamp01(Number.NEGATIVE_INFINITY)).toBe(0);
  });
});

describe("smoothOnePole", () => {
  it("uses attack coefficient when rising", () => {
    const next = smoothOnePole(0.1, 0.9);
    expect(next).toBeGreaterThan(0.1);
    expect(next).toBeLessThanOrEqual(1);
  });

  it("uses release coefficient when falling", () => {
    const next = smoothOnePole(0.9, 0.1);
    expect(next).toBeLessThan(0.9);
    expect(next).toBeGreaterThanOrEqual(0);
  });

  it("clamps result to [0, 1]", () => {
    const high = smoothOnePole(0.9, 5);
    const low = smoothOnePole(0.1, -5);
    expect(high).toBeLessThanOrEqual(1);
    expect(low).toBeGreaterThanOrEqual(0);
  });
});

type Harness = { current: number; unmount: () => void };

function renderHookHarness(initialActive: boolean): {
  harness: Harness;
  setActive: (active: boolean) => void;
  rerender: () => void;
} {
  let container: HTMLDivElement | null = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  let active = initialActive;
  let currentValue = -1;
  let activeRef = { value: false };

  function HookHarness() {
    activeRef.value = true;
    currentValue = useRecordingLevel(active);
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
        activeRef.value = false;
      },
    },
    setActive(next: boolean) {
      active = next;
      act(() => {
        root.render(<HookHarness />);
      });
    },
    rerender() {
      act(() => {
        root.render(<HookHarness />);
      });
    },
  };
}

describe("useRecordingLevel", () => {
  it("returns 0 when not active", async () => {
    const { harness } = renderHookHarness(false);
    await act(async () => {
      await Promise.resolve();
    });
    expect(harness.current).toBe(0);
    harness.unmount();
  });

  it("updates level from event payload", async () => {
    const { harness } = renderHookHarness(true);
    await act(async () => {
      await Promise.resolve();
    });
    act(() => {
      emit(0.8);
    });
    flushRaf();
    expect(harness.current).toBeGreaterThan(0);
    harness.unmount();
  });

  it("cleans up listener on unmount", async () => {
    const unsubscribe = vi.fn();
    listenImpl = () => Promise.resolve(unsubscribe);
    const { harness } = renderHookHarness(true);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(unsubscribe).not.toHaveBeenCalled();
    harness.unmount();
    expect(unsubscribe).toHaveBeenCalled();
  });

  it("cleans up listener when active flips to false", async () => {
    const unsubscribe = vi.fn();
    listenImpl = () => Promise.resolve(unsubscribe);
    const { harness, setActive } = renderHookHarness(true);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    setActive(false);
    expect(unsubscribe).toHaveBeenCalled();
    harness.unmount();
  });

  it("clamps payload values to [0, 1]", async () => {
    const { harness } = renderHookHarness(true);
    await act(async () => {
      await Promise.resolve();
    });
    act(() => {
      emit(5);
    });
    flushRaf();
    expect(harness.current).toBeLessThanOrEqual(1);
    harness.unmount();
  });

  it("ignores NaN payloads", async () => {
    const { harness } = renderHookHarness(true);
    await act(async () => {
      await Promise.resolve();
    });
    act(() => {
      emit(Number.NaN);
    });
    flushRaf();
    expect(harness.current).toBe(0);
    harness.unmount();
  });
});
