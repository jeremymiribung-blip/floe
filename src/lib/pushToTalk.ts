import type {
  AppState,
  GroqTranscription,
  RecordingInfo,
  RecordingStatus,
  TranscriptCleanupResult,
} from "../types/app";

export type ShortcutState = "Pressed" | "Released";

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
  errorMessage: (caught: unknown) => string;
}

export class PushToTalkController {
  private hotkeyDown = false;
  private startInFlight = false;
  private releaseAfterStart = false;
  private recording = false;
  private finishing = false;

  constructor(
    private readonly dependencies: PushToTalkDependencies,
    private readonly callbacks: PushToTalkCallbacks,
  ) {}

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
      this.callbacks.onRecordingStatusChange(status);
      this.callbacks.onStateChange("recording");
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

  private async finishRecording(): Promise<void> {
    if (this.finishing) {
      return;
    }

    this.finishing = true;
    this.callbacks.onErrorChange(null);

    try {
      const latestRecording = await this.dependencies.stopRecording();
      this.recording = false;
      this.callbacks.onLatestRecordingChange(latestRecording);
      await this.refreshRecordingStatus();

      this.callbacks.onStateChange("transcribing");
      const transcription = await this.dependencies.transcribeLatestRecording();

      this.callbacks.onStateChange("cleaning");
      const cleanup = await this.cleanTranscriptOrUseRaw(transcription.text);
      const finalText = cleanup.text;
      this.callbacks.onErrorChange(cleanup.warning);
      this.callbacks.onTranscriptChange(finalText);

      if (finalText.trim().length === 0) {
        this.callbacks.onStateChange("idle");
        return;
      }

      this.callbacks.onStateChange("pasting");
      await this.dependencies.copyTextToClipboard(finalText);
      await this.dependencies.pasteClipboard();
      this.callbacks.onStateChange("pasted");
    } catch (caught) {
      this.callbacks.onErrorChange(this.callbacks.errorMessage(caught));
      this.callbacks.onStateChange("error");
    } finally {
      this.recording = false;
      this.finishing = false;
    }
  }

  private async cleanTranscriptOrUseRaw(
    transcript: string,
  ): Promise<TranscriptCleanupResult> {
    try {
      return await this.dependencies.cleanupTranscript(transcript);
    } catch {
      return {
        text: transcript,
        mode: "raw",
        warning: "Cleanup failed. Floe pasted the raw transcript instead.",
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
}
