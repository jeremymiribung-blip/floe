import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { EVENT_SHOW_SETTINGS } from "./lib/contract";
import {
  isTauriRuntime,
  getHotkeySettings,
  getApiKeyStatus,
} from "./lib/tauri";
import SettingsWindow from "./views/SettingsWindow";
import FloatingRecorderOverlay from "./components/overlay/FloatingRecorderOverlay";
import { usePushToTalk } from "./hooks/usePushToTalk";
import useFloeStore from "./stores/useFloeStore";

export default function App() {
  const { appState } = usePushToTalk();
  const setHotkey = useFloeStore((s) => s.setHotkey);
  const setApiKey = useFloeStore((s) => s.setApiKey);
  const setApiKeyStatus = useFloeStore((s) => s.setApiKeyStatus);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    getHotkeySettings()
      .then((status) => {
        setHotkey(status.label);
      })
      .catch((err) => console.error("getHotkeySettings failed:", err));
  }, [setHotkey]);

  useEffect(() => {
    getApiKeyStatus()
      .then((status) => {
        setApiKeyStatus(status.configured, status.maskedPreview);
        if (status.configured) {
          setApiKey(""); // key is configured in keyring, clear frontend placeholder
        }
      })
      .catch((err) => console.error("getApiKeyStatus failed:", err));
  }, [setApiKey, setApiKeyStatus]);

  useEffect(() => {
    const unlisten = listen(EVENT_SHOW_SETTINGS, () => {
      getCurrentWindow()
        .show()
        .catch((err) => console.error("show window failed:", err));
      getCurrentWindow()
        .setFocus()
        .catch((err) => console.error("setFocus failed:", err));
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const showOverlay = appState !== "idle" && appState !== "ready";

  return (
    <>
      <SettingsWindow
        onClose={() => {
          getCurrentWindow()
            .hide()
            .catch((err) => console.error("hide window failed:", err));
        }}
      />
      {showOverlay && <FloatingRecorderOverlay />}
    </>
  );
}
