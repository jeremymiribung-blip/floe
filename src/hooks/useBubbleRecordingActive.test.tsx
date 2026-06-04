import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useBubbleRecordingActive } from "./useBubbleRecordingActive";

(
  globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }
).IS_REACT_ACT_ENVIRONMENT = true;

type Listener = (event: { payload: { recording: boolean } }) => void;

let activeListeners: Listener[] = [];
let listenImpl: ((event: string, cb: Listener) => Promise<() => void>) | null =
  null;

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
});

afterEach(() => {
  activeListeners = [];
  listenImpl = null;
});

describe("useBubbleRecordingActive", () => {
  it("starts inactive in the Tauri overlay window", async () => {
    const { harness } = renderHookHarness();
    await flushPromises();

    expect(harness.current).toBe(false);
    harness.unmount();
  });

  it("updates from recording-bubble-state events", async () => {
    const { harness } = renderHookHarness();
    await flushPromises();

    emit(true);
    expect(harness.current).toBe(true);

    emit(false);
    expect(harness.current).toBe(false);
    harness.unmount();
  });

  it("cleans up the state listener on unmount", async () => {
    const unsubscribe = vi.fn();
    listenImpl = () => Promise.resolve(unsubscribe);
    const { harness } = renderHookHarness();
    await flushPromises();

    harness.unmount();

    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });
});

type Harness = { current: boolean; unmount: () => void };

function renderHookHarness(): { harness: Harness } {
  let container: HTMLDivElement | null = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  let currentValue = false;

  function HookHarness() {
    currentValue = useBubbleRecordingActive();
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
  };
}

function emit(recording: boolean) {
  act(() => {
    for (const listener of [...activeListeners]) {
      listener({ payload: { recording } });
    }
  });
}

async function flushPromises() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}
