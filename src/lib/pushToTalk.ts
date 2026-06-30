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
import { diagLog, logFrontendEvent, updateSessionHotkeyLatency } from "./tauri";
import { logRecoverable } from "./errorLog";
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
  forceStopRecording: () => Promise<void>;
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
  /** Show a brief toast/notification to the user */
  showToast?: (message: string) => void;
}

export class PushToTalkController {
  private recordingState: RecordingState = "idle";
  private releaseAfterStart = false;
  private finishing = false;
  private previewMode = false;
  private pendingPreviewText: string | null = null;
  /** Snapshot of pipeline data saved when entering preview, needed for confirmPreview diagnostics. */
  private previewPipelineData: {
    latestRecording: RecordingInfo | null;
    transcription: SttResult | null;
    transcriptionError: FloeError | null;
    cleanup: TranscriptCleanupResult | null;
    cleanupFallbackUsed: boolean;
    cleanupValidationMs: number;
    sttDurationMs: number;
    cleanupDurationMs: number;
  } | null = null;
  private activeTraceId: string | null = null;
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
    private readonly appVersion: string = "0.0.0",
  ) {}

  getLatestDiagnosticsJson(): string | null {
    return this.latestDiagnosticsJson;
  }

  /** Returns true when a recording is active or starting (for detecting unexpected backend transitions). */
  isRecording(): boolean {
    return (
      this.recordingState === "recording" || this.recordingState === "starting"
    );
  }

  syncRecordingState(state: RecordingState): void {
    this.recordingState = state;
    // Don't reset finishing while preview is active — the user must
    // confirm or discard before the pipeline is fully complete.
    if (state === "idle" || state === "stopping") {
      if (!this.previewMode) {
        this.finishing = false;
      }
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
    // In preview mode, ignore hotkey — user must use Enter or Escape
    if (this.previewMode) {
      return;
    }
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
      this.activeTraceId = status.traceId ?? null;
      this.hotkeyToRecordingStartMs = this.nowMs() - this.activeTraceStartedAt;
      this.callbacks.onRecordingStatusChange(status);
      this.callbacks.onStateChange("recording");
      this.scheduleWatchdog();

      // Send hotkey-to-recording-start latency to backend for diagnostics
      if (status.traceId) {
        void updateSessionHotkeyLatency(
          status.traceId,
          this.hotkeyToRecordingStartMs,
        ).catch((err) => {
          logRecoverable("updateSessionHotkeyLatency", err);
        });
      }
    } catch (caught) {
      const error = parseFloeError(caught);
      this.recordingState = "idle";
      this.releaseAfterStart = false;
      
      // Check for Internal error - indicates mutex poison or stuck state
      if (error.code === "internal") {
        try {
          await this.dependencies.forceStopRecording();
        } catch (err) {
          logRecoverable("forceStopRecording after internal", err);
        }
        this.callbacks.onErrorChange("Hardware error: Recording reset");
        this.callbacks.onStateChange("idle");
        this.callbacks.showToast?.("Recording reset due to hardware error. Try again.");
      } else {
        this.callbacks.onErrorChange(this.callbacks.errorMessage(error));
        this.callbacks.onStateChange("error");
      }
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
      await this.dependencies.forceStopRecording();
    } catch (err) {
      logRecoverable("forceStopRecording watchdog", err);
    }
    this.callbacks.onErrorChange("Hardware error: Recording reset");
    this.callbacks.onStateChange("idle");
    this.callbacks.showToast?.("Recording reset due to hardware error. Try again.");
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
      // Do NOT set "ready" here — wait for transcription to start to avoid
      // a race where an error between "ready" and "transcribing" is swallowed.
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

      this.pushEvent("stt", "started", 0);

      // Transition to transcribing — this is the first state after recording stops.
      // Any error from here on will be explicitly caught and surfaced.
      this.callbacks.onStateChange("transcribing");
      const sttStartedAt = this.nowMs();
      try {
        transcription = await this.dependencies.transcribeLatestRecording();
        sttDurationMs = this.nowMs() - sttStartedAt;
        this.pushEvent("stt", "completed", sttDurationMs);
      } catch (caught) {
        sttDurationMs = this.nowMs() - sttStartedAt;
        transcriptionError = parseFloeError(caught);
        errorStage = "stt";
        sanitizedErrorCode = floeErrorCode(transcriptionError);
        this.pushEvent("stt", "failed", sttDurationMs, sanitizedErrorCode);
        // Explicitly surface the error via callbacks so it's not silently swallowed
        this.callbacks.onErrorChange(this.callbacks.errorMessage(transcriptionError));
        this.callbacks.onStateChange("error");
        throw transcriptionError;
      }

      this.pushEvent("cleanup", "started", 0);

      this.callbacks.onStateChange("cleaning");
      const cleanupStartedAt = this.nowMs();
      cleanup = await this.cleanTranscriptOrUseRaw(transcription.text);
      cleanupDurationMs = this.nowMs() - cleanupStartedAt;

      this.pushEvent(
        "cleanup",
        cleanupFallbackUsed ? "fallback" : "completed",
        cleanupDurationMs,
        cleanup?.errorCode ?? null,
      );
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

      // ── Preview & Confirm ───────────────────────────────────
      // Instead of immediately pasting, pause to let the user review
      // the transcript. Store pipeline state for later completion.
      this.previewMode = true;
      this.pendingPreviewText = finalText;
      this.previewPipelineData = {
        latestRecording,
        transcription,
        transcriptionError,
        cleanup,
        cleanupFallbackUsed,
        cleanupValidationMs,
        sttDurationMs,
        cleanupDurationMs,
      };

      this.callbacks.onTranscriptChange(finalText);
      this.callbacks.onStateChange("preview");
      // Return early — confirmPreview or discardPreview will complete
      // the pipeline. The finally block skips cleanup when previewMode is set.
      return;
    } catch (caught) {
      const error = parseFloeError(caught);
      if (errorStage === null) {
        errorStage = "recording";
        sanitizedErrorCode = floeErrorCode(error);
      }
      this.callbacks.onErrorChange(this.callbacks.errorMessage(error));
      this.callbacks.onStateChange("error");
    } finally {
      // When in preview mode, the pipeline is paused and will be
      // completed by confirmPreview or discardPreview. Don't reset yet.
      if (this.previewMode) return;

      if (errorStage !== null) {
        this.pushEvent(
          errorStage,
          "failed",
          this.nowMs() - this.activeTraceStartedAt,
          sanitizedErrorCode,
        );
      }
      this.storeDiagnostics({
        createdAt: this.createdAt(),
        platform: this.platform,
        appVersion: this.appVersion,
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

  /**
   * Confirm the previewed transcript: copy to clipboard and paste.
   * Called when the user presses Enter in the preview state.
   */
  async confirmPreview(): Promise<void> {
    if (!this.previewMode || !this.pendingPreviewText) {
      return;
    }

    const finalText = this.pendingPreviewText;
    const pipelineData = this.previewPipelineData;
    this.previewMode = false;
    this.pendingPreviewText = null;
    this.previewPipelineData = null;

    const totalStartedAt = this.activeTraceStartedAt || this.nowMs();
    let clipboardWriteMs = 0;
    let pasteAttemptMs = 0;
    let clipboardSuccess = false;
    let pasteSuccess = false;
    let copiedOnly = false;
    let errorStage: PipelineStage | null = null;
    let sanitizedErrorCode: string | null = null;

    this.callbacks.onStateChange("pasting");

    try {
      clipboardWriteMs = this.nowMs();
      await this.dependencies.copyTextToClipboard(finalText);
      clipboardWriteMs = this.nowMs() - clipboardWriteMs;
      clipboardSuccess = true;
      this.pushEvent("clipboard", "completed", clipboardWriteMs);

      pasteAttemptMs = this.nowMs();
      await this.dependencies.pasteClipboard();
      pasteAttemptMs = this.nowMs() - pasteAttemptMs;
      pasteSuccess = true;
      this.callbacks.onStateChange("pasted");
      this.pushEvent(
        "paste",
        "completed",
        pasteAttemptMs,
        null,
        this.nowMs() - this.activeTraceStartedAt,
      );
    } catch (caught) {
      const error = parseFloeError(caught);
      sanitizedErrorCode = floeErrorCode(error);
      if (clipboardSuccess) {
        pasteAttemptMs = this.nowMs() - pasteAttemptMs;
        errorStage = "paste";
        this.pushEvent("paste", "failed", pasteAttemptMs, sanitizedErrorCode);
        this.callbacks.onErrorChange(null);
        this.callbacks.onStateChange("copied");
        copiedOnly = true;
        this.callbacks.showToast?.("Copied to clipboard (automatic paste failed)");
        return;
      }
      clipboardWriteMs = this.nowMs() - clipboardWriteMs;
      errorStage = "clipboard";
      this.pushEvent(
        "clipboard",
        "failed",
        clipboardWriteMs,
        sanitizedErrorCode,
      );
      throw error;
    } finally {
      // Store diagnostics with the original pipeline data merged with paste results
      if (pipelineData) {
        this.storeDiagnostics({
          createdAt: this.createdAt(),
          platform: this.platform,
          appVersion: this.appVersion,
          totalMs: this.nowMs() - totalStartedAt,
          hotkeyToRecordingStartMs: this.hotkeyToRecordingStartMs,
          recordingInfo: pipelineData.latestRecording,
          sttDurationMs: pipelineData.sttDurationMs,
          stt: pipelineData.transcription,
          sttError: pipelineData.transcriptionError as Parameters<
            typeof this.storeDiagnostics
          >[0]["sttError"],
          cleanupDurationMs: pipelineData.cleanupDurationMs,
          cleanup: pipelineData.cleanup,
          cleanupFallbackUsed: pipelineData.cleanupFallbackUsed,
          cleanupErrorCode: pipelineData.cleanup?.errorCode ?? null,
          cleanupValidationMs: pipelineData.cleanupValidationMs,
          clipboardWriteMs,
          pasteAttemptMs,
          clipboardSuccess,
          pasteSuccess,
          copiedOnly,
          errorStage,
          sanitizedErrorCode,
        });
      }
      this.recordingState = "idle";
      this.finishing = false;
    }
  }

  /**
   * Discard the previewed transcript: reset to idle without pasting.
   * Called when the user presses Escape in the preview state.
   */
  async discardPreview(): Promise<void> {
    if (!this.previewMode) {
      return;
    }

    this.previewMode = false;
    this.pendingPreviewText = null;
    this.previewPipelineData = null;
    this.finishing = false;
    this.recordingState = "idle";
    this.callbacks.onStateChange("idle");
    this.callbacks.onTranscriptChange(null);
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
    } catch (err) {
      logRecoverable("refreshRecordingStatus", err);
    }
  }

  /** Push a structured lifecycle event to the backend's timed timeline. */
  private pushEvent(
    stage: string,
    eventType: string,
    durationMs: number,
    errorCode?: string | null,
    pipelineTotalMs?: number | null,
  ): void {
    if (!this.activeTraceId) return;
    void logFrontendEvent({
      traceId: this.activeTraceId,
      stage,
      eventType,
      durationMs: Math.round(durationMs),
      errorCode: errorCode ?? null,
      retryCount: null,
      pipelineTotalMs: pipelineTotalMs ?? null,
    }).catch((err) => {
      // Best-effort; never block the pipeline over diagnostics.
      logRecoverable("logFrontendEvent", err);
    });
  }

  private storeDiagnostics(
    input: Parameters<typeof createPipelineDiagnostics>[0],
  ): void {
    try {
      const diagnostics = createPipelineDiagnostics(input);
      this.latestDiagnosticsJson = diagnosticsToJson(diagnostics);
      this.callbacks.onDiagnosticsChange?.(this.latestDiagnosticsJson);
    } catch (err) {
      // Diagnostics are best-effort and must never break the push-to-talk
      // pipeline. A safety-guard throw here means a contributor added a
      // forbidden key or pattern; the tests will catch it in CI.
      logRecoverable("storeDiagnostics", err);
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
