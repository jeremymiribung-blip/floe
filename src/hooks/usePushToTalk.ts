import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  AppState,
  RecordingInfo,
  RecordingStatePayload,
  RecordingStatus,
} from "../types/app";
import { PushToTalkController } from "../lib/pushToTalk";
import { clipboardErrorMessage } from "../lib/clipboardErrors";
import { recordingErrorMessage } from "../lib/recordingErrors";
import {
  bubbleHide,
  bubbleShow,
  cleanupTranscript,
  copyTextToClipboard,
  diagLog,
  forceStopRecording,
  getRecordingStatus,
  isTauriRuntime,
  pasteClipboard,
  startRecording,
  stopRecording,
  transcribeLatestRecording,
} from "../lib/tauri";
import {
  EVENT_HOTKEY_STATE,
  EVENT_RECORDING_STATE_CHANGED,
} from "../lib/contract";
import useFloeStore from "../stores/useFloeStore";

function pushToTalkErrorMessage(caught: unknown): string {
  const maybeClipboardError = caught as { domain?: string; code?: string };
  if (
    maybeClipboardError.domain === "clipboard" ||
    maybeClipboardError.code === "clipboardUnavailable" ||
    maybeClipboardError.code === "pasteUnavailable"
  ) {
    return clipboardErrorMessage(caught);
  }

  const maybeSttError = caught as { domain?: string; code?: string };
  if (
    maybeSttError.domain === "stt" ||
    typeof maybeSttError.code === "string"
  ) {
    const sttError = caught as { message?: string };
    return typeof sttError.message === "string"
      ? sttError.message
      : "Transcription failed";
  }

  return recordingErrorMessage(caught);
}

export interface UsePushToTalkResult {
  appState: AppState;
  error: string | null;
  latestTranscript: string | null;
  latestDiagnosticsJson: string | null;
  recordingStatus: RecordingStatus | null;
  latestRecording: RecordingInfo | null;
  triggerStart: () => void;
  triggerStop: () => void;
  confirmPreview: () => void;
  discardPreview: () => void;
}

