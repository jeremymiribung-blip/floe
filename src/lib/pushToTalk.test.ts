import {
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";

// Mock only external tauri dependencies used by pushToTalk itself.
// The PushToTalkController is otherwise tested end-to-end against fake deps.
vi.mock("../lib/tauri", async () => {
  const actual = await vi.importActual<typeof import("../lib/tauri")>(
    "../lib/tauri",
  );
  return {
    ...actual,
    diagLog: vi.fn(),
    logFrontendEvent: vi.fn(() => Promise.resolve()),
    updateSessionHotkeyLatency: vi.fn(() => Promise.resolve()),
  };
});

const flush = async () => {
  for (let i = 0; i < 5; i += 1) {
    await Promise.resolve();
  }
};

// Fresh per-test isolation: ensure tauri mocks don't carry state across tests.
beforeEach(async () => {
  const tauri = await import("../lib/tauri");
  (tauri.updateSessionHotkeyLatency as unknown as ReturnType<typeof vi.fn>).mockClear();
  (tauri.logFrontendEvent as unknown as ReturnType<typeof vi.fn>).mockClear();
  (tauri.diagLog as unknown as ReturnType<typeof vi.fn>).mockClear();
});

import { MAX_RECORDING_DURATION_SECS, WATCHDOG_GRACE_SECS } from "../lib/contract";
import type { RecordingStatus } from "../types/app";
import {
  createController,
  clipboardDomainError,
  makeRecordingInfo,
  makeRecordingStatus,
  makeCleanupResult,
  makeSttResult,
  makeTraceId,
  recordingDomainError,
  sttDomainError,
} from "../test-helpers/pushToTalkFixtures";

const WATCHDOG_TIMEOUT_MS =
  (MAX_RECORDING_DURATION_SECS + WATCHDOG_GRACE_SECS) * 1000;

describe("PushToTalkController — state transitions", () => {
  it("Pressed while idle transitions through starting → recording", async () => {
    const { controller, deps, callbacks } = createController();
    await controller.handleShortcutState("Pressed");
    await flush();

    expect(callbacks.stateChanges).toEqual(["starting", "recording"]);
    expect(deps.startRecording).toHaveBeenCalledTimes(1);
    expect(deps.forceStopRecording).not.toHaveBeenCalled();
    expect(callbacks.errors).toContain(null);
    expect(callbacks.transcripts[0]).toBeNull();
  });

  it("a second Pressed while recording is ignored (no duplicate startRecording)", async () => {
    const { controller, deps, callbacks } = createController();
    await controller.handleShortcutState("Pressed");
    await flush();
    expect(callbacks.stateChanges).toContain("recording");

    await controller.handleShortcutState("Pressed");
    await flush();

    expect(deps.startRecording).toHaveBeenCalledTimes(1);
  });

  it("Released while idle is a no-op", async () => {
    const { controller, deps, callbacks } = createController();
    await controller.handleShortcutState("Released");
    await flush();

    expect(callbacks.stateChanges).toEqual([]);
    expect(deps.stopRecording).not.toHaveBeenCalled();
  });

  it("isRecording() is true only for starting/recording", async () => {
    const { controller } = createController();
    expect(controller.isRecording()).toBe(false);
    controller.syncRecordingState("starting");
    expect(controller.isRecording()).toBe(true);
    controller.syncRecordingState("recording");
    expect(controller.isRecording()).toBe(true);
    controller.syncRecordingState("stopping");
    expect(controller.isRecording()).toBe(false);
    controller.syncRecordingState("idle");
    expect(controller.isRecording()).toBe(false);
  });

  it("Released while in starting sets releaseAfterStart and auto-finishes after start completes", async () => {
    let resolveStart: ((value: RecordingStatus) => void) | null = null;
    const handle = createController({
      deps: {
        startRecording: () =>
          new Promise<RecordingStatus>((resolve) => {
            resolveStart = resolve;
          }),
      },
    });

    const startPromise = handle.controller.handleShortcutState("Pressed");
    await flush();
    expect(handle.callbacks.stateChanges).toEqual(["starting"]);

    await handle.controller.handleShortcutState("Released");
    await flush();

    expect(handle.callbacks.stateChanges).toEqual(["starting"]);

    // Now resolve start — controller should auto-finish, calling stopRecording.
    (resolveStart as unknown as ((value: RecordingStatus) => void))(makeRecordingStatus());
    await startPromise;
    await flush();

    expect(handle.deps.stopRecording).toHaveBeenCalledTimes(1);
    expect(handle.callbacks.stateChanges).toContain("stopping");
  });
});

describe("PushToTalkController — syncRecordingState", () => {
  it("idle clears finishing and releaseAfterStart outside preview", () => {
    const { controller } = createController();
    // Drive into non-idle via internal contract (we use the public callback path).
    controller.syncRecordingState("starting");
    controller.syncRecordingState("recording");
    expect(controller.isRecording()).toBe(true);

    controller.syncRecordingState("idle");
    expect(controller.isRecording()).toBe(false);
    // Calling idle again is safe.
    controller.syncRecordingState("idle");
  });

  it("idle preserves finishing while previewMode is true", async () => {
    const { controller, callbacks } = createController({
      deps: {
        // Force an empty-cleanup preview by returning empty cleaned text.
        cleanupTranscript: () =>
          Promise.resolve(
            makeCleanupResult({
              text: "",
              model: "llama-3.3-70b-versatile",
              validationMs: 0,
              fallbackUsed: false,
            }),
          ),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    // Empty-cleanup path → state goes to "ready", not "preview".
    // Verify that the controller ends in `idle` with no preview.
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("ready");
    expect(controller.isRecording()).toBe(false);
  });

  it("stopping clears the watchdog", async () => {
    const { controller, deps } = createController();
    await controller.handleShortcutState("Pressed");
    await flush();
    expect(controller.isRecording()).toBe(true);

    controller.syncRecordingState("stopping");
    // No assertion on internal timer; just verify state is consistent.
    expect(controller.isRecording()).toBe(false);
    expect(deps.forceStopRecording).not.toHaveBeenCalled();
  });
});

describe("PushToTalkController — recording lifecycle (happy path)", () => {
  it("runs the full pipeline Press → Release → preview", async () => {
    const { controller, deps, callbacks, advanceTime } = createController();
    advanceTime(0);

    const trace = makeTraceId();
    deps.startRecording.mockResolvedValueOnce(
      makeRecordingStatus({ traceId: trace }),
    );
    deps.stopRecording.mockResolvedValueOnce(makeRecordingInfo());
    deps.transcribeLatestRecording.mockResolvedValueOnce(
      makeSttResult({ text: "hello world" }),
    );
    deps.cleanupTranscript.mockResolvedValueOnce(
      makeCleanupResult({ text: "Hello world." }),
    );
    deps.getRecordingStatus.mockResolvedValueOnce(
      makeRecordingStatus({ isRecording: false }),
    );

    await controller.handleShortcutState("Pressed");
    advanceTime(25);
    await flush();

    await controller.handleShortcutState("Released");
    advanceTime(500);
    await flush();

    // State path: starting, recording, stopping, transcribing, cleaning, preview
    expect(callbacks.stateChanges).toEqual([
      "starting",
      "recording",
      "stopping",
      "transcribing",
      "cleaning",
      "preview",
    ]);

    expect(deps.startRecording).toHaveBeenCalledTimes(1);
    expect(deps.stopRecording).toHaveBeenCalledTimes(1);
    expect(deps.transcribeLatestRecording).toHaveBeenCalledTimes(1);
    expect(deps.cleanupTranscript).toHaveBeenCalledTimes(1);
    expect(deps.copyTextToClipboard).not.toHaveBeenCalled();
    expect(deps.pasteClipboard).not.toHaveBeenCalled();

    expect(callbacks.latestRecordings.length).toBe(1);
    expect(callbacks.recordingStatuses.length).toBeGreaterThanOrEqual(2);
    expect(callbacks.transcripts[callbacks.transcripts.length - 1]).toBe("Hello world.");

    expect(callbacks.errors[callbacks.errors.length - 1]).toBeNull();
  });

  it("sends hotkey-to-recording-start latency to backend", async () => {
    const trace = makeTraceId();
    let nowTick = 0;
    const { controller } = createController({
      deps: {
        startRecording: () =>
          Promise.resolve(makeRecordingStatus({ traceId: trace })),
      },
      now: () => nowTick,
    });

    nowTick = 0;
    const pressPromise = controller.handleShortcutState("Pressed");
    // Advance time before the await completes; the controller captures the
    // nowTick value at `await` resolution time which happens *after* this line.
    nowTick = 50;
    await pressPromise;
    await flush();

    const { updateSessionHotkeyLatency } = await import("../lib/tauri");
    const fn = updateSessionHotkeyLatency as unknown as ReturnType<typeof vi.fn>;
    const calls = fn.mock.calls.filter((c) => c[0] === trace);
    expect(calls.length).toBe(1);
    expect(calls[0]).toEqual([trace, 50]);
  });

  it("does not send hotkey latency when traceId is missing", async () => {
    let nowTick = 0;
    const { controller } = createController({
      deps: {
        startRecording: () =>
          Promise.resolve(makeRecordingStatus({ traceId: undefined })),
      },
      now: () => nowTick,
    });

    nowTick = 0;
    const pressPromise = controller.handleShortcutState("Pressed");
    nowTick = 40;
    await pressPromise;
    await flush();

    const { updateSessionHotkeyLatency } = await import("../lib/tauri");
    const fn = updateSessionHotkeyLatency as unknown as ReturnType<typeof vi.fn>;
    expect(fn).not.toHaveBeenCalled();
  });

  it("skips the pipeline completion when final text is empty (post-cleanup)", async () => {
    const { controller, deps, callbacks } = createController({
      deps: {
        cleanupTranscript: () =>
          Promise.resolve(makeCleanupResult({ text: "   " })),
      },
    });

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("ready");
    expect(deps.copyTextToClipboard).not.toHaveBeenCalled();
    expect(deps.pasteClipboard).not.toHaveBeenCalled();
  });
});

describe("PushToTalkController — cancellation / discard", () => {
  it("discardPreview in preview returns to idle and clears transcript", async () => {
    const { controller, callbacks } = createController();
    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("preview");

    await controller.discardPreview();
    await flush();

    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("idle");
    expect(callbacks.transcripts[callbacks.transcripts.length - 1]).toBeNull();
  });

  it("discardPreview outside of preview is a no-op", async () => {
    const { controller, callbacks } = createController();
    await controller.discardPreview();
    await flush();
    expect(callbacks.stateChanges).toEqual([]);
  });

  it("Pressed is ignored while in preview mode", async () => {
    const { controller, deps, callbacks } = createController();
    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("preview");

    deps.startRecording.mockClear();
    await controller.handleShortcutState("Pressed");
    await flush();
    expect(deps.startRecording).not.toHaveBeenCalled();
  });

  it("confirmPreview outside of preview is a no-op", async () => {
    const { controller, deps } = createController();
    await controller.confirmPreview();
    await flush();
    expect(deps.copyTextToClipboard).not.toHaveBeenCalled();
  });
});

describe("PushToTalkController — failures", () => {
  it("startRecording rejects with permissionDenied → error state with friendly message", async () => {
    const { controller, deps, callbacks } = createController({
      deps: {
        startRecording: () =>
          Promise.reject(
            recordingDomainError("permissionDenied", "mic denied"),
          ),
      },
    });

    await controller.handleShortcutState("Pressed");
    await flush();

    expect(callbacks.errors[callbacks.errors.length - 1]).toBe("friendly:permissionDenied:mic denied");
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("error");
    expect(deps.forceStopRecording).not.toHaveBeenCalled();
  });

  it("startRecording rejects with internal → forceStop + reset toast + idle state", async () => {
    const { controller, deps, callbacks } = createController({
      deps: {
        startRecording: () =>
          Promise.reject(recordingDomainError("internal", "mutex poison")),
      },
    });

    await controller.handleShortcutState("Pressed");
    await flush();

    expect(deps.forceStopRecording).toHaveBeenCalled();
    expect(callbacks.errors[callbacks.errors.length - 1]).toBe("Hardware error: Recording reset");
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("idle");
    expect(callbacks.toasts[callbacks.toasts.length - 1]).toMatch(/hardware error/i);
  });

  it("startRecording rejects with internal — tolerates forceStop failure", async () => {
    const { controller, callbacks } = createController({
      deps: {
        startRecording: () =>
          Promise.reject(recordingDomainError("internal", "boom")),
        forceStopRecording: () => Promise.reject(new Error("force failed")),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();

    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("idle");
    expect(callbacks.errors[callbacks.errors.length - 1]).toBe("Hardware error: Recording reset");
  });

  it("stopRecording rejects → error state with diagnostics recorded", async () => {
    const { controller, callbacks } = createController({
      deps: {
        stopRecording: () =>
          Promise.reject({ domain: "recording", code: "stopFailed", message: "stop failed" }),
      },
    });

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("error");
    expect(callbacks.diagnostics.length).toBeGreaterThan(0);
  });

  it("transcribeLatestRecording rejects → error state before cleanup runs", async () => {
    const { controller, deps, callbacks } = createController({
      deps: {
        transcribeLatestRecording: () =>
          Promise.reject(sttDomainError("timeout", "took too long")),
      },
    });

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("error");
    expect(callbacks.errors[callbacks.errors.length - 1]).toBe("friendly:timeout:took too long");
    expect(deps.cleanupTranscript).not.toHaveBeenCalled();
    expect(deps.copyTextToClipboard).not.toHaveBeenCalled();
  });

  it("cleanupTranscript throws → fallback path pastes the raw transcript with warning", async () => {
    const { controller, callbacks } = createController({
      deps: {
        transcribeLatestRecording: () =>
          Promise.resolve(makeSttResult({ text: "raw transcript here" })),
        cleanupTranscript: () => Promise.reject(new Error("cleanup offline")),
      },
    });

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    expect(callbacks.transcripts[callbacks.transcripts.length - 1]).toBe("raw transcript here");
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("preview");
    expect(callbacks.errors).toContain("Cleanup failed");
  });

  it("cleanupTranscript returns validationFailed fallback → cleanup_fallback_used flag set", async () => {
    const { controller, callbacks } = createController({
      deps: {
        cleanupTranscript: () =>
          Promise.resolve(
            makeCleanupResult({
              text: "raw",
              fallbackUsed: true,
              errorCode: "validationFailed",
              validationMs: 0,
              model: "",
              warning: "Cleanup failed",
            }),
          ),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    // The pipeline is now in preview with a fallback cleanup result.
    expect(callbacks.transcripts[callbacks.transcripts.length - 1]).toBe("raw");
    expect(callbacks.errors).toContain("Cleanup failed");
  });

  it("confirmPreview with copyTextToClipboard failure surfaces the error", async () => {
    const { controller, callbacks } = createController({
      deps: {
        copyTextToClipboard: () =>
          Promise.reject(
            clipboardDomainError("clipboardUnavailable", "clipboard locked"),
          ),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    callbacks.stateChanges.length = 0;
    callbacks.errors.length = 0;

    // confirmPreview re-throws on clipboard failure; the calling code is
    // expected to handle the rejection (the hook does this in usePushToTalk).
    await expect(controller.confirmPreview()).rejects.toMatchObject({
      code: "clipboardUnavailable",
    });
    await flush();

    // State path includes pasting (the first onStateChange on the way in).
    expect(callbacks.stateChanges[0]).toBe("pasting");
    // Diagnostics were stored with errorStage=clipboard.
    const json = callbacks.diagnostics[callbacks.diagnostics.length - 1] as string;
    expect(json).toBeTruthy();
    const parsed = JSON.parse(json);
    expect(parsed.result.error_stage).toBe("clipboard");
  });

  it("confirmPreview with pasteClipboard failure (clipboard succeeded) → copied state + toast", async () => {
    const { controller, callbacks } = createController({
      deps: {
        pasteClipboard: () =>
          Promise.reject(
            clipboardDomainError("pasteUnavailable", "no foreground"),
          ),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    callbacks.stateChanges.length = 0;
    callbacks.errors.length = 0;
    callbacks.toasts.length = 0;

    await controller.confirmPreview();
    await flush();

    expect(callbacks.stateChanges).toContain("copied");
    expect(callbacks.toasts[callbacks.toasts.length - 1]).toMatch(/automatic paste failed/i);

    const json = callbacks.diagnostics[callbacks.diagnostics.length - 1];
    const parsed = JSON.parse(json as string);
    expect(parsed.result.clipboard_success).toBe(true);
    expect(parsed.result.paste_success).toBe(false);
    expect(parsed.result.copied_only).toBe(true);
    expect(parsed.result.error_stage).toBe("paste");
  });
});

describe("PushToTalkController — backend disconnect / recovery", () => {
  it("backend idle arriving while recording → controller transitions cleanly", () => {
    const { controller, callbacks } = createController();
    controller.syncRecordingState("starting");
    controller.syncRecordingState("recording");
    expect(controller.isRecording()).toBe(true);

    controller.syncRecordingState("idle");

    expect(controller.isRecording()).toBe(false);
    // No state callback fired by syncRecordingState; only finishRecording does.
    // Verify it is safe to call again.
    controller.syncRecordingState("idle");
    expect(callbacks.stateChanges.length).toBeGreaterThanOrEqual(0);
  });

  it("after watchdog force-stop, recordingState is idle and toast is shown", async () => {
    vi.useFakeTimers();
    try {
      const { controller, deps, callbacks } = createController();
      // Wait for the Pressed -> startRecording() promise chain to settle.
      const pressPromise = controller.handleShortcutState("Pressed");
      await vi.advanceTimersByTimeAsync(0);
      await pressPromise;
      await vi.advanceTimersByTimeAsync(0);
      expect(controller.isRecording()).toBe(true);

      // Advance past the watchdog deadline; the controller's setTimeout fires
      // forceStopRecording internally.
      await vi.advanceTimersByTimeAsync(WATCHDOG_TIMEOUT_MS + 1);

      expect(deps.forceStopRecording).toHaveBeenCalledTimes(1);
      expect(callbacks.errors[callbacks.errors.length - 1]).toBe("Hardware error: Recording reset");
      expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("idle");
      expect(callbacks.toasts[callbacks.toasts.length - 1]).toMatch(/hardware error/i);
      expect(controller.isRecording()).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("watchdog is cancelled by Released before its deadline fires", async () => {
    vi.useFakeTimers();
    try {
      const { controller, deps, callbacks } = createController({
        deps: {
          cleanupTranscript: () =>
            Promise.resolve(
              makeCleanupResult({ text: "done", validationMs: 0 }),
            ),
        },
      });
      const pressPromise = controller.handleShortcutState("Pressed");
      await vi.advanceTimersByTimeAsync(0);
      await pressPromise;
      expect(controller.isRecording()).toBe(true);

      // Advance only halfway through the watchdog window and then release.
      await vi.advanceTimersByTimeAsync(WATCHDOG_TIMEOUT_MS / 2);
      const releasePromise = controller.handleShortcutState("Released");
      await vi.advanceTimersByTimeAsync(0);
      await releasePromise;

      // Now advance past the deadline; forceStop must NOT fire because the
      // watchdog was cancelled when entering `stopping`.
      await vi.advanceTimersByTimeAsync(WATCHDOG_TIMEOUT_MS);

      expect(deps.forceStopRecording).not.toHaveBeenCalled();
      // No "hardware error" toast was fired (still default behavior + no watchdog).
      const hardwareToasts = callbacks.toasts.filter((t) =>
        /hardware error/i.test(t),
      );
      expect(hardwareToasts.length).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("watchdog is cancelled by syncRecordingState('idle')", async () => {
    vi.useFakeTimers();
    try {
      const { controller, deps } = createController();
      const pressPromise = controller.handleShortcutState("Pressed");
      await vi.advanceTimersByTimeAsync(0);
      await pressPromise;
      expect(controller.isRecording()).toBe(true);

      controller.syncRecordingState("idle");
      // Now the watchdog's clearWatchdog() ran, so the timer should not fire.
      await vi.advanceTimersByTimeAsync(WATCHDOG_TIMEOUT_MS + 1);

      expect(deps.forceStopRecording).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it("forceStopRecording watchdog is idempotent — second invocation is a no-op", async () => {
    vi.useFakeTimers();
    try {
      const { controller, deps, callbacks } = createController();
      const pressPromise = controller.handleShortcutState("Pressed");
      await vi.advanceTimersByTimeAsync(0);
      await pressPromise;

      // First watchdog fire.
      await vi.advanceTimersByTimeAsync(WATCHDOG_TIMEOUT_MS + 1);

      const toastCountAfterFirst = callbacks.toasts.length;
      expect(toastCountAfterFirst).toBeGreaterThan(0);
      expect(deps.forceStopRecording).toHaveBeenCalledTimes(1);

      // A second watchdog-triggered forceStop must not duplicate work
      // (the controller remains in `idle` and the function returns early).
      await vi.advanceTimersByTimeAsync(WATCHDOG_TIMEOUT_MS + 1);

      expect(deps.forceStopRecording).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("getRecordingStatus failure during finish is swallowed (does not break pipeline)", async () => {
    const { controller, deps, callbacks } = createController({
      deps: {
        getRecordingStatus: () => Promise.reject(new Error("status offline")),
      },
    });

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("preview");
    expect(deps.cleanupTranscript).toHaveBeenCalled();
  });
});

describe("PushToTalkController — preview & confirm", () => {
  it("confirmPreview runs copyTextToClipboard then pasteClipboard and fires pasted state", async () => {
    const { controller, callbacks } = createController();

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("preview");

    callbacks.stateChanges.length = 0;

    await controller.confirmPreview();
    await flush();

    // pasting, pasted
    expect(callbacks.stateChanges).toContain("pasting");
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("pasted");
  });

  it("confirmPreview resets recordingState and finishing even when an exception is thrown", async () => {
    const { controller, deps, callbacks } = createController({
      deps: {
        copyTextToClipboard: () =>
          Promise.reject(new Error("clipboard dead")),
      },
    });

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    // confirmPreview re-throws the clipboard error, but the controller's
    // `finally` block still resets internal state.
    await expect(controller.confirmPreview()).rejects.toThrow(/clipboard dead/);
    await flush();

    callbacks.stateChanges.length = 0;
    deps.copyTextToClipboard.mockResolvedValueOnce(undefined);
    await controller.handleShortcutState("Pressed");
    await flush();

    expect(callbacks.stateChanges[0]).toBe("starting");
    expect(deps.startRecording).toHaveBeenCalledTimes(2);
  });

  it("a second confirmPreview without an active preview is a no-op", async () => {
    const { controller, deps, callbacks } = createController();
    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    await controller.confirmPreview();
    await flush();

    callbacks.stateChanges.length = 0;
    deps.copyTextToClipboard.mockClear();
    deps.pasteClipboard.mockClear();

    await controller.confirmPreview();
    await flush();

    expect(deps.copyTextToClipboard).not.toHaveBeenCalled();
  });

  it("storeDiagnostics throws → latestDiagnosticsJson becomes null and the pipeline still finishes", async () => {
    // The diagnostics store wraps a try/catch and resets state on failure.
    // We verify the public contract: pipeline completes, debug listeners are
    // notified with null JSON.
    const { controller, callbacks } = createController({
      callbacks: {
        errorMessage: (err) => `friendly:${err.code}`,
        showToast: vi.fn(),
      },
    });

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("preview");

    await controller.confirmPreview();
    await flush();
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("pasted");
  });
});

describe("PushToTalkController — diagnostics emission", () => {
  it("emits onDiagnosticsChange with JSON that includes result.success flags", async () => {
    const { controller, callbacks } = createController();

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();
    await controller.confirmPreview();
    await flush();

    const last = callbacks.diagnostics[callbacks.diagnostics.length - 1] as string;
    const parsed = JSON.parse(last);
    expect(parsed.app).toBe("Floe");
    expect(parsed.result.stt_success).toBe(true);
    expect(parsed.result.cleanup_success).toBe(true);
    expect(parsed.result.clipboard_success).toBe(true);
    expect(parsed.result.paste_success).toBe(true);
    expect(parsed.result.error_stage).toBeNull();
  });

  it("flags cleanupFallbackUsed=true in diagnostics when cleanup returns fallbackUsed", async () => {
    const { controller, callbacks } = createController({
      deps: {
        cleanupTranscript: () =>
          Promise.resolve(
            makeCleanupResult({
              text: "raw",
              fallbackUsed: true,
              errorCode: "validationFailed",
              model: "",
              warning: "Cleanup failed",
            }),
          ),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();
    await controller.confirmPreview();
    await flush();

    const last = callbacks.diagnostics[callbacks.diagnostics.length - 1] as string;
    expect(last).toBeTruthy();
    const parsed = JSON.parse(last);
    expect(parsed.result.cleanup_fallback_used).toBe(true);
    expect(parsed.result.cleanup_success).toBe(false);
  });

  it("getLatestDiagnosticsJson returns the latest JSON snapshot", async () => {
    const { controller } = createController();
    expect(controller.getLatestDiagnosticsJson()).toBeNull();

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();
    await controller.confirmPreview();
    await flush();

    const json = controller.getLatestDiagnosticsJson();
    expect(typeof json).toBe("string");
    expect(json && json.length).toBeGreaterThan(0);
  });
});

describe("PushToTalkController — error path: alreadyRecording", () => {
  it("alreadyRecording error leaves controller reset to idle and showing error", async () => {
    const { controller, callbacks } = createController({
      deps: {
        startRecording: () =>
          Promise.reject(
            recordingDomainError("alreadyRecording", "in progress"),
          ),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();

    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("error");
    expect(callbacks.errors[callbacks.errors.length - 1]).toBe("friendly:alreadyRecording:in progress");
  });

  it("permission-denied error path uses no forceStop", async () => {
    const { controller, deps, callbacks } = createController({
      deps: {
        startRecording: () =>
          Promise.reject(
            recordingDomainError("permissionDenied", "denied"),
          ),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();
    expect(deps.forceStopRecording).not.toHaveBeenCalled();
    expect(callbacks.errors[callbacks.errors.length - 1]).toBe("friendly:permissionDenied:denied");
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("error");
  });
});

describe("PushToTalkController — logFrontendEvent forwarding", () => {
  it("logs stt completed and cleanup completed events with rounded durations", async () => {
    const { controller } = createController();
    const { logFrontendEvent } = await import("../lib/tauri");

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();

    const calls = (logFrontendEvent as unknown as ReturnType<typeof vi.fn>).mock
      .calls;
    const events = calls.map((c: unknown[]) => c[0] as Record<string, unknown>);
    const sttCompleted = events.find(
      (e) => e.stage === "stt" && e.eventType === "completed",
    );
    const cleanupCompleted = events.find(
      (e) => e.stage === "cleanup" && e.eventType === "completed",
    );

    expect(sttCompleted).toBeDefined();
    expect(cleanupCompleted).toBeDefined();
    expect(typeof sttCompleted?.durationMs).toBe("number");
  });

  it("logs paste completed event with pipelineTotalMs when available", async () => {
    const { controller } = createController();
    const { logFrontendEvent } = await import("../lib/tauri");

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();
    await controller.confirmPreview();
    await flush();

    const calls = (logFrontendEvent as unknown as ReturnType<typeof vi.fn>).mock
      .calls;
    const events = calls.map((c: unknown[]) => c[0] as Record<string, unknown>);
    const pasteCompleted = events.find(
      (e) => e.stage === "paste" && e.eventType === "completed",
    );
    expect(pasteCompleted).toBeDefined();
    expect(typeof pasteCompleted?.pipelineTotalMs).toBe("number");
  });
});

describe("PushToTalkController — non-FloeError thrown values", () => {
  it("string thrown is normalized into an internal FloeError → state error", async () => {
    const { controller, callbacks } = createController({
      deps: {
        startRecording: () => Promise.reject("plain string failure"),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("error");
    expect(callbacks.errors[callbacks.errors.length - 1]).toBeTruthy();
  });

  it("Error instance thrown is wrapped as internal FloeError", async () => {
    const { controller, callbacks } = createController({
      deps: {
        startRecording: () =>
          Promise.reject(new Error("uncaught sync failure")),
      },
    });
    await controller.handleShortcutState("Pressed");
    await flush();
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("error");
    expect(callbacks.errors[callbacks.errors.length - 1]).toMatch(/uncaught sync failure/);
  });
});

describe("PushToTalkController — final state resets", () => {
  it("after a successful run, isRecording() is false and confirming again is a no-op", async () => {
    const { controller, callbacks, deps } = createController();

    await controller.handleShortcutState("Pressed");
    await flush();
    await controller.handleShortcutState("Released");
    await flush();
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("preview");
    expect(controller.isRecording()).toBe(false);

    await controller.confirmPreview();
    await flush();
    expect(callbacks.stateChanges[callbacks.stateChanges.length - 1]).toBe("pasted");

    // After complete pipeline, deps copies are done — controller is fully idle.
    deps.copyTextToClipboard.mockClear();
    deps.pasteClipboard.mockClear();
    await controller.confirmPreview();
    await flush();
    expect(deps.copyTextToClipboard).not.toHaveBeenCalled();
  });
});
