import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppShell } from "./components/AppShell";
import { OnboardingView } from "./components/OnboardingView";
import { OverviewView } from "./components/OverviewView";
import { SettingsView } from "./components/SettingsView";
import { clipboardErrorMessage } from "./lib/clipboardErrors";
import { isMacLikePlatform } from "./lib/hotkeyCapture";
import { PushToTalkController } from "./lib/pushToTalk";
import { recordingErrorMessage } from "./lib/recordingErrors";
import { computeVisibleSetupState } from "./lib/setupState";
import { statusLabel } from "./lib/status";
import {
  bubbleHide,
  bubbleShow,
  cleanupTranscript,
  clearApiKey,
  copyTextToClipboard,
  diagLog,
  getApiKeyStatus,
  getHotkeySettings,
  getRecordingStatus,
  isTauriRuntime,
  pasteClipboard,
  resetHotkeyToDefault,
  saveApiKey,
  setHotkey,
  setStartAtLoginEnabled,
  startRecording,
  stopRecording,
  transcribeLatestRecording,
} from "./lib/tauri";
import { getStartAtLoginStatus as loadStartAtLoginStatus } from "./lib/tauri";
import type {
  AppState,
  ClipboardError,
  ApiKeyStatus,
  SttError,
  HotkeyError,
  HotkeyStatus,
  SettingsError,
  StartAtLoginError,
  StartAtLoginStatus,
} from "./types/app";

type View = "overview" | "settings";

const HOTKEY_UNAVAILABLE_STATUS: HotkeyStatus = isMacLikePlatform()
  ? {
      accelerator: "Alt+Space",
      label: "Option + Space",
      isDefault: true,
      isRegistered: false,
      error: "Hotkey unavailable",
    }
  : {
      accelerator: "Control+Space",
      label: "Ctrl + Space",
      isDefault: true,
      isRegistered: false,
      error: "Hotkey unavailable",
    };

