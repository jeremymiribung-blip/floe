import type {
  AppState,
  RecordingState,
  SttResult,
  RecordingInfo,
  RecordingStatus,
  TranscriptCleanupResult,
  FloeError,
} from "../types/app";
import { parseFloeError, floeErrorCode } from "./errors";
import {
  createPipelineDiagnostics,
  diagnosticsToJson,
  type PipelineStage,
} from "./diagnostics";
import { diagLog } from "./tauri";
import { MAX_RECORDING_DURATION_SECS, WATCHDOG_GRACE_SECS } from "./contract";

export type ShortcutState = "Pressed" | "Released";

/**
 * Frontend safety watchdog timeout.
 * Must equal (MAX_RECORDING_DURATION_SECS + WATCHDOG_GRACE_SECS) * 1000.
 * This is the same formula as the backend's watchdog in RecordingManager.
 */
const WATCHDOG_TIMEOUT_MS =
  (MAX_RECORDING_DURATION_SECS + WATCHDOG_GRACE_SECS) * 1000;

export interface PushToTalkDependencies {
  startRecording: () => Promise<RecordingStatus>;
  stopRecording: () => Promise<RecordingInfo>;
  getRecordingStatus: () => Promise<RecordingStatus>;
  transcribeLatestRecording: () => Promise<SttResult>;
  cleanupTranscript: (transcript: string) => Promise<TranscriptCleanupResult>;
  copyTextToClipboard: (text: string) => Promise<void>;
  pasteClipboard: () => Promise<void>;
}

export interface PushToTalkCallbacks {
  onStateChange: (state: AppState) => void;
  onErrorChange: (message: string | null) => void;
  onRecordingStatusChange: (status: RecordingStatus) => void;
  onLatestRecordingChange: (recording: RecordingInfo) => void;
  onTranscriptChange: (transcript: string | null) => void;
  onDiagnosticsChange?: (json: string | null) => void;
  errorMessage: (error: FloeError) => string;
}