export function usePushToTalk(): UsePushToTalkResult {
  const [appState, setAppState] = useState<AppState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [latestTranscript, setLatestTranscript] = useState<string | null>(null);
  const [latestDiagnosticsJson, setLatestDiagnosticsJson] = useState<
    string | null
  >(null);
  const [recordingStatus, setRecordingStatus] =
    useState<RecordingStatus | null>(null);
  const [latestRecording, setLatestRecording] = useState<RecordingInfo | null>(
    null,
  );

  const controllerRef = useRef<PushToTalkController | null>(null);
  const syncFromPipeline = useFloeStore((s) => s.syncFromPipeline);
  const setRecordingStartedAt = useFloeStore((s) => s.setRecordingStartedAt);
  const skipCleanupRef = useRef(useFloeStore.getState().skipCleanup);
  skipCleanupRef.current = useFloeStore.getState().skipCleanup;

  if (controllerRef.current === null) {
    controllerRef.current = new PushToTalkController(
      {
        startRecording,
        stopRecording,
        forceStopRecording,
        getRecordingStatus,
        transcribeLatestRecording,
        cleanupTranscript: (transcript: string) =>
          cleanupTranscript(transcript, skipCleanupRef.current),
        copyTextToClipboard,
        pasteClipboard,
      },
      {
        onStateChange: (state: AppState) => {
          setAppState(state);
          syncFromPipeline(state);
          if (state === "recording") {
            setRecordingStartedAt(Date.now());
          }
          if (state === "preview") {
            // Show and focus the main window so the preview overlay is visible
            // and can accept keyboard events
            if (isTauriRuntime()) {
              getCurrentWindow()
                .show()
                .catch(() => {});
              getCurrentWindow()
                .setFocus()
                .catch(() => {});
            }
          }
        },
        onErrorChange: setError,
        onRecordingStatusChange: setRecordingStatus,
        onLatestRecordingChange: setLatestRecording,
        onTranscriptChange: setLatestTranscript,
        onDiagnosticsChange: setLatestDiagnosticsJson,
        errorMessage: pushToTalkErrorMessage,
      },
    );
  }

  const handleHotkeyEvent = useCallback(
    async (state: "Pressed" | "Released") => {
      diagLog(`[FE] handleHotkeyEvent: state=${state}`);
      if (state === "Pressed") {
        void bubbleShow();
      } else {
        void bubbleHide();
      }

      try {
        await controllerRef.current?.handleShortcutState(state);
      } catch {
        setAppState("error");
        setError("Recording failed");
        void bubbleHide();
      }
    },
    [],
  );

  // Listen for global hotkey events from the backend
  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;

    listen<{ state: "Pressed" | "Released" }>(EVENT_HOTKEY_STATE, (event) => {
      diagLog(`[FE] listen callback: state=${event.payload.state}`);
      void handleHotkeyEvent(event.payload.state);
    })
      .then((nextUnlisten) => {
        if (isActive) {
          unlisten = nextUnlisten;
        } else {
          nextUnlisten();
        }
      })
      .catch(() => {
        if (isActive) {
          setError("Floe could not listen for the global hotkey.");
          void bubbleHide();
        }
      });

    return () => {
      isActive = false;
      unlisten?.();
    };
  }, [handleHotkeyEvent]);

  // Listen for backend recording state transitions (e.g. unexpected idle from device disconnect or watchdog)
  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;

    listen<RecordingStatePayload>(EVENT_RECORDING_STATE_CHANGED, (event) => {
      const backendState = event.payload.state;
      diagLog(`[FE] recording_state_changed: ${backendState}`);

      controllerRef.current?.syncRecordingState(backendState);

      // If backend went idle while the controller thinks recording is active,
      // the backend had an unexpected issue (device disconnect, watchdog).
      if (backendState === "idle" && controllerRef.current?.isRecording()) {
        diagLog(
          "[FE] recording_state_changed: unexpected idle, checking status",
        );
        void getRecordingStatus().then((status) => {
          const lastError = status.lastError;
          if (lastError) {
            // Check for Internal error - indicates hardware reset
            if (lastError.code === "internal") {
              setError("Hardware error: Recording reset");
            } else {
              setError(
                recordingErrorMessage(lastError) ||
                  "Recording failed unexpectedly",
              );
            }
          }
          setAppState("idle");
          syncFromPipeline("idle");
          void bubbleHide();
        });
      }
    })
      .then((nextUnlisten) => {
        if (isActive) {
          unlisten = nextUnlisten;
        } else {
          nextUnlisten();
        }
      })
      .catch(() => {
        if (isActive) {
          setError("Floe could not listen for recording state changes.");
        }
      });

    return () => {
      isActive = false;
      unlisten?.();
    };
  }, []);
  // Show/hide bubble overlay based on recording state
  useEffect(() => {
    if (
      appState === "recording" ||
      appState === "starting" ||
      appState === "stopping"
    ) {
      void bubbleShow();
    } else {
      void bubbleHide();
    }

    return () => {
      void bubbleHide();
    };
  }, [appState]);

  const triggerStart = useCallback(() => {
    void handleHotkeyEvent("Pressed");
  }, [handleHotkeyEvent]);

  const triggerStop = useCallback(() => {
    void handleHotkeyEvent("Released");
  }, [handleHotkeyEvent]);

  const confirmPreview = useCallback(() => {
    void controllerRef.current?.confirmPreview();
  }, []);

  const discardPreview = useCallback(() => {
    void controllerRef.current?.discardPreview();
  }, []);

  return {
    appState,
    error,
    latestTranscript,
    latestDiagnosticsJson,
    recordingStatus,
    latestRecording,
    triggerStart,
    triggerStop,
    confirmPreview,
    discardPreview,
  };
}
