import { afterEach, describe, expect, it, vi } from "vitest";
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
  const diagnostics: Array<string | null> = [];
  let watchdogCallback: (() => void) | null = null;
  const watchdog = {
    fire: vi.fn(async () => {
      if (watchdogCallback) {
        const cb = watchdogCallback;
        watchdogCallback = null;
        await cb();
      }
    }),
  };
  vi.spyOn(globalThis, "setTimeout").mockImplementation(((
    handler: () => void,
  ) => {
    watchdogCallback = handler;
    return 1 as ReturnType<typeof setTimeout>;
  }) as typeof setTimeout);
  vi.spyOn(globalThis, "clearTimeout").mockImplementation(() => {
    watchdogCallback = null;
  });
  let now = 1_000;
  const nowMs = () => {
    now += 10;
    return now;
  };

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
        return transcription("raw transcript");
      }),
  );
  const cleanupTranscript = vi.fn(
    options.cleanupTranscript ??
      (async (transcript: string) => {
        calls.push("clean");
        return {
          text: `${transcript}.`,
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
      onDiagnosticsChange: (json) => diagnostics.push(json),
      errorMessage: errorMessage,
    },
    nowMs,
    () => new Date("2026-01-01T12:00:00.000Z"),
    "windows",
  );

  return {
    calls,
    controller,
    copyTextToClipboard,
    errors,
    diagnostics,
    getRecordingStatus,
    pasteClipboard,
    startRecording,
    stopRecording,
    states,
    transcripts,
    transcribeLatestRecording,
    watchdog,
  };
}

describe("PushToTalkController", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });
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
        return transcription("   ");
      },
      cleanupTranscript: (transcript) => {
        return Promise.resolve({
          text: transcript.trim(),
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

  it("surfaces cleanup warnings while still pasting cleaned text", async () => {
    const harness = createHarness({
      cleanupTranscript: async (transcript) => {
        harness.calls.push("clean");
        return {
          text: `${transcript}.`,
          warning: "Cleanup failed",
        };
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    expect(harness.copyTextToClipboard).toHaveBeenCalledWith("raw transcript.");
    expect(harness.pasteClipboard).toHaveBeenCalledTimes(1);
    expect(harness.errors).toContain("Cleanup failed");
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

  it("uses llama-3.1-8b-instant as the cleanup fallback model when cleanup throws", async () => {
    const harness = createHarness({
      cleanupTranscript: async () => {
        throw new Error("cleanup failed");
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    const json = harness.controller.getLatestDiagnosticsJson() ?? "";
    const diagnostics = JSON.parse(json);
    expect(diagnostics.models.cleanup).toBe("llama-3.1-8b-instant");
    expect(diagnostics.models.cleanup).not.toContain("gpt-oss");
    expect(diagnostics.result.cleanup_fallback_used).toBe(true);
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

    resolveTranscription(transcription("raw transcript"));
    await firstRelease;

    expect(lastState(harness.states)).toBe("pasted");
  });

  it("emits sanitized diagnostics for a successful pipeline", async () => {
    const harness = createHarness();

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    const diagnostics = latestDiagnostics(harness.diagnostics);
    expect(diagnostics.app).toBe("Floe");
    expect(diagnostics.trace_version).toBe(1);
    expect(diagnostics.created_at).toBe("2026-01-01T12:00:00.000Z");
    expect(diagnostics.platform).toBe("windows");
    expect(diagnostics.pipeline.total_ms).toBeGreaterThan(0);
    expect(diagnostics.pipeline.hotkey_to_recording_start_ms).toBeGreaterThan(
      0,
    );
    expect(diagnostics.pipeline.audio_encode_ms).toBe(4);
    expect(diagnostics.models.stt).toBe("whisper-large-v3-turbo");
    expect(diagnostics.models.cleanup).toBe("llama-3.1-8b-instant");
    expect(diagnostics.audio).toEqual({
      format: "wav",
      sample_rate: 16_000,
      channels: 1,
      bytes: 96_044,
    });
    expect(diagnostics.result.stt_success).toBe(true);
    expect(diagnostics.result.cleanup_success).toBe(true);
    expect(diagnostics.result.clipboard_success).toBe(true);
    expect(diagnostics.result.paste_success).toBe(true);
    expect(diagnostics.bottleneck.stage).toBeTruthy();
  });

  it("does not put transcript, cleaned text, keys, auth headers, audio, or clipboard content in diagnostics", async () => {
    const privateTranscript = "private raw transcript gsk_secret";
    const cleanedText = "private cleaned text authorization bearer";
    const harness = createHarness({
      transcribeLatestRecording: async () => transcription(privateTranscript),
      cleanupTranscript: async () => ({
        text: cleanedText,
        model: "llama-3.1-8b-instant",
        retryCount: 0,
        validationMs: 1,
        fallbackUsed: false,
      }),
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    const json = harness.controller.getLatestDiagnosticsJson() ?? "";
    expect(json).not.toContain(privateTranscript);
    expect(json).not.toContain(cleanedText);
    expect(json).not.toContain("gsk_secret");
    expect(json.toLowerCase()).not.toContain("authorization");
    expect(json.toLowerCase()).not.toContain("bearer");
    expect(json).not.toContain("rawAudio");
    expect(json).not.toContain("clipboard contents");
  });

  it("tracks failed STT with a sanitized stage and retry count", async () => {
    const error = new Error("Transcription failed") as Error & {
      code?: string;
      model?: string;
      retryCount?: number;
    };
    error.code = "timeout";
    error.model = "whisper-large-v3-turbo";
    error.retryCount = 2;
    const harness = createHarness({
      transcribeLatestRecording: async () => {
        throw error;
      },
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    const diagnostics = latestDiagnostics(harness.diagnostics);
    expect(diagnostics.result.stt_success).toBe(false);
    expect(diagnostics.result.error_stage).toBe("stt");
    expect(diagnostics.result.sanitized_error_code).toBe("timeout");
    expect(diagnostics.retries.stt).toBe(2);
    expect(diagnostics.result.clipboard_success).toBe(false);
  });

  it("tracks cleanup fallback and validation failure without stopping paste", async () => {
    const harness = createHarness({
      cleanupTranscript: async (transcript) => ({
        text: transcript,
        warning: "Cleanup failed",
        model: "llama-3.1-8b-instant",
        retryCount: 0,
        validationMs: 2,
        fallbackUsed: true,
        errorCode: "validationFailed",
      }),
    });

    await harness.controller.handleShortcutState("Pressed");
    await harness.controller.handleShortcutState("Released");

    const diagnostics = latestDiagnostics(harness.diagnostics);
    expect(diagnostics.result.cleanup_success).toBe(false);
    expect(diagnostics.result.cleanup_fallback_used).toBe(true);
    expect(diagnostics.result.error_stage).toBe("cleanup_validation");
    expect(diagnostics.result.sanitized_error_code).toBe("validationFailed");
    expect(diagnostics.pipeline.cleanup_validation_ms).toBe(2);
    expect(lastState(harness.states)).toBe("pasted");
  });

  describe("stuck recording guard", () => {
    it("transitions to error and calls stopRecording when the watchdog fires", async () => {
      let resolveStart: (status: RecordingStatus) => void = () => undefined;
      const startPromise = new Promise<RecordingStatus>((resolve) => {
        resolveStart = resolve;
      });
      const harness = createHarness({
        startRecording: () => startPromise,
      });

      const press = harness.controller.handleShortcutState("Pressed");
      resolveStart(recordingStatus);
      await press;

      expect(harness.watchdog.fire).not.toHaveBeenCalled();

      await harness.watchdog.fire();

      expect(harness.stopRecording).toHaveBeenCalledTimes(1);
      expect(harness.errors).toContain("Recording failed");
      expect(lastState(harness.states)).toBe("error");
    });

    it("does not fire the watchdog when recording stops normally", async () => {
      const harness = createHarness();

      await harness.controller.handleShortcutState("Pressed");
      await harness.controller.handleShortcutState("Released");

      await harness.watchdog.fire();

      expect(harness.stopRecording).toHaveBeenCalledTimes(1);
      expect(harness.transcribeLatestRecording).toHaveBeenCalledTimes(1);
      expect(lastState(harness.states)).toBe("pasted");
    });

    it("treats backend watchdogTimeout as a forced error", async () => {
      const watchdogError = new Error("Recording failed") as Error & {
        code?: string;
      };
      watchdogError.code = "watchdogTimeout";
      const harness = createHarness({
        stopRecording: async () => {
          harness.calls.push("stop");
          throw watchdogError;
        },
      });

      await harness.controller.handleShortcutState("Pressed");
      await harness.controller.handleShortcutState("Released");

      expect(harness.errors).toContain("Recording failed");
      expect(lastState(harness.states)).toBe("error");
    });

    it("treats backend stopFailed as a forced error", async () => {
      const stopError = new Error("Recording failed") as Error & {
        code?: string;
      };
      stopError.code = "stopFailed";
      const harness = createHarness({
        stopRecording: async () => {
          harness.calls.push("stop");
          throw stopError;
        },
      });

      await harness.controller.handleShortcutState("Pressed");
      await harness.controller.handleShortcutState("Released");

      expect(harness.errors).toContain("Recording failed");
      expect(lastState(harness.states)).toBe("error");
    });
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

function transcription(text: string): GroqTranscription {
  return {
    text,
    model: "whisper-large-v3-turbo",
    retryCount: 0,
  };
}

function latestDiagnostics(entries: Array<string | null>) {
  const json = entries[entries.length - 1];
  if (!json) {
    throw new Error("Diagnostics were not emitted");
  }

  return JSON.parse(json);
}
