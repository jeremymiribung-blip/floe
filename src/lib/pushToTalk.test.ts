import { describe, expect, it, vi } from "vitest";
import { PushToTalkController } from "./pushToTalk";
import type {
  AppState,
  GroqTranscription,
  RecordingInfo,
  RecordingStatus,
  TranscriptCleanupResult,
} from "../types/app";

const latestRecording: RecordingInfo = {
  sampleRate: 48_000,
  inputChannels: 1,
  outputChannels: 1,
  durationMs: 1_000,
  sampleCount: 48_000,
  wavByteCount: 96_044,
  wavBitsPerSample: 16,
  startedAtMs: 1_000,
  endedAtMs: 2_000,
  maxDurationReached: false,
  endedReason: "manual",
};

const idleStatus: RecordingStatus = {
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

const recordingStatus: RecordingStatus = {
  ...idleStatus,
  isRecording: true,
  sampleRate: 48_000,
  inputChannels: 1,
  durationMs: 100,
  sampleCount: 4_800,
  startedAtMs: 1_000,
  latestRecording: null,
};

interface HarnessOptions {
  startRecording?: () => Promise<RecordingStatus>;
  stopRecording?: () => Promise<RecordingInfo>;
  getRecordingStatus?: () => Promise<RecordingStatus>;
  transcribeLatestRecording?: () => Promise<GroqTranscription>;
  cleanupTranscript?: (transcript: string) => Promise<TranscriptCleanupResult>;
  copyTextToClipboard?: (text: string) => Promise<void>;
  pasteClipboard?: () => Promise<void>;
}

function createHarness(options: HarnessOptions = {}) {
  const calls: string[] = [];
  const states: AppState[] = [];
  const errors: Array<string | null> = [];
  const transcripts: Array<string | null> = [];

  const startRecording = vi.fn(
    options.startRecording ??
      (async () => {
        calls.push("start");
        return recordingStatus;
      }),
  );
  const stopRecording = vi.fn(
    options.stopRecording ??
      (async () => {
        calls.push("stop");
        return latestRecording;
      }),
  );
  const getRecordingStatus = vi.fn(
    options.getRecordingStatus ??
      (async () => {
        calls.push("status");
        return idleStatus;
      }),
  );
  const transcribeLatestRecording = vi.fn(
    options.transcribeLatestRecording ??
      (async () => {
        calls.push("transcribe");
        return { text: "raw transcript" };
      }),
  );
  const cleanupTranscript = vi.fn(
    options.cleanupTranscript ??
      (async (transcript: string) => {
        calls.push("clean");
        return {
          text: `${transcript}.`,
          mode: "fast" as const,
          warning: null,
        };
      }),
  );
  const copyTextToClipboard = vi.fn(
    options.copyTextToClipboard ??
      (async (text: string) => {
        calls.push(`copy:${text}`);
      }),
  );
  const pasteClipboard = vi.fn(
    options.pasteClipboard ??
      (async () => {
        calls.push("paste");
      }),
  );

  const controller = new PushToTalkController(
    {
      startRecording,
      stopRecording,
      getRecordingStatus,
      transcribeLatestRecording,
      cleanupTranscript,
      copyTextToClipboard,
      pasteClipboard,
    },
    {
      onStateChange: (state) => states.push(state),
      onErrorChange: (message) => errors.push(message),
      onRecordingStatusChange: () => undefined,
      onLatestRecordingChange: () => undefined,
      onTranscriptChange: (transcript) => transcripts.push(transcript),
      errorMessage: errorMessage,
    },
  );

  return {
    calls,
    controller,
    copyTextToClipboard,
    errors,
    getRecordingStatus,
    pasteClipboard,
    startRecording,
    stopRecording,
    states,
    transcripts,
    transcribeLatestRecording,
  };
}

describe("PushToTalkController", () => {
  it("starts once for repeated pressed events", async () => {
    let resolveStart: (status: RecordingStatus) => void = () => undefined;
    const startPromise = new Promise<RecordingStatus>((resolve) => {
      resolveStart = resolve;
    });
    const harness = createHarness({
      startRecording: () => startPromise,
    });

    const firstPress = harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Pressed");
    resolveStart(recordingStatus);
    await firstPress;

    expect(harness.startRecording).toHaveBeenCalledTimes(1);
    expect(harness.states).toContain("recording");
  });

  it("ignores release when no recording is active", async () => {
    const harness = createHarness();

    await harness.controller.handleShortcutState("Released");

    expect(harness.startRecording).not.toHaveBeenCalled();
    expect(harness.stopRecording).not.toHaveBeenCalled();
    expect(harness.transcribeLatestRecording).not.toHaveBeenCalled();
    expect(harness.states).toEqual([]);
  });

  it("stops, transcribes, cleans, copies, and pastes on release", async () => {
    const harness = createHarness();

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    expect(harness.calls).toEqual([
      "start",
      "stop",
      "status",
      "transcribe",
      "clean",
      "copy:raw transcript.",
      "paste",
    ]);
    expect(harness.states).toEqual([
      "recording",
      "transcribing",
      "cleaning",
      "pasting",
      "pasted",
    ]);
  });

  it("finishes when release arrives before start resolves", async () => {
    let resolveStart: (status: RecordingStatus) => void = () => undefined;
    const startPromise = new Promise<RecordingStatus>((resolve) => {
      resolveStart = resolve;
    });
    const harness = createHarness({
      startRecording: async () => {
        harness.calls.push("start");
        return startPromise;
      },
    });

    const press = harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");
    resolveStart(recordingStatus);
    await press;

    expect(harness.calls).toEqual([
      "start",
      "stop",
      "status",
      "transcribe",
      "clean",
      "copy:raw transcript.",
      "paste",
    ]);
    expect(lastState(harness.states)).toBe("pasted");
  });

  it("reports start failures without stopping or transcribing", async () => {
    const harness = createHarness({
      startRecording: async () => {
        throw new Error("start failed");
      },
    });

    await harness.controller.handleShortcutState("Pressed");

    expect(harness.stopRecording).not.toHaveBeenCalled();
    expect(harness.transcribeLatestRecording).not.toHaveBeenCalled();
    expect(harness.errors).toContain("start failed");
    expect(lastState(harness.states)).toBe("error");
  });

  it("continues transcription when stopped status refresh fails", async () => {
    const harness = createHarness({
      getRecordingStatus: async () => {
        throw new Error("status failed");
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    expect(harness.getRecordingStatus).toHaveBeenCalledTimes(1);
    expect(harness.calls).toEqual([
      "start",
      "stop",
      "transcribe",
      "clean",
      "copy:raw transcript.",
      "paste",
    ]);
    expect(lastState(harness.states)).toBe("pasted");
  });

  it("skips clipboard work for an empty cleaned transcript", async () => {
    const harness = createHarness({
      transcribeLatestRecording: async () => {
        harness.calls.push("transcribe");
        return { text: "   " };
      },
      cleanupTranscript: (transcript) => {
        return Promise.resolve({
          text: transcript.trim(),
          mode: "fast",
          warning: null,
        });
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    expect(harness.copyTextToClipboard).not.toHaveBeenCalled();
    expect(harness.pasteClipboard).not.toHaveBeenCalled();
    expect(harness.transcripts).toEqual([null, ""]);
    expect(lastState(harness.states)).toBe("ready");
  });

  it("surfaces cleanup warnings while still pasting fallback text", async () => {
    const harness = createHarness({
      cleanupTranscript: async (transcript) => {
        harness.calls.push("clean");
        return {
          text: `${transcript}.`,
          mode: "fast",
          warning: "Cerebras failed. Floe used Fast cleanup instead.",
        };
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    expect(harness.copyTextToClipboard).toHaveBeenCalledWith("raw transcript.");
    expect(harness.pasteClipboard).toHaveBeenCalledTimes(1);
    expect(harness.errors).toContain(
      "Cerebras failed. Floe used Fast cleanup instead.",
    );
    expect(lastState(harness.states)).toBe("pasted");
  });

  it("pastes nothing when transcription fails", async () => {
    const harness = createHarness({
      transcribeLatestRecording: async () => {
        throw new Error("transcription failed");
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    expect(harness.copyTextToClipboard).not.toHaveBeenCalled();
    expect(harness.pasteClipboard).not.toHaveBeenCalled();
    expect(harness.errors).toContain("transcription failed");
    expect(lastState(harness.states)).toBe("error");
  });

  it("pastes the raw transcript when cleanup fails", async () => {
    const harness = createHarness({
      cleanupTranscript: async () => {
        throw new Error("cleanup failed");
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    expect(harness.calls).toContain("copy:raw transcript");
    expect(harness.calls).toContain("paste");
    expect(lastState(harness.states)).toBe("pasted");
  });

  it("copies text before a paste failure is reported", async () => {
    const harness = createHarness({
      pasteClipboard: async () => {
        throw new Error("paste failed");
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    expect(harness.calls).toEqual([
      "start",
      "stop",
      "status",
      "transcribe",
      "clean",
      "copy:raw transcript.",
    ]);
    expect(harness.copyTextToClipboard).toHaveBeenCalledWith("raw transcript.");
    expect(harness.errors).toContain("paste failed");
    expect(lastState(harness.states)).toBe("error");
  });

  it("transitions to copied when paste automation is blocked", async () => {
    const harness = createHarness({
      pasteClipboard: async () => {
        const error = new Error(
          "Transcript copied to clipboard, but Floe could not send the paste shortcut. Paste manually with Command+V or Control+V.",
        ) as Error & { code?: string };
        error.code = "pasteUnavailable";
        throw error;
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    expect(harness.calls).toEqual([
      "start",
      "stop",
      "status",
      "transcribe",
      "clean",
      "copy:raw transcript.",
    ]);
    expect(harness.copyTextToClipboard).toHaveBeenCalledWith("raw transcript.");
    expect(harness.errors).toContain(null);
    expect(lastState(harness.states)).toBe("copied");
  });

  it("prevents concurrent transcriptions", async () => {
    let resolveTranscription: (transcription: GroqTranscription) => void = () =>
      undefined;
    const transcriptionPromise = new Promise<GroqTranscription>((resolve) => {
      resolveTranscription = resolve;
    });
    const harness = createHarness({
      transcribeLatestRecording: () => transcriptionPromise,
    });

    await harness.controller.handleShortcutState("Pressed");
    const firstRelease = harness.controller.handleShortcutState("Released");
    await Promise.resolve();
    await Promise.resolve();
    await harness.controller.handleShortcutState("Released");

    expect(harness.transcribeLatestRecording).toHaveBeenCalledTimes(1);

    resolveTranscription({ text: "raw transcript" });
    await firstRelease;

    expect(lastState(harness.states)).toBe("pasted");
  });
});

function errorMessage(caught: unknown): string {
  if (caught instanceof Error) {
    return caught.message;
  }

  const maybeError = caught as Partial<{ message: string }>;

  return typeof maybeError.message === "string" ? maybeError.message : "failed";
}

function lastState(states: AppState[]): AppState | undefined {
  return states[states.length - 1];
}
