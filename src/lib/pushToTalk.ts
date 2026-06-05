import type {
  AppState,
  GroqTranscription,
  GroqTranscriptionError,
  RecordingInfo,
  RecordingStatus,
  TranscriptCleanupResult,
} from "../types/app";
import {
  createPipelineDiagnostics,
  diagnosticsToJson,
  type PipelineStage,
} from "./diagnostics";

export type ShortcutState = "Pressed" | "Released";

const MAX_RECORDING_DURATION_MS = 120_000;
const WATCHDOG_GRACE_MS = 5_000;
const WATCHDOG_TIMEOUT_MS = MAX_RECORDING_DURATION_MS + WATCHDOG_GRACE_MS;

export interface PushToTalkDependencies {
  startRecording: () => Promise<RecordingStatus>;
  stopRecording: () => Promise<RecordingInfo>;
  getRecordingStatus: () => Promise<RecordingStatus>;
  transcribeLatestRecording: () => Promise<GroqTranscription>;
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
  errorMessage: (caught: unknown) => string;
}

export class PushToTalkController {
  private hotkeyDown = false;
  private startInFlight = false;
  private releaseAfterStart = false;
  private recording = false;
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

  async handleShortcutState(state: ShortcutState): Promise<void> {
    if (state === "Pressed") {
      await this.handlePressed();
      return;
    }

    await this.handleReleased();
  }

  private async handlePressed(): Promise<void> {
    if (this.hotkeyDown) {
      return;
    }

    this.hotkeyDown = true;
    if (this.startInFlight || this.recording || this.finishing) {
      return;
    }

    this.activeTraceStartedAt = this.nowMs();
    await this.startRecording();
  }

  private async handleReleased(): Promise<void> {
    this.hotkeyDown = false;

    if (this.startInFlight) {
      this.releaseAfterStart = true;
      return;
    }

    if (!this.recording || this.finishing) {
      return;
    }

    await this.finishRecording();
  }

  private async startRecording(): Promise<void> {
    this.startInFlight = true;
    this.releaseAfterStart = false;
    this.callbacks.onErrorChange(null);
    this.callbacks.onTranscriptChange(null);

    try {
      const status = await this.dependencies.startRecording();
      this.recording = true;
      this.hotkeyToRecordingStartMs = this.nowMs() - this.activeTraceStartedAt;
      this.callbacks.onRecordingStatusChange(status);
      this.callbacks.onStateChange("recording");
      this.scheduleWatchdog();
    } catch (caught) {
      this.recording = false;
      this.callbacks.onErrorChange(this.callbacks.errorMessage(caught));
      this.callbacks.onStateChange("error");
    } finally {
      this.startInFlight = false;
    }

    if (this.releaseAfterStart && this.recording && !this.finishing) {
      this.releaseAfterStart = false;
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
    if (!this.recording || this.finishing) {
      return;
    }

    this.recording = false;
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
    const totalStartedAt = this.activeTraceStartedAt || this.nowMs();
    let latestRecording: RecordingInfo | null = null;
    let transcription: GroqTranscription | null = null;
    let transcriptionError: Partial<GroqTranscriptionError> | null = null;
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
      this.recording = false;
      this.callbacks.onLatestRecordingChange(latestRecording);
      await this.refreshRecordingStatus();

      this.callbacks.onStateChange("transcribing");
      const sttStartedAt = this.nowMs();
      try {
        transcription = await this.dependencies.transcribeLatestRecording();
      } catch (caught) {
        sttDurationMs = this.nowMs() - sttStartedAt;
        transcriptionError = caught as Partial<GroqTranscriptionError>;
        errorStage = "stt";
        sanitizedErrorCode = sanitizedCode(caught);
        throw caught;
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
        sanitizedErrorCode = sanitizedCode(caught);
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
        throw caught;
      }
    } catch (caught) {
      if (errorStage === null) {
        errorStage = "recording";
        sanitizedErrorCode = sanitizedCode(caught);
      }
      this.callbacks.onErrorChange(this.callbacks.errorMessage(caught));
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
      this.recording = false;
      this.finishing = false;
    }
  }

  private async cleanTranscriptOrUseRaw(
    transcript: string,
  ): Promise<TranscriptCleanupResult> {
    try {
      return await this.dependencies.cleanupTranscript(transcript);
    } catch (caught) {
      return {
        text: transcript,
        warning: "Cleanup failed",
        model: "llama-3.1-8b-instant",
        retryCount: 0,
        validationMs: 0,
        fallbackUsed: true,
        errorCode: sanitizedCode(
          caught,
        ) as TranscriptCleanupResult["errorCode"],
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

function sanitizedCode(caught: unknown): string | null {
  const maybeCode = caught as Partial<{ code: string; errorCode: string }>;

  if (typeof maybeCode.code === "string") {
    return maybeCode.code;
  }
  if (typeof maybeCode.errorCode === "string") {
    return maybeCode.errorCode;
  }

  return null;
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
