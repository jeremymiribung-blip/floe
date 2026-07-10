import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { EVENT_SHOW_SETTINGS, EVENT_UPDATE_INSTALLED } from "./lib/contract";
import {
  isTauriRuntime,
  getHotkeySettings,
  getApiKeyStatus,
  getUpdateInfo,
  checkForUpdate,
} from "./lib/tauri";
import { logCritical, logRecoverable, errorMessage } from "./lib/errorLog";
import SettingsWindow from "./views/SettingsWindow";
import Onboarding from "./views/Onboarding";
import FloatingRecorderOverlay from "./components/overlay/FloatingRecorderOverlay";
import { usePushToTalk } from "./hooks/usePushToTalk";
import useFloeStore from "./stores/useFloeStore";

export default function App() {
  const { appState, error, latestTranscript, confirmPreview, discardPreview } =
    usePushToTalk();
  const setHotkeyStatus = useFloeStore((s) => s.setHotkeyStatus);
  const setApiKey = useFloeStore((s) => s.setApiKey);
  const setApiKeyStatus = useFloeStore((s) => s.setApiKeyStatus);
  const setUpdateInfo = useFloeStore((s) => s.setUpdateInfo);
  const setLastStartupError = useFloeStore((s) => s.setLastStartupError);
  const setupState = useFloeStore((s) => s.deriveSetupState());

  useEffect(() => {
    if (!isTauriRuntime()) return;
    getHotkeySettings()
      .then((status) => {
        setHotkeyStatus(status.label, status.isRegistered);
      })
      .catch((err) => {
        logCritical("startup getHotkeySettings", err);
        setLastStartupError(
          `Could not load hotkey settings: ${errorMessage(err)}`,
        );
      });
  }, [setHotkeyStatus, setLastStartupError]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    getApiKeyStatus()
      .then((status) => {
        setApiKeyStatus(status.configured, status.maskedPreview);
        if (status.configured) {
          setApiKey("");
        }
      })
      .catch((err) => {
        logCritical("startup getApiKeyStatus", err);
        setLastStartupError(
          `Could not load API key status: ${errorMessage(err)}`,
        );
      });
  }, [setApiKey, setApiKeyStatus, setLastStartupError]);

  // ── Update check on startup ────────────────────────────
  useEffect(() => {
    if (!isTauriRuntime()) return;
    getUpdateInfo()
      .then((info) => setUpdateInfo(info))
      .catch((err) => {
        logCritical("startup getUpdateInfo", err);
        setUpdateInfo({
          currentVersion: "1.0.0",
          latestVersion: null,
          status: "error",
          downloadProgress: 0,
          lastCheckResult: null,
          errorMessage: errorMessage(err),
        });
      });
    checkForUpdate()
      .then((info) => setUpdateInfo(info))
      .catch((err) => {
        logCritical("startup checkForUpdate", err);
        setUpdateInfo({
          currentVersion: "1.0.0",
          latestVersion: null,
          status: "error",
          downloadProgress: 0,
          lastCheckResult: null,
          errorMessage: errorMessage(err),
        });
      });
  }, [setUpdateInfo]);

  // ── Listen for update-installed event → close window ──
  useEffect(() => {
    const unlisten = listen(EVENT_UPDATE_INSTALLED, () => {
      getCurrentWindow()
        .close()
        .catch((err) =>
          logRecoverable("close window after update installed", err),
        );
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen(EVENT_SHOW_SETTINGS, () => {
      getCurrentWindow()
        .show()
        .catch((err) => logRecoverable("show window on settings event", err));
      getCurrentWindow()
        .setFocus()
        .catch((err) => logRecoverable("focus window on settings event", err));
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const showOverlay = appState !== "idle" && appState !== "ready";
  const isReady = setupState === "ready";

  return (
    <>
      {isReady ? (
        <SettingsWindow
          onClose={() => {
            getCurrentWindow()
              .hide()
              .catch((err) => logRecoverable("hide window on close", err));
          }}
        />
      ) : (
        <Onboarding />
      )}
      {showOverlay && (
        <FloatingRecorderOverlay
          error={error}
          appState={appState}
          transcript={latestTranscript}
          onConfirm={confirmPreview}
          onDiscard={discardPreview}
        />
      )}
    </>
  );
}
