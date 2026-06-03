import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppShell } from "./components/AppShell";
import { SettingsView } from "./components/SettingsView";
import { StatusView } from "./components/StatusView";
import { PushToTalkController } from "./lib/pushToTalk";
import { statusLabel } from "./lib/status";
import {
  bubbleHide,
  bubbleShow,
  cleanupTranscript,
  clearCerebrasApiKey,
  clearGroqApiKey,
  copyTextToClipboard,
  getCerebrasApiKeyStatus,
  getGroqApiKeyStatus,
  getHotkeySettings,
  getRecordingStatus,
  getStartAtLoginStatus,
  isTauriRuntime,
  pasteClipboard,
  resetHotkeyToDefault,
  saveCerebrasApiKey,
  saveGroqApiKey,
  setHotkey,
  setStartAtLoginEnabled,
  startRecording,
  stopRecording,
  transcribeLatestRecording,
} from "./lib/tauri";
import type {
  AppState,
  CerebrasApiKeyStatus,
  ClipboardError,
  GroqApiKeyStatus,
  GroqTranscriptionError,
  HotkeyError,
  HotkeyStatus,
  RecordingError,
  SettingsError,
  StartAtLoginError,
  StartAtLoginStatus,
} from "./types/app";

type View = "status" | "settings";

export default function App() {
  const [view, setView] = useState<View>("status");
  const [appState, setAppState] = useState<AppState>("ready");
  const [error, setError] = useState<string | null>(null);
  const [hotkeyStatus, setHotkeyStatus] = useState<HotkeyStatus | null>(null);
  const [startAtLoginStatus, setStartAtLoginStatus] =
    useState<StartAtLoginStatus | null>(null);
  const [groqStatus, setGroqStatus] = useState<GroqApiKeyStatus | null>(null);
  const [cerebrasStatus, setCerebrasStatus] =
    useState<CerebrasApiKeyStatus | null>(null);
  const controllerRef = useRef<PushToTalkController | null>(null);

  if (controllerRef.current === null) {
    controllerRef.current = new PushToTalkController(
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
        onStateChange: setAppState,
        onErrorChange: setError,
        onRecordingStatusChange: () => undefined,
        onLatestRecordingChange: () => undefined,
        onTranscriptChange: () => undefined,
        errorMessage: pushToTalkErrorMessage,
      },
    );
  }

  useEffect(() => {
    Promise.all([
      getGroqApiKeyStatus(),
      getCerebrasApiKeyStatus(),
      getHotkeySettings(),
    ])
      .then(([groq, cerebras, hotkey]) => {
        setGroqStatus(groq);
        setCerebrasStatus(cerebras);
        setHotkeyStatus(hotkey);
        if (!hotkey.isRegistered) {
          setError(hotkey.registrationError ?? "Hotkey unavailable");
          setAppState("error");
        } else {
          setAppState("ready");
        }
      })
      .catch(() => {
        setError("Floe could not load setup state.");
        setAppState("error");
      });

    getStartAtLoginStatus()
      .then(setStartAtLoginStatus)
      .catch(() => {
        setStartAtLoginStatus({
          enabled: false,
          available: false,
        });
      });
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;

    listen<{ state: "Pressed" | "Released" }>(
      "floe-global-hotkey-state",
      (event) => {
        void controllerRef.current?.handleShortcutState(event.payload.state);
      },
    )
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
          setAppState("error");
        }
      });

    return () => {
      isActive = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;

    listen("floe-show-settings", () => {
      setView("settings");
    })
      .then((nextUnlisten) => {
        if (isActive) {
          unlisten = nextUnlisten;
        } else {
          nextUnlisten();
        }
      })
      .catch(() => {
        // Tray settings is a convenience path; setup failure should not block the app.
      });

    return () => {
      isActive = false;
      unlisten?.();
    };
  }, []);

  const handleSaveGroq = useCallback(async (value: string) => {
    try {
      const next = await saveGroqApiKey(value);
      setGroqStatus(next);
      setError(null);
      setAppState("ready");
    } catch (caught) {
      setError(settingsErrorMessage(caught));
      setAppState("error");
      throw caught;
    }
  }, []);

  const handleClearGroq = useCallback(async () => {
    try {
      setGroqStatus(await clearGroqApiKey());
      setError(null);
      setAppState("ready");
    } catch (caught) {
      setError(settingsErrorMessage(caught));
      setAppState("error");
      throw caught;
    }
  }, []);

  const handleSaveCerebras = useCallback(async (value: string) => {
    try {
      const next = await saveCerebrasApiKey(value);
      setCerebrasStatus(next);
      setError(null);
      setAppState("ready");
    } catch (caught) {
      setError(settingsErrorMessage(caught));
      setAppState("error");
      throw caught;
    }
  }, []);

  const handleClearCerebras = useCallback(async () => {
    try {
      setCerebrasStatus(await clearCerebrasApiKey());
      setError(null);
      setAppState("ready");
    } catch (caught) {
      setError(settingsErrorMessage(caught));
      setAppState("error");
      throw caught;
    }
  }, []);

  const handleChangeHotkey = useCallback(async (accelerator: string) => {
    try {
      const next = await setHotkey(accelerator);
      setHotkeyStatus(next);
      setError(null);
      setAppState("ready");
    } catch (caught) {
      setError(hotkeyErrorMessage(caught));
      setAppState("error");
      throw caught;
    }
  }, []);

  const handleResetHotkey = useCallback(async () => {
    try {
      const next = await resetHotkeyToDefault();
      setHotkeyStatus(next);
      setError(null);
      setAppState("ready");
    } catch (caught) {
      setError(hotkeyErrorMessage(caught));
      setAppState("error");
    }
  }, []);

  const handleSetStartAtLogin = useCallback(async (enabled: boolean) => {
    try {
      const next = await setStartAtLoginEnabled(enabled);
      setStartAtLoginStatus(next);
      setError(null);
      setAppState("ready");
    } catch (caught) {
      setError(startAtLoginErrorMessage(caught, enabled));
      setAppState("error");
      throw caught;
    }
  }, []);

  const hotkeyLabel = hotkeyStatus?.configured.label ?? "Loading";
  const flowBusy =
    appState === "transcribing" ||
    appState === "cleaning" ||
    appState === "pasting" ||
    appState === "recording";
  const statusText =
    error && appState === "error" ? "Error" : statusLabel(appState);

  useEffect(() => {
    if (appState === "recording") {
      void bubbleShow();
    } else {
      void bubbleHide();
    }
  }, [appState]);

  return (
    <AppShell>
      {view === "status" ? (
        <StatusView
          status={statusText}
          hotkeyLabel={hotkeyLabel}
          error={appState === "error" ? error : null}
          onOpenSettings={() => setView("settings")}
        />
      ) : (
        <SettingsView
          groqStatus={groqStatus}
          cerebrasStatus={cerebrasStatus}
          hotkeyStatus={hotkeyStatus}
          startAtLoginStatus={startAtLoginStatus}
          onClose={() => setView("status")}
          onSaveGroq={handleSaveGroq}
          onClearGroq={handleClearGroq}
          onSaveCerebras={handleSaveCerebras}
          onClearCerebras={handleClearCerebras}
          onChangeHotkey={handleChangeHotkey}
          onResetHotkey={handleResetHotkey}
          onSetStartAtLogin={handleSetStartAtLogin}
          busy={flowBusy}
        />
      )}
    </AppShell>
  );
}

