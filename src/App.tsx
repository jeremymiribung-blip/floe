import { listen } from "@tauri-apps/api/event";
import {
  EVENT_SHOW_SETTINGS,
  EVENT_SHUTTING_DOWN,
  listenHotkeyState,
  listenRecordingStateChanged,
} from "./lib/contract";
import { useCallback, useEffect, useRef, useState } from "react";
import { AppShell } from "./components/AppShell";
import { OnboardingView } from "./components/OnboardingView";
import { OverviewView } from "./components/OverviewView";
import { SettingsView } from "./components/SettingsView";
import { clipboardErrorMessage } from "./lib/clipboardErrors";
import {
  isFloeErrorDomain,
  floeErrorMessage,
  parseFloeError,
  startAtLoginErrorMessage,
} from "./lib/errors";
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
  ApiKeyStatus,
  FloeError,
  HotkeyStatus,
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
  const shuttingDownRef = useRef(false);

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
      if (shuttingDownRef.current) {
        diagLog("[FE] handleHotkeyEvent: ignored during shutdown");
        void bubbleHide();
        return;
      }

      diagLog(`[FE] handleHotkeyEvent: state=${state}`);
      if (state === "Released") {
        void bubbleHide();
      }

      try {
        await controllerRef.current?.handleShortcutState(state);
      } catch (caught) {
        setAppState("error");
        setError(floeErrorMessage(parseFloeError(caught)));
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

    listenHotkeyState((payload) => {
      if (!isActive) return;
      diagLog(`[FE] listenHotkeyState: state=${payload.state}`);
      void handleHotkeyEvent(payload.state);
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

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;

    listen(EVENT_SHOW_SETTINGS, () => {
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

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;

    listen(EVENT_SHUTTING_DOWN, () => {
      shuttingDownRef.current = true;
      void bubbleHide();
    })
      .then((nextUnlisten) => {
        if (isActive) {
          unlisten = nextUnlisten;
        } else {
          nextUnlisten();
        }
      })
      .catch(() => {
        // Shutdown notification is best-effort; the backend terminates regardless.
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

    listenRecordingStateChanged((payload) => {
      if (!isActive) return;
      controllerRef.current?.syncRecordingState(payload.state);
    })
      .then((nextUnlisten) => {
        if (isActive) {
          unlisten = nextUnlisten;
        } else {
          nextUnlisten();
        }
      })
      .catch(() => {
        // State sync is best-effort; the frontend self-heals on next invoke.
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
        throw settingsErrorForOnboarding(parseFloeError(caught));
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
      setError(settingsErrorMessage(parseFloeError(caught)));
      throw caught;
    }
  }, []);

  const handleChangeHotkey = useCallback(async (accelerator: string) => {
    try {
      const next = await setHotkey(accelerator);
      setHotkeyStatus(next);
      setError(null);
    } catch (caught) {
      throw hotkeyErrorForOnboarding(parseFloeError(caught));
    }
  }, []);

  const handleResetHotkey = useCallback(async () => {
    try {
      const next = await resetHotkeyToDefault();
      setHotkeyStatus(next);
      setError(null);
    } catch (caught) {
      setError(hotkeyErrorMessage(parseFloeError(caught)));
    }
  }, []);

  const handleSetStartAtLogin = useCallback(async (enabled: boolean) => {
    try {
      const next = await setStartAtLoginEnabled(enabled);
      setStartAtLoginStatus(next);
      setError(null);
    } catch (caught) {
      setError(startAtLoginErrorMessage(parseFloeError(caught), enabled));
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
    appState === "starting" ||
    appState === "recording" ||
    appState === "stopping";
  const dynamicStatus = error ?? statusLabel(appState);

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

function settingsErrorForOnboarding(error: FloeError): Error {
  return new Error(floeErrorMessage(error));
}

function settingsErrorMessage(error: FloeError): string {
  return floeErrorMessage(error) || "Settings could not be saved.";
}

function hotkeyErrorForOnboarding(error: FloeError): Error {
  return new Error(hotkeyErrorMessage(error));
}

function hotkeyErrorMessage(error: FloeError): string {
  if (isFloeErrorDomain(error, "hotkey")) {
    if (
      error.code === "alreadyInUse" ||
      error.code === "unsupportedHotkey" ||
      error.code === "registrationFailed"
    ) {
      return "Hotkey unavailable";
    }
    return error.message;
  }
  return "Hotkey unavailable";
}

function pushToTalkErrorMessage(error: FloeError): string {
  if (isFloeErrorDomain(error, "clipboard")) {
    return clipboardErrorMessage(error);
  }
  if (isFloeErrorDomain(error, "stt")) {
    return transcriptionErrorMessage(error);
  }
  if (isFloeErrorDomain(error, "recording")) {
    return recordingErrorMessage(error);
  }
  return error.message;
}

function transcriptionErrorMessage(error: FloeError): string {
  return floeErrorMessage(error) || "Transcription failed";
}
