import {
  Activity,
  Clipboard,
  Copy,
  Info,
  KeyRound,
  Mic,
  Save,
  Settings,
  Square,
  Trash2,
  WandSparkles,
} from "lucide-react";
import type { FormEvent } from "react";
import { useEffect, useState } from "react";
import { formatRecordingInfo } from "./lib/recording";
import { cleanupTranscript } from "./lib/transcriptCleanup";
import {
  clearGroqApiKey,
  copyTextToClipboard,
  getAppSettings,
  getAppStatus,
  getGroqApiKeyStatus,
  getLatestRecordingInfo,
  getRecordingStatus,
  pasteText,
  saveAppSettings,
  saveGroqApiKey,
  startRecording,
  stopRecording,
  transcribeLatestRecording,
} from "./lib/tauri";
import { statusLabel } from "./lib/status";
import type {
  AppSettings,
  AppState,
  AppStatus,
  ClipboardError,
  GroqApiKeyStatus,
  GroqTranscriptionError,
  RecordingError,
  RecordingInfo,
  RecordingStatus,
  SettingsError,
} from "./types/app";

export default function App() {
  const [appState, setAppState] = useState<AppState>("loading");
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [apiKeyStatus, setApiKeyStatus] = useState<GroqApiKeyStatus | null>(
    null,
  );
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [groqApiKeyInput, setGroqApiKeyInput] = useState("");
  const [hotkeyLabelInput, setHotkeyLabelInput] = useState("");
  const [settingsMessage, setSettingsMessage] = useState<string | null>(null);
  const [recordingStatus, setRecordingStatus] =
    useState<RecordingStatus | null>(null);
  const [latestRecording, setLatestRecording] = useState<RecordingInfo | null>(
    null,
  );
  const [latestTranscript, setLatestTranscript] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      getAppStatus(),
      getGroqApiKeyStatus(),
      getAppSettings(),
      getRecordingStatus(),
    ])
      .then(
        ([
          appStatus,
          currentApiKeyStatus,
          currentSettings,
          currentRecordingStatus,
        ]) => {
          setStatus(appStatus);
          setApiKeyStatus(currentApiKeyStatus);
          setSettings(currentSettings);
          setHotkeyLabelInput(currentSettings.hotkeyLabel);
          setRecordingStatus(currentRecordingStatus);
          setLatestRecording(currentRecordingStatus.latestRecording);
          setAppState(
            currentRecordingStatus.isRecording ? "recording" : "ready",
          );
        },
      )
      .catch(() => {
        setError("Floe could not load setup state.");
        setAppState("error");
      });
  }, []);

  function applyRecordingStatus(nextStatus: RecordingStatus) {
    setRecordingStatus(nextStatus);
    setLatestRecording(nextStatus.latestRecording);
    setAppState(nextStatus.isRecording ? "recording" : "ready");
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

  async function handleSaveGroqApiKey(event: FormEvent) {
    event.preventDefault();

    try {
      setAppState("checking");
      setError(null);
      setSettingsMessage(null);
      const nextStatus = await saveGroqApiKey(groqApiKeyInput);
      setApiKeyStatus(nextStatus);
      setGroqApiKeyInput("");
      setSettingsMessage("Groq API key saved.");
      setAppState("ready");
    } catch (caught) {
      setSettingsMessage(settingsErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handleClearGroqApiKey() {
    try {
      setAppState("checking");
      setError(null);
      setSettingsMessage(null);
      setApiKeyStatus(await clearGroqApiKey());
      setGroqApiKeyInput("");
      setSettingsMessage("Groq API key cleared.");
      setAppState("ready");
    } catch (caught) {
      setSettingsMessage(settingsErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handleSaveAppSettings(event: FormEvent) {
    event.preventDefault();

    try {
      setAppState("checking");
      setError(null);
      setSettingsMessage(null);
      const savedSettings = await saveAppSettings({
        hotkeyLabel: hotkeyLabelInput,
      });
      setSettings(savedSettings);
      setHotkeyLabelInput(savedSettings.hotkeyLabel);
      setSettingsMessage("App settings saved.");
      setAppState("ready");
    } catch (caught) {
      setSettingsMessage(settingsErrorMessage(caught));
      setAppState("error");
    }
  }

  async function handleStartRecording() {
    try {
      setAppState("checking");
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
      setAppState("checking");
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
      setAppState("checking");
      setError(null);
      applyRecordingStatus(await getRecordingStatus());
    } catch (caught) {
      setError(recordingErrorMessage("status", caught));
      setAppState("error");
    }
  }

  async function handleLatestRecordingInfo() {
    try {
      setAppState("checking");
      setError(null);
      setLatestRecording(await getLatestRecordingInfo());
      applyRecordingStatus(await getRecordingStatus());
    } catch (caught) {
      setError(recordingErrorMessage("latest info", caught));
      setAppState("error");
    }
  }

  async function handleTranscribeLatestRecording() {
    try {
      setAppState("checking");
      setError(null);
      const transcription = await transcribeLatestRecording();
      const finalText = cleanupTranscript(transcription.text);
      setLatestTranscript(finalText);

      if (finalText.trim().length === 0) {
        setAppState("ready");
        return;
      }

      await pasteText(finalText);
      setAppState("ready");
    } catch (caught) {
      const maybeClipboardError = caught as Partial<ClipboardError>;
      setError(
        maybeClipboardError.code === "clipboardUnavailable" ||
          maybeClipboardError.code === "pasteUnavailable"
          ? clipboardErrorMessage(caught)
          : transcriptionErrorMessage(caught),
      );
      setAppState("error");
    }
  }

  async function handleCopyLatestTranscript() {
    if (!latestTranscript || latestTranscript.trim().length === 0) {
      return;
    }

    try {
      setAppState("checking");
      setError(null);
      await copyTextToClipboard(latestTranscript);
      setAppState("ready");
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
      setAppState("checking");
      setError(null);
      await pasteText(latestTranscript);
      setAppState("ready");
    } catch (caught) {
      setError(clipboardErrorMessage(caught));
      setAppState("error");
    }
  }

  const isRecording = recordingStatus?.isRecording ?? false;
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
            <h2>{status?.appName ?? "Floe"} manual flow</h2>
          </div>
          <p>{error ?? status?.message ?? "Loading setup stubs..."}</p>
        </section>

        <section className="settings-panel">
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
              <dt>Global hotkey</dt>
              <dd>{settings?.hotkeyLabel ?? "Loading"}</dd>
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
          <form className="settings-form" onSubmit={handleSaveAppSettings}>
            <label htmlFor="hotkey-label">Global hotkey label</label>
            <div className="field-row">
              <input
                id="hotkey-label"
                type="text"
                value={hotkeyLabelInput}
                onChange={(event) => setHotkeyLabelInput(event.target.value)}
              />
              <button type="submit">
                <Save aria-hidden="true" />
                Save settings
              </button>
            </div>
          </form>
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
              disabled={isRecording}
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
              disabled={isRecording}
              onClick={handleTranscribeLatestRecording}
            >
              <WandSparkles aria-hidden="true" />
              Transcribe + paste
            </button>
            <button
              type="button"
              disabled={!hasPasteableTranscript}
              onClick={handleCopyLatestTranscript}
            >
              <Copy aria-hidden="true" />
              Copy transcript
            </button>
            <button
              type="button"
              disabled={!hasPasteableTranscript}
              onClick={handlePasteLatestTranscript}
            >
              <Clipboard aria-hidden="true" />
              Paste transcript
            </button>
          </div>
          <button
            className="secondary-action"
            type="button"
            disabled
            aria-disabled="true"
          >
            Push-to-talk coming later
          </button>
        </section>
      </div>
    </main>
  );
}
