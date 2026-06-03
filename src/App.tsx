import {
  Activity,
  Clipboard,
  Copy,
  Info,
  KeyRound,
  Mic,
  Settings,
  Square,
  Trash2,
  WandSparkles,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import type { FormEvent } from "react";
import { useEffect, useRef, useState } from "react";
import { formatRecordingInfo } from "./lib/recording";
import { PushToTalkController } from "./lib/pushToTalk";
import {
  cleanupTranscript,
  clearCerebrasApiKey,
  clearGroqApiKey,
  copyTextToClipboard,
  getAppSettings,
  getAppStatus,
  getCerebrasApiKeyStatus,
  getCleanupMode,
  getGroqApiKeyStatus,
  getLatestRecordingInfo,
  getRecordingStatus,
  isTauriRuntime,
  pasteClipboard,
  saveCerebrasApiKey,
  saveGroqApiKey,
  setCleanupMode,
  startRecording,
  stopRecording,
  transcribeLatestRecording,
} from "./lib/tauri";
import { statusLabel } from "./lib/status";
import type {
  AppSettings,
  AppState,
  AppStatus,
  CerebrasApiKeyStatus,
  ClipboardError,
  CleanupMode,
  GroqApiKeyStatus,
  GroqTranscriptionError,
  RecordingError,
  RecordingInfo,
  RecordingStatus,
  SettingsError,
} from "./types/app";

export default function App() {
  const [appState, setAppState] = useState<AppState>("idle");
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [apiKeyStatus, setApiKeyStatus] = useState<GroqApiKeyStatus | null>(
    null,
  );
  const [cerebrasApiKeyStatus, setCerebrasApiKeyStatus] =
    useState<CerebrasApiKeyStatus | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [groqApiKeyInput, setGroqApiKeyInput] = useState("");
  const [cerebrasApiKeyInput, setCerebrasApiKeyInput] = useState("");
  const [settingsMessage, setSettingsMessage] = useState<string | null>(null);
  const [recordingStatus, setRecordingStatus] =
    useState<RecordingStatus | null>(null);
  const [latestRecording, setLatestRecording] = useState<RecordingInfo | null>(
    null,
  );
  const [latestTranscript, setLatestTranscript] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const settingsPanelRef = useRef<HTMLElement | null>(null);
  const pushToTalkController = useRef<PushToTalkController | null>(null);
  const manualTranscriptionInFlight = useRef(false);
  const hotkeyAccelerator = settings?.hotkey.accelerator;
  const hotkeyLabel = settings?.hotkey.label;

  if (pushToTalkController.current === null) {
    pushToTalkController.current = new PushToTalkController(
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
        onRecordingStatusChange: applyRecordingStatus,
        onLatestRecordingChange: setLatestRecording,
        onTranscriptChange: setLatestTranscript,
        errorMessage: pushToTalkErrorMessage,
      },
    );
  }

  useEffect(() => {
    Promise.all([
      getAppStatus(),
      getGroqApiKeyStatus(),
      getCerebrasApiKeyStatus(),
      getAppSettings(),
      getRecordingStatus(),
    ])
      .then(
        ([
          appStatus,
          currentApiKeyStatus,
          currentCerebrasApiKeyStatus,
          currentSettings,
          currentRecordingStatus,
        ]) => {
          setStatus(appStatus);
          setApiKeyStatus(currentApiKeyStatus);
          setCerebrasApiKeyStatus(currentCerebrasApiKeyStatus);
          setSettings(currentSettings);
          setRecordingStatus(currentRecordingStatus);
          setLatestRecording(currentRecordingStatus.latestRecording);
          setAppState(
            currentRecordingStatus.isRecording ? "recording" : "idle",
          );
        },
      )
      .catch(() => {
        setError("Floe could not load setup state.");
        setAppState("error");
      });
  }, []);

  useEffect(() => {
    if (!hotkeyAccelerator || !hotkeyLabel || !isTauriRuntime()) {
      return;
    }

    let isActive = true;

    register(hotkeyAccelerator, (event) => {
      if (event.shortcut !== hotkeyAccelerator) {
        return;
      }

      void pushToTalkController.current?.handleShortcutState(event.state);
    })
      .then(() => {
        if (isActive) {
          setError(null);
        }
      })
      .catch(() => {
        if (isActive) {
          setError(
            `Floe could not register ${hotkeyLabel}. Manual controls are still available.`,
          );
          setAppState("error");
        }
      });

    return () => {
      isActive = false;
      void unregister(hotkeyAccelerator);
    };
  }, [hotkeyAccelerator, hotkeyLabel]);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;

    listen("floe-show-settings", () => {
      const panel = settingsPanelRef.current;

      if (!panel) {
        return;
      }

      panel.scrollIntoView({ block: "start", behavior: "smooth" });
      panel.focus({ preventScroll: true });
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

  function applyRecordingStatus(nextStatus: RecordingStatus) {
    setRecordingStatus(nextStatus);
    setLatestRecording(nextStatus.latestRecording);
    setAppState(nextStatus.isRecording ? "recording" : "idle");
  }

  function recordingErrorMessage(action: string, caught: unknown): string {
    const recordingError = caught as Partial<RecordingError>;

    if (typeof recordingError.message === "string") {
      return recordingError.message;
    }

    return `The ${action} recording check failed.`;
  }

  function settingsErrorMessage(caught: unknown): string {
    const settingsError = caught as Partial<SettingsError>;

    if (typeof settingsError.message === "string") {
      return settingsError.message;
    }

    return "Settings could not be saved.";
  }

  function transcriptionErrorMessage(caught: unknown): string {
    const transcriptionError = caught as Partial<GroqTranscriptionError>;

    if (typeof transcriptionError.message === "string") {
      return transcriptionError.message;
    }

    return "The transcription request failed.";
  }

  function clipboardErrorMessage(caught: unknown): string {
    const clipboardError = caught as Partial<ClipboardError>;

    if (clipboardError.code === "pasteUnavailable") {
      return "Transcript copied to clipboard, but Floe could not send the paste shortcut. Paste manually with Command+V or Control+V.";
    }

    if (typeof clipboardError.message === "string") {
      return clipboardError.message;
    }

    return "Floe could not complete the clipboard action.";
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

  function cleanupModeLabel(cleanupMode: CleanupMode): string {
    const labels: Record<CleanupMode, string> = {
      raw: "Raw",
      fast: "Fast",
      clean: "Clean",
    };

    return labels[cleanupMode];
  }

  async function handleSaveGroqApiKey(event: FormEvent) {
    event.preventDefault();

    try {
      setError(null);
      setSettingsMessage(null);
      const nextStatus = await saveGroqApiKey(groqApiKeyInput);
      setApiKeyStatus(nextStatus);
      setGroqApiKeyInput("");
      setSettingsMessage("Groq API key saved.");
      setAppState("idle");
    } catch (caught) {
      setSettingsMessage(settingsErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handleClearGroqApiKey() {
    try {
      setError(null);
      setSettingsMessage(null);
      setApiKeyStatus(await clearGroqApiKey());
      setGroqApiKeyInput("");
      setSettingsMessage("Groq API key cleared.");
      setAppState("idle");
    } catch (caught) {
      setSettingsMessage(settingsErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handleSaveCerebrasApiKey(event: FormEvent) {
    event.preventDefault();

    try {
      setError(null);
      setSettingsMessage(null);
      const nextStatus = await saveCerebrasApiKey(cerebrasApiKeyInput);
      setCerebrasApiKeyStatus(nextStatus);
      setCerebrasApiKeyInput("");
      setSettingsMessage("Cerebras API key saved.");
      setAppState("idle");
    } catch (caught) {
      setSettingsMessage(settingsErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handleClearCerebrasApiKey() {
    try {
      setError(null);
      setSettingsMessage(null);
      setCerebrasApiKeyStatus(await clearCerebrasApiKey());
      setCerebrasApiKeyInput("");
      setSettings((currentSettings) =>
        currentSettings
          ? {
              ...currentSettings,
              cleanupMode:
                currentSettings.cleanupMode === "clean"
                  ? "fast"
                  : currentSettings.cleanupMode,
            }
          : currentSettings,
      );
      setSettingsMessage("Cerebras API key cleared.");
      setAppState("idle");
    } catch (caught) {
      setSettingsMessage(settingsErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handleCleanupModeChange(cleanupMode: CleanupMode) {
    try {
      setError(null);
      setSettingsMessage(null);
      const savedMode = await setCleanupMode(cleanupMode);
      setSettings((currentSettings) =>
        currentSettings
          ? {
              ...currentSettings,
              cleanupMode: savedMode,
            }
          : currentSettings,
      );
      setSettingsMessage(`Cleanup mode set to ${cleanupModeLabel(savedMode)}.`);
      setAppState("idle");
    } catch (caught) {
      const fallbackMode = await getCleanupMode().catch(
        () => "fast" as CleanupMode,
      );
      setSettings((currentSettings) =>
        currentSettings
          ? {
              ...currentSettings,
              cleanupMode: fallbackMode,
            }
          : currentSettings,
      );
      setSettingsMessage(settingsErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handleStartRecording() {
    if (isRecording || isFlowBusy) {
      return;
    }

    try {
      setError(null);
      setLatestTranscript(null);
      applyRecordingStatus(await startRecording());
    } catch (caught) {
      setError(recordingErrorMessage("start", caught));
      setAppState("error");
    }
  }

  async function handleStopRecording() {
    try {
      setError(null);
      const info = await stopRecording();
      setLatestRecording(info);
      applyRecordingStatus(await getRecordingStatus());
    } catch (caught) {
      setError(recordingErrorMessage("stop", caught));
      setAppState("error");
    }
  }

  async function handleRefreshRecordingStatus() {
    try {
      setError(null);
      applyRecordingStatus(await getRecordingStatus());
    } catch (caught) {
      setError(recordingErrorMessage("status", caught));
      setAppState("error");
    }
  }

  async function handleLatestRecordingInfo() {
    try {
      setError(null);
      setLatestRecording(await getLatestRecordingInfo());
      applyRecordingStatus(await getRecordingStatus());
    } catch (caught) {
      setError(recordingErrorMessage("latest info", caught));
      setAppState("error");
    }
  }

  async function handleTranscribeLatestRecording() {
    if (isRecording || isFlowBusy || manualTranscriptionInFlight.current) {
      return;
    }

    manualTranscriptionInFlight.current = true;

    try {
      setError(null);
      setAppState("transcribing");
      const transcription = await transcribeLatestRecording();
      setAppState("cleaning");
      const cleanup = await cleanupTranscript(transcription.text).catch(() => ({
        text: transcription.text,
        mode: "raw" as CleanupMode,
        warning: "Cleanup failed. Floe pasted the raw transcript instead.",
      }));
      const finalText = cleanup.text;
      setError(cleanup.warning);

      setLatestTranscript(finalText);

      if (finalText.trim().length === 0) {
        setAppState("idle");
        return;
      }

      setAppState("pasting");
      await copyTextToClipboard(finalText);
      await pasteClipboard();
      setAppState("pasted");
    } catch (caught) {
      const maybeClipboardError = caught as Partial<ClipboardError>;
      setError(
        maybeClipboardError.code === "clipboardUnavailable" ||
          maybeClipboardError.code === "pasteUnavailable"
          ? clipboardErrorMessage(caught)
          : transcriptionErrorMessage(caught),
      );
      setAppState("error");
    } finally {
      manualTranscriptionInFlight.current = false;
    }
  }

  async function handleCopyLatestTranscript() {
    if (!latestTranscript || latestTranscript.trim().length === 0) {
      return;
    }

    try {
      setError(null);
      await copyTextToClipboard(latestTranscript);
      setAppState("pasted");
    } catch (caught) {
      setError(clipboardErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handlePasteLatestTranscript() {
    if (!latestTranscript || latestTranscript.trim().length === 0) {
      return;
    }

    try {
      setError(null);
      setAppState("pasting");
      await copyTextToClipboard(latestTranscript);
      await pasteClipboard();
      setAppState("pasted");
    } catch (caught) {
      setError(clipboardErrorMessage(caught));
      setAppState("error");
    }
  }

  const isRecording = recordingStatus?.isRecording ?? false;
  const isFlowBusy =
    appState === "transcribing" ||
    appState === "cleaning" ||
    appState === "pasting";
  const safeLatestRecording =
    latestRecording ?? recordingStatus?.latestRecording ?? null;
  const hasPasteableTranscript =
    latestTranscript !== null && latestTranscript.trim().length > 0;

  return (
    <main>
      <div className="app-shell">
        <header className="app-header">
          <div>
            <p className="eyebrow">Desktop transcription</p>
            <h1>Floe</h1>
          </div>
          <div className={`status-pill status-pill-${appState}`}>
            <span aria-hidden="true" />
            {statusLabel(appState)}
          </div>
        </header>

        <section className="status-panel" aria-live="polite">
          <div>
            <p className="section-label">Status</p>
            <h2>{status?.appName ?? "Floe"} push-to-talk flow</h2>
          </div>
          <p>{error ?? status?.message ?? "Loading setup stubs..."}</p>
        </section>

        <section
          className="settings-panel"
          ref={settingsPanelRef}
          tabIndex={-1}
        >
          <div>
            <p className="section-label">Settings</p>
            <h2>
              <Settings aria-hidden="true" />
              Secure storage
            </h2>
          </div>
          <dl className="settings-summary">
            <div>
              <dt>Groq API key</dt>
              <dd>
                {apiKeyStatus?.configured
                  ? `Configured (${apiKeyStatus.maskedPreview})`
                  : "Not configured"}
              </dd>
            </div>
            <div>
              <dt>Cerebras API key</dt>
              <dd>
                {cerebrasApiKeyStatus?.configured
                  ? `Configured (${cerebrasApiKeyStatus.maskedPreview})`
                  : "Not configured"}
              </dd>
            </div>
            <div>
              <dt>Cleanup mode</dt>
              <dd>
                {settings?.cleanupMode
                  ? cleanupModeLabel(settings.cleanupMode)
                  : "Loading"}
              </dd>
            </div>
            <div>
              <dt>Global hotkey</dt>
              <dd>{settings?.hotkey.label ?? "Loading"}</dd>
            </div>
            <div>
              <dt>Secret storage</dt>
              <dd>OS keychain</dd>
            </div>
          </dl>
          <form className="settings-form" onSubmit={handleSaveGroqApiKey}>
            <label htmlFor="groq-api-key">Groq API key</label>
            <div className="field-row">
              <input
                id="groq-api-key"
                type="password"
                value={groqApiKeyInput}
                autoComplete="off"
                onChange={(event) => setGroqApiKeyInput(event.target.value)}
              />
              <button type="submit">
                <KeyRound aria-hidden="true" />
                Save key
              </button>
              <button
                className="secondary-button"
                type="button"
                onClick={handleClearGroqApiKey}
              >
                <Trash2 aria-hidden="true" />
                Clear
              </button>
            </div>
          </form>
          <form className="settings-form" onSubmit={handleSaveCerebrasApiKey}>
            <label htmlFor="cerebras-api-key">Cerebras API key</label>
            <div className="field-row">
              <input
                id="cerebras-api-key"
                type="password"
                value={cerebrasApiKeyInput}
                autoComplete="off"
                onChange={(event) => setCerebrasApiKeyInput(event.target.value)}
              />
              <button type="submit">
                <KeyRound aria-hidden="true" />
                Save key
              </button>
              <button
                className="secondary-button"
                type="button"
                onClick={handleClearCerebrasApiKey}
              >
                <Trash2 aria-hidden="true" />
                Clear
              </button>
            </div>
          </form>
          <div className="settings-form">
            <label htmlFor="cleanup-mode">Cleanup mode</label>
            <div className="field-row">
              <select
                id="cleanup-mode"
                value={settings?.cleanupMode ?? "fast"}
                onChange={(event) =>
                  void handleCleanupModeChange(
                    event.target.value as CleanupMode,
                  )
                }
              >
                <option value="raw">Raw</option>
                <option value="fast">Fast</option>
                <option value="clean">Clean</option>
              </select>
            </div>
          </div>
          {settingsMessage ? (
            <p className="settings-message">{settingsMessage}</p>
          ) : null}
        </section>

        <section className="manual-panel">
          <div>
            <p className="section-label">Manual testing</p>
            <h2>
              <Mic aria-hidden="true" />
              Recording controls
            </h2>
          </div>
          <div className="actions">
            <button
              type="button"
              disabled={isRecording || isFlowBusy}
              onClick={handleStartRecording}
            >
              <Mic aria-hidden="true" />
              Start
            </button>
            <button
              type="button"
              disabled={!isRecording}
              onClick={handleStopRecording}
            >
              <Square aria-hidden="true" />
              Stop
            </button>
            <button type="button" onClick={handleRefreshRecordingStatus}>
              <Activity aria-hidden="true" />
              Status
            </button>
            <button type="button" onClick={handleLatestRecordingInfo}>
              <Info aria-hidden="true" />
              Latest info
            </button>
          </div>

          <dl className="recording-metadata">
            <div>
              <dt>Recording</dt>
              <dd>{isRecording ? "Active" : "Idle"}</dd>
            </div>
            <div>
              <dt>Sample rate</dt>
              <dd>{recordingStatus?.sampleRate ?? "None"}</dd>
            </div>
            <div>
              <dt>Input channels</dt>
              <dd>{recordingStatus?.inputChannels ?? "None"}</dd>
            </div>
            <div>
              <dt>Duration cap</dt>
              <dd>{recordingStatus?.maxDurationSeconds ?? 120}s</dd>
            </div>
          </dl>

          {safeLatestRecording ? (
            <p className="manual-result">
              {formatRecordingInfo(safeLatestRecording)}
            </p>
          ) : null}

          {latestTranscript !== null ? (
            <div className="transcript-result">
              <p className="section-label">Transcript</p>
              <p>{latestTranscript || "No speech detected."}</p>
            </div>
          ) : null}

          <div className="actions">
            <button
              className="secondary-button"
              type="button"
              disabled={isRecording || isFlowBusy}
              onClick={handleTranscribeLatestRecording}
            >
              <WandSparkles aria-hidden="true" />
              Transcribe + paste
            </button>
            <button
              type="button"
              disabled={!hasPasteableTranscript || isFlowBusy}
              onClick={handleCopyLatestTranscript}
            >
              <Copy aria-hidden="true" />
              Copy transcript
            </button>
            <button
              type="button"
              disabled={!hasPasteableTranscript || isFlowBusy}
              onClick={handlePasteLatestTranscript}
            >
              <Clipboard aria-hidden="true" />
              Paste transcript
            </button>
          </div>
        </section>
      </div>
    </main>
  );
}