export default function App() {
  const [view, setView] = useState<View>("overview");
  const [appState, setAppState] = useState<AppState>("ready");
  const [error, setError] = useState<string | null>(null);
  const [hotkeyStatus, setHotkeyStatus] = useState<HotkeyStatus | null>(null);
  const [startAtLoginStatus, setStartAtLoginStatus] =
    useState<StartAtLoginStatus | null>(null);
  const [apiKeyStatus, setApiKeyStatus] = useState<ApiKeyStatus | null>(null);
  const [latestDiagnosticsJson, setLatestDiagnosticsJson] = useState<
    string | null
  >(null);
  const [showHotkeyStepAfterSave, setShowHotkeyStepAfterSave] = useState(false);
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
        onDiagnosticsChange: setLatestDiagnosticsJson,
        errorMessage: pushToTalkErrorMessage,
      },
    );
  }

  useEffect(() => {
    getApiKeyStatus()
      .then((status) => {
        setApiKeyStatus(status);
      })
      .catch(() => {
        setApiKeyStatus({
          configured: false,
          maskedPreview: null,
        });
        setError("Floe could not load API key status.");
      });

    getHotkeySettings()
      .then((hotkey) => {
        setHotkeyStatus(hotkey);
      })
      .catch(() => {
        console.warn("Floe could not load hotkey status.");
        setHotkeyStatus(HOTKEY_UNAVAILABLE_STATUS);
        setError("Floe could not load hotkey status.");
      });

    loadStartAtLoginStatus()
      .then(setStartAtLoginStatus)
      .catch(() => {
        setStartAtLoginStatus({
          enabled: false,
          available: false,
        });
      });
  }, []);

  const handleHotkeyEvent = useCallback(
    async (state: "Pressed" | "Released") => {
      diagLog(`[FE] handleHotkeyEvent: state=${state}`);
      if (state === "Released") {
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

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;

    listen<{ state: "Pressed" | "Released" }>(
      "floe-global-hotkey-state",
      (event) => {
        diagLog(`[FE] listen callback: state=${event.payload.state}`);
        void handleHotkeyEvent(event.payload.state);
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
          void bubbleHide();
        }
      });

    return () => {
      isActive = false;
      unlisten?.();
    };
  }, [handleHotkeyEvent]);

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

  const handleSaveApiKey = useCallback(
    async (value: string) => {
      const wasConfigured = apiKeyStatus?.configured === true;

      try {
        const next = await saveApiKey(value);
        setApiKeyStatus(next);
        setShowHotkeyStepAfterSave(!wasConfigured);
        setError(null);
      } catch (caught) {
        throw settingsErrorForOnboarding(caught);
      }
    },
    [apiKeyStatus],
  );

  const handleClearApiKey = useCallback(async () => {
    try {
      const next = await clearApiKey();
      setApiKeyStatus(next);
      setShowHotkeyStepAfterSave(false);
      setError(null);
    } catch (caught) {
      setError(settingsErrorMessage(caught));
      throw caught;
    }
  }, []);

  const handleChangeHotkey = useCallback(async (accelerator: string) => {
    try {
      const next = await setHotkey(accelerator);
      setHotkeyStatus(next);
      setError(null);
    } catch (caught) {
      throw hotkeyErrorForOnboarding(caught);
    }
  }, []);

  const handleResetHotkey = useCallback(async () => {
    try {
      const next = await resetHotkeyToDefault();
      setHotkeyStatus(next);
      setError(null);
    } catch (caught) {
      setError(hotkeyErrorMessage(caught));
    }
  }, []);

  const handleSetStartAtLogin = useCallback(async (enabled: boolean) => {
    try {
      const next = await setStartAtLoginEnabled(enabled);
      setStartAtLoginStatus(next);
      setError(null);
    } catch (caught) {
      setError(startAtLoginErrorMessage(caught, enabled));
      throw caught;
    }
  }, []);

  const handleCompleteOnboarding = useCallback(() => {
    setShowHotkeyStepAfterSave(false);
    setView("overview");
  }, []);

  const setupState = computeVisibleSetupState(
    apiKeyStatus,
    hotkeyStatus,
    showHotkeyStepAfterSave,
  );

  useEffect(() => {
    if (setupState === "ready") {
      setError(null);
    }
  }, [setupState]);

  const hotkeyLabel = hotkeyStatus?.label ?? "Loading";
  const flowBusy =
    appState === "transcribing" ||
    appState === "cleaning" ||
    appState === "pasting" ||
    appState === "recording";
  const dynamicStatus = error ?? statusLabel(appState);

  useEffect(() => {
    if (appState === "recording") {
      void bubbleShow();
    } else {
      void bubbleHide();
    }

    return () => {
      void bubbleHide();
    };
  }, [appState]);

  if (setupState !== "ready") {
    return (
      <AppShell>
        <OnboardingView
          step={setupState}
          hotkeyStatus={hotkeyStatus}
          onSaveApiKey={handleSaveApiKey}
          onChangeHotkey={handleChangeHotkey}
          onComplete={handleCompleteOnboarding}
          busy={flowBusy}
        />
      </AppShell>
    );
  }

  if (view === "settings") {
    return (
      <AppShell>
        <SettingsView
          apiKeyStatus={apiKeyStatus}
          hotkeyStatus={hotkeyStatus}
          startAtLoginStatus={startAtLoginStatus}
          onClose={() => setView("overview")}
          onSaveApiKey={handleSaveApiKey}
          onClearApiKey={handleClearApiKey}
          onChangeHotkey={handleChangeHotkey}
          onResetHotkey={handleResetHotkey}
          onSetStartAtLogin={handleSetStartAtLogin}
          busy={flowBusy}
        />
      </AppShell>
    );
  }

  return (
    <AppShell>
      <OverviewView
        status={dynamicStatus}
        hotkeyLabel={hotkeyLabel}
        onOpenSettings={() => setView("settings")}
        diagnosticsJson={latestDiagnosticsJson}
        onCopyDiagnostics={copyTextToClipboard}
      />
    </AppShell>
  );
}

function settingsErrorForOnboarding(caught: unknown): Error {
  const settingsError = caught as Partial<SettingsError>;

  if (typeof settingsError.message === "string") {
    return new Error(settingsError.message);
  }

  return new Error("Could not save key");
}

function settingsErrorMessage(caught: unknown): string {
  const settingsError = caught as Partial<SettingsError>;

  if (typeof settingsError.message === "string") {
    return settingsError.message;
  }

  return "Settings could not be saved.";
}

function hotkeyErrorForOnboarding(caught: unknown): Error {
  const message = hotkeyErrorMessage(caught);

  if (message === "Hotkey unavailable") {
    return new Error(message);
  }

  return new Error(message);
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

  const maybeTranscriptionError = caught as Partial<SttError>;
  if (typeof maybeTranscriptionError.code === "string") {
    return transcriptionErrorMessage(caught);
  }

  return recordingErrorMessage(caught);
}

function transcriptionErrorMessage(caught: unknown): string {
  const transcriptionError = caught as Partial<SttError>;
  if (typeof transcriptionError.message === "string") {
    return transcriptionError.message;
  }
  return "Transcription failed";
}