export class PushToTalkController {
  private recordingState: RecordingState = "idle";
  private releaseAfterStart = false;
  private finishing = false;
  private activeTraceStartedAt = 0;
  private hotkeyToRecordingStartMs = 0;
  private latestDiagnosticsJson: string | null = null;
  private watchdogTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private readonly dependencies: PushToTalkDependencies,
    private readonly callbacks: PushToTalkCallbacks,
    private readonly nowMs: () => number = defaultNowMs,
    private readonly createdAt: () => Date = () => new Date(),
    private readonly platform: string = defaultPlatform(),
  ) {}

  getLatestDiagnosticsJson(): string | null {
    return this.latestDiagnosticsJson;
  }

  syncRecordingState(state: RecordingState): void {
    this.recordingState = state;
    if (state === "idle" || state === "stopping") {
      this.finishing = false;
      this.clearWatchdog();
    }
    if (state === "idle" && !this.finishing) {
      this.releaseAfterStart = false;
    }
  }

  async handleShortcutState(state: ShortcutState): Promise<void> {
    diagLog(
      `[FE] handleShortcutState: ${state} recordingState=${this.recordingState} finishing=${this.finishing} releaseAfterStart=${this.releaseAfterStart}`,
    );
    if (state === "Pressed") {
      await this.handlePressed();
      return;
    }

    await this.handleReleased();
  }

  private async handlePressed(): Promise<void> {
    if (this.recordingState !== "idle") {
      return;
    }

    this.recordingState = "starting";
    this.callbacks.onStateChange("starting");
    this.activeTraceStartedAt = this.nowMs();
    await this.startRecording();
  }

  private async handleReleased(): Promise<void> {
    diagLog(
      `[FE] handleReleased: recordingState=${this.recordingState} finishing=${this.finishing}`,
    );

    if (this.recordingState === "starting") {
      diagLog(`[FE] handleReleased: setting releaseAfterStart=true`);
      this.releaseAfterStart = true;
      return;
    }

    if (this.recordingState !== "recording" || this.finishing) {
      diagLog(
        `[FE] handleReleased: early return - recordingState=${this.recordingState} finishing=${this.finishing}`,
      );
      return;
    }

    this.recordingState = "stopping";
    this.callbacks.onStateChange("stopping");
    diagLog(`[FE] handleReleased: calling finishRecording`);
    await this.finishRecording();
  }

  private async startRecording(): Promise<void> {
    this.releaseAfterStart = false;
    this.callbacks.onErrorChange(null);
    this.callbacks.onTranscriptChange(null);

    try {
      const status = await this.dependencies.startRecording();
      this.recordingState = "recording";
      this.hotkeyToRecordingStartMs = this.nowMs() - this.activeTraceStartedAt;
      this.callbacks.onRecordingStatusChange(status);
      this.callbacks.onStateChange("recording");
      this.scheduleWatchdog();
    } catch (caught) {
      this.recordingState = "idle";
      this.releaseAfterStart = false;
      this.callbacks.onErrorChange(
        this.callbacks.errorMessage(parseFloeError(caught)),
      );
      this.callbacks.onStateChange("error");
    }

    const shouldFinishAfterStart = this.releaseAfterStart;
    this.releaseAfterStart = false;
    diagLog(
      `[FE] startRecording done: shouldFinishAfterStart=${shouldFinishAfterStart} recordingState=${this.recordingState} finishing=${this.finishing}`,
    );

    if (
      shouldFinishAfterStart &&
      this.recordingState === "recording" &&
      !this.finishing
    ) {
      this.recordingState = "stopping";
      this.callbacks.onStateChange("stopping");
      await this.finishRecording();
    }
  }

  private scheduleWatchdog(): void {
    this.clearWatchdog();
    this.watchdogTimer = setTimeout(() => {
      this.watchdogTimer = null;
      void this.forceStopRecording();
    }, WATCHDOG_TIMEOUT_MS);
  }

  private clearWatchdog(): void {
    if (this.watchdogTimer !== null) {
      clearTimeout(this.watchdogTimer);
      this.watchdogTimer = null;
    }
  }

  private async forceStopRecording(): Promise<void> {
    this.clearWatchdog();
    if (this.recordingState !== "recording" || this.finishing) {
      return;
    }

    this.recordingState = "idle";
    try {
      await this.dependencies.stopRecording();
    } catch {
      // Backend may have already finalized via its own watchdog; ignore.
    }
    this.callbacks.onErrorChange("Recording failed");
    this.callbacks.onStateChange("error");
  }

  private async finishRecording(): Promise<void> {
    if (this.finishing) {
      return;
    }

    this.finishing = true;
    this.clearWatchdog();
    this.callbacks.onErrorChange(null);
    if (
      this.recordingState === "recording" ||
      this.recordingState === "stopping"
    ) {
      this.recordingState = "idle";
      this.callbacks.onStateChange("ready");
    }
    const totalStartedAt = this.activeTraceStartedAt || this.nowMs();
    let latestRecording: RecordingInfo | null = null;
    let transcription: SttResult | null = null;
    let transcriptionError: FloeError | null = null;
    let cleanup: TranscriptCleanupResult | null = null;
    let cleanupFallbackUsed = false;
    let cleanupValidationMs = 0;
    let sttDurationMs = 0;
    let cleanupDurationMs = 0;
    let clipboardWriteMs = 0;
    let pasteAttemptMs = 0;
    let clipboardSuccess = false;
    let pasteSuccess = false;
    let copiedOnly = false;
    let errorStage: PipelineStage | null = null;
    let sanitizedErrorCode: string | null = null;

    try {
      latestRecording = await this.dependencies.stopRecording();
      this.callbacks.onLatestRecordingChange(latestRecording);
      await this.refreshRecordingStatus();

      this.callbacks.onStateChange("transcribing");
      const sttStartedAt = this.nowMs();
      try {
        transcription = await this.dependencies.transcribeLatestRecording();
      } catch (caught) {
        sttDurationMs = this.nowMs() - sttStartedAt;
        transcriptionError = parseFloeError(caught);
        errorStage = "stt";
        sanitizedErrorCode = floeErrorCode(transcriptionError);
        throw transcriptionError;
      }
      sttDurationMs = this.nowMs() - sttStartedAt;

      this.callbacks.onStateChange("cleaning");
      const cleanupStartedAt = this.nowMs();
      cleanup = await this.cleanTranscriptOrUseRaw(transcription.text);
      cleanupDurationMs = this.nowMs() - cleanupStartedAt;
      cleanupFallbackUsed =
        cleanup.fallbackUsed === true || cleanup.warning === "Cleanup failed";
      cleanupValidationMs = cleanup.validationMs ?? 0;
      if (cleanupFallbackUsed && cleanup.errorCode) {
        errorStage =
          cleanup.errorCode === "validationFailed"
            ? "cleanup_validation"
            : "cleanup";
        sanitizedErrorCode = cleanup.errorCode;
      }
      const finalText = cleanup.text;
      this.callbacks.onErrorChange(cleanup.warning ?? null);
      this.callbacks.onTranscriptChange(finalText);

      if (finalText.trim().length === 0) {
        this.callbacks.onStateChange("ready");
        return;
      }

      this.callbacks.onStateChange("pasting");
      let clipboardStartedAt = 0;
      let pasteStartedAt = 0;
      try {
        clipboardStartedAt = this.nowMs();
        await this.dependencies.copyTextToClipboard(finalText);
        clipboardWriteMs = this.nowMs() - clipboardStartedAt;
        clipboardSuccess = true;
        pasteStartedAt = this.nowMs();
        await this.dependencies.pasteClipboard();
        pasteAttemptMs = this.nowMs() - pasteStartedAt;
        pasteSuccess = true;
        this.callbacks.onStateChange("pasted");
      } catch (caught) {
        const error = parseFloeError(caught);
        sanitizedErrorCode = floeErrorCode(error);
        if (clipboardSuccess) {
          pasteAttemptMs = this.nowMs() - pasteStartedAt;
          errorStage = "paste";
          this.callbacks.onErrorChange(null);
          this.callbacks.onStateChange("copied");
          copiedOnly = true;
          return;
        }
        clipboardWriteMs = this.nowMs() - clipboardStartedAt;
        errorStage = "clipboard";
        throw error;
      }
    } catch (caught) {
      const error = parseFloeError(caught);
      if (errorStage === null) {
        errorStage = "recording";
        sanitizedErrorCode = floeErrorCode(error);
      }
      this.callbacks.onErrorChange(this.callbacks.errorMessage(error));
      this.callbacks.onStateChange("error");
    } finally {
      this.storeDiagnostics({
        createdAt: this.createdAt(),
        platform: this.platform,
        totalMs: this.nowMs() - totalStartedAt,
        hotkeyToRecordingStartMs: this.hotkeyToRecordingStartMs,
        recordingInfo: latestRecording,
        sttDurationMs,
        stt: transcription,
        sttError: transcriptionError,
        cleanupDurationMs,
        cleanup,
        cleanupFallbackUsed,
        cleanupErrorCode: cleanup?.errorCode ?? null,
        cleanupValidationMs,
        clipboardWriteMs,
        pasteAttemptMs,
        clipboardSuccess,
        pasteSuccess,
        copiedOnly,
        errorStage,
        sanitizedErrorCode,
      });
      this.recordingState = "idle";
      this.finishing = false;
    }
  }

  private async cleanTranscriptOrUseRaw(
    transcript: string,
  ): Promise<TranscriptCleanupResult> {
    try {
      return await this.dependencies.cleanupTranscript(transcript);
    } catch (caught) {
      const error = parseFloeError(caught);
      return {
        text: transcript,
        warning: "Cleanup failed",
        model: "",
        retryCount: 0,
        validationMs: 0,
        fallbackUsed: true,
        errorCode: floeErrorCode(error),
      };
    }
  }

  private async refreshRecordingStatus(): Promise<void> {
    try {
      this.callbacks.onRecordingStatusChange(
        await this.dependencies.getRecordingStatus(),
      );
    } catch {
      // Recording already stopped; status refresh should not block transcription.
    }
  }

  private storeDiagnostics(
    input: Parameters<typeof createPipelineDiagnostics>[0],
  ): void {
    try {
      const diagnostics = createPipelineDiagnostics(input);
      this.latestDiagnosticsJson = diagnosticsToJson(diagnostics);
      this.callbacks.onDiagnosticsChange?.(this.latestDiagnosticsJson);
    } catch {
      // Diagnostics are best-effort and must never break the push-to-talk
      // pipeline. A safety-guard throw here means a contributor added a
      // forbidden key or pattern; the tests will catch it in CI.
      this.latestDiagnosticsJson = null;
      this.callbacks.onDiagnosticsChange?.(null);
    }
  }
}

function defaultNowMs(): number {
  return performance.now();
}

function defaultPlatform(): string {
  if (typeof navigator !== "undefined" && navigator.userAgent) {
    const userAgent = navigator.userAgent.toLowerCase();
    if (userAgent.includes("windows")) {
      return "windows";
    }
    if (userAgent.includes("mac")) {
      return "macos";
    }
    if (userAgent.includes("linux")) {
      return "linux";
    }
  }

  return "unknown";
}
