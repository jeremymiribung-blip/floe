import {
  Activity,
  Clipboard,
  Copy,
  Info,
  KeyRound,
  Mic,
  RotateCcw,
  Settings,
  Square,
  Trash2,
  WandSparkles,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import type { FormEvent } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { captureHotkey } from "./lib/hotkeyCapture";
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
  getHotkeySettings,
  getLatestRecordingInfo,
  getRecordingStatus,
  isTauriRuntime,
  pasteClipboard,
  resetHotkeyToDefault,
  saveCerebrasApiKey,
  saveGroqApiKey,
  setCleanupMode,
  setHotkey,
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
  GlobalHotkeyEvent,
  GroqApiKeyStatus,
  GroqTranscriptionError,
  HotkeyError,
  HotkeyStatus,
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
  const [hotkeyStatus, setHotkeyStatus] = useState<HotkeyStatus | null>(null);
  const [isHotkeyCaptureActive, setIsHotkeyCaptureActive] = useState(false);
  const [hotkeyCaptureMessage, setHotkeyCaptureMessage] = useState<
    string | null
  >(null);
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
  const stateBeforeHotkeyCapture = useRef<AppState>("idle");

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
      getHotkeySettings(),
      getRecordingStatus(),
    ])
      .then(
        ([
          appStatus,
          currentApiKeyStatus,
          currentCerebrasApiKeyStatus,
          currentSettings,
          currentHotkeyStatus,
          currentRecordingStatus,
        ]) => {
          setStatus(appStatus);
          setApiKeyStatus(currentApiKeyStatus);
          setCerebrasApiKeyStatus(currentCerebrasApiKeyStatus);
          setSettings(currentSettings);
          setHotkeyStatus(currentHotkeyStatus);
          setRecordingStatus(currentRecordingStatus);
          setLatestRecording(currentRecordingStatus.latestRecording);
          setError(currentHotkeyStatus.registrationError);
          setAppState(
            !currentHotkeyStatus.isRegistered
              ? "error"
              : currentRecordingStatus.isRecording
                ? "recording"
                : "idle",
          );
        },
      )
      .catch(() => {
        setError("Floe could not load setup state.");
        setAppState("error");
      });
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let isActive = true;
    let unlisten: (() => void) | null = null;

    listen<GlobalHotkeyEvent>("floe-global-hotkey-state", (event) => {
      void pushToTalkController.current?.handleShortcutState(
        event.payload.state,
      );
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

    if (recordingError.code === "alreadyRecording") {
      return "Recording is already active.";
    }

    if (action === "start" && typeof recordingError.message !== "string") {
      return "Microphone recording could not start.";
    }

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

  function hotkeyErrorMessage(caught: unknown): string {
    const hotkeyError = caught as Partial<HotkeyError>;

    if (hotkeyError.code === "alreadyInUse") {
      return "This shortcut is already in use.";
    }

    if (hotkeyError.code === "unsupportedHotkey") {
      return "This shortcut is not supported.";
    }

    if (hotkeyError.code === "registrationFailed") {
      return "Hotkey could not be registered.";
    }

    if (typeof hotkeyError.message === "string") {
      return hotkeyError.message;
    }

    return "Hotkey could not be registered.";
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

  function beginHotkeyCapture() {
    if (isRecording || isFlowBusy) {
      return;
    }

    stateBeforeHotkeyCapture.current = appState;
    setError(null);
    setSettingsMessage(null);
    setHotkeyCaptureMessage("Press your new shortcut...");
    setIsHotkeyCaptureActive(true);
    setAppState("capturing_hotkey");
  }

  const cancelHotkeyCapture = useCallback(() => {
    setIsHotkeyCaptureActive(false);
    setHotkeyCaptureMessage(null);
    setSettingsMessage("Hotkey change canceled.");
    setAppState(stateBeforeHotkeyCapture.current);
  }, []);

  const saveCapturedHotkey = useCallback(async (accelerator: string) => {
    try {
      setHotkeyCaptureMessage("Saving shortcut...");
      const nextStatus = await setHotkey(accelerator);
      setHotkeyStatus(nextStatus);
      setSettings((currentSettings) =>
        currentSettings
          ? {
              ...currentSettings,
              hotkey: nextStatus.configured,
            }
          : currentSettings,
      );
      setIsHotkeyCaptureActive(false);
      setHotkeyCaptureMessage(null);
      setSettingsMessage(
        `Global hotkey set to ${nextStatus.configured.label}.`,
      );
      setAppState("idle");
    } catch (caught) {
      setIsHotkeyCaptureActive(false);
      setHotkeyCaptureMessage(null);
      const currentHotkeyStatus = await getHotkeySettings().catch(() => null);
      if (currentHotkeyStatus) {
        setHotkeyStatus(currentHotkeyStatus);
      }
      setSettingsMessage(hotkeyErrorMessage(caught));
      setAppState("error");
    }
  }, []);

  useEffect(() => {
    if (!isHotkeyCaptureActive) {
      return;
    }

    function handleCaptureKeydown(event: KeyboardEvent) {
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        cancelHotkeyCapture();
        return;
      }

      try {
        const captured = captureHotkey(event);
        void saveCapturedHotkey(captured.accelerator);
      } catch (caught) {
        setHotkeyCaptureMessage(
          caught instanceof Error
            ? caught.message
            : "This shortcut is not supported.",
        );
      }
    }

    window.addEventListener("keydown", handleCaptureKeydown, true);

    return () => {
      window.removeEventListener("keydown", handleCaptureKeydown, true);
    };
  }, [cancelHotkeyCapture, isHotkeyCaptureActive, saveCapturedHotkey]);

  async function handleResetHotkeyToDefault() {
    if (isRecording || isFlowBusy) {
      return;
    }

    try {
      setError(null);
      setSettingsMessage(null);
      const nextStatus = await resetHotkeyToDefault();
      setHotkeyStatus(nextStatus);
      setSettings((currentSettings) =>
        currentSettings
          ? {
              ...currentSettings,
              hotkey: nextStatus.configured,
            }
          : currentSettings,
      );
      setSettingsMessage(
        `Global hotkey reset to ${nextStatus.configured.label}.`,
      );
      setAppState("idle");
    } catch (caught) {
      setHotkeyStatus(await getHotkeySettings().catch(() => hotkeyStatus));
      setSettingsMessage(hotkeyErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handleStartRecording() {
    if (isRecording) {
      setError("Recording is already active.");
      return;
    }

    if (isFlowBusy) {
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
    if (manualTranscriptionInFlight.current || appState === "transcribing") {
      setError("Transcription is already running.");
      setAppState("error");
      return;
    }

    if (isRecording || isFlowBusy) {
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
    appState === "capturing_hotkey" ||
    appState === "transcribing" ||
    appState === "cleaning" ||
    appState === "pasting";
  const safeLatestRecording =
    latestRecording ?? recordingStatus?.latestRecording ?? null;
  const hasPasteableTranscript =
    latestTranscript !== null && latestTranscript.trim().length > 0;
  const currentHotkeyLabel =
    hotkeyStatus?.configured.label ?? settings?.hotkey.label ?? "Loading";
  const registeredHotkeyLabel =
    hotkeyStatus?.registered?.label ?? "Not registered";
  const hotkeyRegistrationStatus = hotkeyStatus?.isRegistered
    ? `Registered (${registeredHotkeyLabel})`
    : (hotkeyStatus?.registrationError ?? "Not registered");

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
              <dd>{currentHotkeyLabel}</dd>
            </div>
            <div>
              <dt>Hotkey status</dt>
              <dd>{hotkeyRegistrationStatus}</dd>
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
            <label htmlFor="global-hotkey">Global hotkey</label>
            <div className="field-row">
              <input
                id="global-hotkey"
                value={
                  isHotkeyCaptureActive
                    ? "Press your new shortcut..."
                    : currentHotkeyLabel
                }
                readOnly
              />
              {isHotkeyCaptureActive ? (
                <button
                  className="secondary-button"
                  type="button"
                  onClick={cancelHotkeyCapture}
                >
                  <Square aria-hidden="true" />
                  Cancel
                </button>
              ) : (
                <>
                  <button
                    type="button"
                    disabled={isRecording || isFlowBusy}
                    onClick={beginHotkeyCapture}
                  >
                    <KeyRound aria-hidden="true" />
                    Change hotkey
                  </button>
                  <button
                    className="secondary-button"
                    type="button"
                    disabled={isRecording || isFlowBusy}
                    onClick={handleResetHotkeyToDefault}
                  >
                    <RotateCcw aria-hidden="true" />
                    Reset default
                  </button>
                </>
              )}
            </div>
            {hotkeyCaptureMessage ? (
              <p className="settings-message">{hotkeyCaptureMessage}</p>
            ) : null}
          </div>
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