function settingsErrorMessage(caught: unknown): string {
  const settingsError = caught as Partial<SettingsError>;

  if (typeof settingsError.message === "string") {
    return settingsError.message;
  }

  return "Settings could not be saved.";
}

function hotkeyErrorMessage(caught: unknown): string {
  const hotkeyError = caught as Partial<HotkeyError>;

  if (hotkeyError.code === "alreadyInUse") {
    return "Hotkey unavailable";
  }
  if (hotkeyError.code === "unsupportedHotkey") {
    return "Hotkey unavailable";
  }
  if (hotkeyError.code === "registrationFailed") {
    return "Hotkey unavailable";
  }
  if (typeof hotkeyError.message === "string") {
    return hotkeyError.message;
  }

  return "Hotkey unavailable";
}

function startAtLoginErrorMessage(caught: unknown, enabling: boolean): string {
  const startAtLoginError = caught as Partial<StartAtLoginError>;

  if (
    startAtLoginError.message === "Could not enable start at login" ||
    startAtLoginError.message === "Could not disable start at login" ||
    startAtLoginError.message === "Start at login unavailable"
  ) {
    return startAtLoginError.message;
  }

  if (startAtLoginError.code === "unavailable") {
    return "Start at login unavailable";
  }

  return enabling
    ? "Could not enable start at login"
    : "Could not disable start at login";
}

function pushToTalkErrorMessage(caught: unknown): string {
  const maybeClipboardError = caught as Partial<ClipboardError>;
  if (
    maybeClipboardError.code === "clipboardUnavailable" ||
    maybeClipboardError.code === "pasteUnavailable"
  ) {
    return clipboardErrorMessage(caught);
  }

  const maybeTranscriptionError = caught as Partial<GroqTranscriptionError>;
  if (typeof maybeTranscriptionError.code === "string") {
    return transcriptionErrorMessage(caught);
  }

  return recordingErrorMessage("push-to-talk", caught);
}

function recordingErrorMessage(action: string, caught: unknown): string {
  const recordingError = caught as Partial<RecordingError>;
  if (recordingError.code === "alreadyRecording") {
    return "Recording already active";
  }
  if (action === "start" && typeof recordingError.message !== "string") {
    return "Recording could not start";
  }
  if (typeof recordingError.message === "string") {
    return recordingError.message;
  }
  return "Recording failed";
}

function transcriptionErrorMessage(caught: unknown): string {
  const transcriptionError = caught as Partial<GroqTranscriptionError>;
  if (typeof transcriptionError.message === "string") {
    return transcriptionError.message;
  }
  return "Transcription failed";
}

function clipboardErrorMessage(caught: unknown): string {
  const clipboardError = caught as Partial<ClipboardError>;
  if (clipboardError.code === "pasteUnavailable") {
    return "Paste failed";
  }
  if (typeof clipboardError.message === "string") {
    return clipboardError.message;
  }
  return "Clipboard failed";
}
