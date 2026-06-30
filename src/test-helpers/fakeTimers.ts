// ─────────────────────────────────────────────────────────────────────────────
// Controllable fake timers for watchdog + delayed-effect tests
//
// The push-to-talk watchdog schedules a setTimeout with a 125-second delay
// (MAX_RECORDING_DURATION_SECS + WATCHDOG_GRACE_SECS). Real timers would
// make tests flaky and slow. This helper installs vitest's fake timers and
// exposes a small, focused API for advancing the clock deterministically.
//
// Usage:
//
//   import { useFakeTimers } from "../test-helpers/fakeTimers";
//
//   useFakeTimers();
//   // ...test body...
//   advance(WATCHDOG_TIMEOUT_MS);
//   await flush();
// ─────────────────────────────────────────────────────────────────────────────

import { afterEach, beforeEach, vi } from "vitest";

export interface FakeTimerHandle {
  advance: (ms: number) => Promise<void>;
  setNow: (ms: number) => void;
  now: () => number;
  restore: () => void;
}

/**
 * Install fake timers and arrange automatic restoration.
 * `vi.useFakeTimers()` is called in `beforeEach` (with `now: 0`),
 * and `vi.useRealTimers()` is restored in `afterEach`.
 *
 * Tests should call `advance(ms)` then `await flush()` to drive the
 * pending microtasks + timers in a deterministic order.
 */
export function useFakeTimers(): FakeTimerHandle {
  let now = 0;

  beforeEach(() => {
    vi.useFakeTimers({ now: 0 });
    now = 0;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  return {
    advance: async (ms: number) => {
      now += ms;
      vi.advanceTimersByTime(ms);
      await vi.runOnlyPendingTimersAsync();
    },
    setNow: (ms: number) => {
      now = ms;
      vi.setSystemTime(ms);
    },
    now: () => now,
    restore: () => vi.useRealTimers(),
  };
}

/**
 * Helper to flush microtask queues after resolving / rejecting promises in
 * tests that don't need fake timers. Tests often need a couple of
 * `await Promise.resolve()` passes to drain chained `.then` callbacks.
 */
export async function flushMicrotasks(depth = 5): Promise<void> {
  for (let i = 0; i < depth; i += 1) {
    await Promise.resolve();
  }
}
